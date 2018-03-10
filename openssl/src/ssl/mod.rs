//! SSL/TLS support.
//!
//! `SslConnector` and `SslAcceptor` should be used in most cases - they handle
//! configuration of the OpenSSL primitives for you.
//!
//! # Examples
//!
//! To connect as a client to a remote server:
//!
//! ```
//! use openssl::ssl::{SslMethod, SslConnector};
//! use std::io::{Read, Write};
//! use std::net::TcpStream;
//!
//! let connector = SslConnector::builder(SslMethod::tls()).unwrap().build();
//!
//! let stream = TcpStream::connect("google.com:443").unwrap();
//! let mut stream = connector.connect("google.com", stream).unwrap();
//!
//! stream.write_all(b"GET / HTTP/1.0\r\n\r\n").unwrap();
//! let mut res = vec![];
//! stream.read_to_end(&mut res).unwrap();
//! println!("{}", String::from_utf8_lossy(&res));
//! ```
//!
//! To accept connections as a server from remote clients:
//!
//! ```no_run
//! use openssl::ssl::{SslMethod, SslAcceptor, SslStream, SslFiletype};
//! use std::net::{TcpListener, TcpStream};
//! use std::sync::Arc;
//! use std::thread;
//!
//!
//! let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
//! acceptor.set_private_key_file("key.pem", SslFiletype::PEM).unwrap();
//! acceptor.set_certificate_chain_file("certs.pem").unwrap();
//! acceptor.check_private_key().unwrap();
//! let acceptor = Arc::new(acceptor.build());
//!
//! let listener = TcpListener::bind("0.0.0.0:8443").unwrap();
//!
//! fn handle_client(stream: SslStream<TcpStream>) {
//!     // ...
//! }
//!
//! for stream in listener.incoming() {
//!     match stream {
//!         Ok(stream) => {
//!             let acceptor = acceptor.clone();
//!             thread::spawn(move || {
//!                 let stream = acceptor.accept(stream).unwrap();
//!                 handle_client(stream);
//!             });
//!         }
//!         Err(e) => { /* connection failed */ }
//!     }
//! }
//! ```
use ffi;
use foreign_types::{ForeignType, ForeignTypeRef, Opaque};
use libc::{c_char, c_int, c_long, c_uchar, c_uint, c_ulong, c_void};
use std::any::TypeId;
use std::cmp;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fmt;
use std::io;
use std::io::prelude::*;
use std::marker::PhantomData;
use std::mem::{self, ManuallyDrop};
use std::ops::{Deref, DerefMut};
use std::panic::resume_unwind;
use std::path::Path;
use std::ptr;
use std::slice;
use std::str;
use std::sync::Mutex;

use {cvt, cvt_n, cvt_p, init};
use dh::{Dh, DhRef};
use ec::EcKeyRef;
#[cfg(any(all(feature = "v101", ossl101), all(feature = "v102", ossl102)))]
use ec::EcKey;
use x509::{X509, X509Name, X509Ref, X509StoreContextRef, X509VerifyResult};
use x509::store::{X509StoreBuilderRef, X509StoreRef};
#[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
          all(feature = "v111", ossl111)))]
use x509::store::X509Store;
#[cfg(any(ossl102, ossl110))]
use verify::X509VerifyParamRef;
use pkey::{HasPrivate, PKeyRef, Params, Private};
use error::ErrorStack;
use ex_data::Index;
use stack::{Stack, StackRef};
use ssl::bio::BioMethod;
use ssl::error::InnerError;
use ssl::callbacks::*;

pub use ssl::connector::{ConnectConfiguration, SslAcceptor, SslAcceptorBuilder, SslConnector,
                         SslConnectorBuilder};
pub use ssl::error::{Error, ErrorCode, HandshakeError};

mod error;
mod callbacks;
mod connector;
mod bio;
#[cfg(test)]
mod test;

bitflags! {
    /// Options controlling the behavior of an `SslContext`.
    pub struct SslOptions: c_ulong {
        /// Disables a countermeasure against an SSLv3/TLSv1.0 vulnerability affecting CBC ciphers.
        const DONT_INSERT_EMPTY_FRAGMENTS = ffi::SSL_OP_DONT_INSERT_EMPTY_FRAGMENTS;

        /// A "reasonable default" set of options which enables compatibility flags.
        const ALL = ffi::SSL_OP_ALL;

        /// Do not query the MTU.
        ///
        /// Only affects DTLS connections.
        const NO_QUERY_MTU = ffi::SSL_OP_NO_QUERY_MTU;

        /// Enables Cookie Exchange as described in [RFC 4347 Section 4.2.1].
        ///
        /// Only affects DTLS connections.
        ///
        /// [RFC 4347 Section 4.2.1]: https://tools.ietf.org/html/rfc4347#section-4.2.1
        const COOKIE_EXCHANGE = ffi::SSL_OP_COOKIE_EXCHANGE;

        /// Disables the use of session tickets for session resumption.
        const NO_TICKET = ffi::SSL_OP_NO_TICKET;

        /// Always start a new session when performing a renegotiation on the server side.
        const NO_SESSION_RESUMPTION_ON_RENEGOTIATION =
            ffi::SSL_OP_NO_SESSION_RESUMPTION_ON_RENEGOTIATION;

        /// Disables the use of TLS compression.
        const NO_COMPRESSION = ffi::SSL_OP_NO_COMPRESSION;

        /// Allow legacy insecure renegotiation with servers or clients that do not support secure
        /// renegotiation.
        const ALLOW_UNSAFE_LEGACY_RENEGOTIATION =
            ffi::SSL_OP_ALLOW_UNSAFE_LEGACY_RENEGOTIATION;

        /// Creates a new key for each session when using ECDHE.
        ///
        /// This is always enabled in OpenSSL 1.1.0.
        const SINGLE_ECDH_USE = ffi::SSL_OP_SINGLE_ECDH_USE;

        /// Creates a new key for each session when using DHE.
        ///
        /// This is always enabled in OpenSSL 1.1.0.
        const SINGLE_DH_USE = ffi::SSL_OP_SINGLE_DH_USE;

        /// Use the server's preferences rather than the client's when selecting a cipher.
        ///
        /// This has no effect on the client side.
        const CIPHER_SERVER_PREFERENCE = ffi::SSL_OP_CIPHER_SERVER_PREFERENCE;

        /// Disables version rollback attach detection.
        const TLS_ROLLBACK_BUG = ffi::SSL_OP_TLS_ROLLBACK_BUG;

        /// Disables the use of SSLv2.
        const NO_SSLV2 = ffi::SSL_OP_NO_SSLv2;

        /// Disables the use of SSLv3.
        const NO_SSLV3 = ffi::SSL_OP_NO_SSLv3;

        /// Disables the use of TLSv1.0.
        const NO_TLSV1 = ffi::SSL_OP_NO_TLSv1;

        /// Disables the use of TLSv1.1.
        const NO_TLSV1_1 = ffi::SSL_OP_NO_TLSv1_1;

        /// Disables the use of TLSv1.2.
        const NO_TLSV1_2 = ffi::SSL_OP_NO_TLSv1_2;

        /// Disables the use of TLSv1.3.
        ///
        /// Requires the `v111` feature and OpenSSL 1.1.1.
        #[cfg(all(feature = "v111", ossl111))]
        const NO_TLSV1_3 = ffi::SSL_OP_NO_TLSv1_3;

        /// Disables the use of DTLSv1.0
        ///
        /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
        #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
                all(feature = "v111", ossl111)))]
        const NO_DTLSV1 = ffi::SSL_OP_NO_DTLSv1;

        /// Disables the use of DTLSv1.2.
        /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
        #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
                all(feature = "v111", ossl111)))]
        const NO_DTLSV1_2 = ffi::SSL_OP_NO_DTLSv1_2;

        /// Disables the use of all (D)TLS protocol versions.
        ///
        /// This can be used as a mask when whitelisting protocol versions.
        ///
        /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
        ///
        /// # Examples
        ///
        /// Only support TLSv1.2:
        ///
        /// ```rust
        /// use openssl::ssl::SslOptions;
        ///
        /// let options = SslOptions::NO_SSL_MASK & !SslOptions::NO_TLSV1_2;
        /// ```
        #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
                all(feature = "v111", ossl111)))]
        const NO_SSL_MASK = ffi::SSL_OP_NO_SSL_MASK;

        /// Enable TLSv1.3 Compatibility mode.
        ///
        /// This is on by default in OpenSSL 1.1.1. A future version may have this
        /// disabled by default.
        #[cfg(all(feature = "v111", ossl111))]
        const ENABLE_MIDDLEBOX_COMPAT = ffi::SSL_OP_ENABLE_MIDDLEBOX_COMPAT;
    }
}

bitflags! {
    /// Options controlling the behavior of an `SslContext`.
    pub struct SslMode: c_long {
        /// Enables "short writes".
        ///
        /// Normally, a write in OpenSSL will always write out all of the requested data, even if it
        /// requires more than one TLS record or write to the underlying stream. This option will
        /// cause a write to return after writing a single TLS record instead.
        const ENABLE_PARTIAL_WRITE = ffi::SSL_MODE_ENABLE_PARTIAL_WRITE;

        /// Disables a check that the data buffer has not moved between calls when operating in a
        /// nonblocking context.
        const ACCEPT_MOVING_WRITE_BUFFER = ffi::SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER;

        /// Enables automatic retries after TLS session events such as renegotiations or heartbeats.
        ///
        /// By default, OpenSSL will return a `WantRead` error after a renegotiation or heartbeat.
        /// This option will cause OpenSSL to automatically continue processing the requested
        /// operation instead.
        ///
        /// Note that `SslStream::read` and `SslStream::write` will automatically retry regardless
        /// of the state of this option. It only affects `SslStream::ssl_read` and
        /// `SslStream::ssl_write`.
        const AUTO_RETRY = ffi::SSL_MODE_AUTO_RETRY;

        /// Disables automatic chain building when verifying a peer's certificate.
        ///
        /// TLS peers are responsible for sending the entire certificate chain from the leaf to a
        /// trusted root, but some will incorrectly not do so. OpenSSL will try to build the chain
        /// out of certificates it knows of, and this option will disable that behavior.
        const NO_AUTO_CHAIN = ffi::SSL_MODE_NO_AUTO_CHAIN;

        /// Release memory buffers when the session does not need them.
        ///
        /// This saves ~34 KiB of memory for idle streams.
        const RELEASE_BUFFERS = ffi::SSL_MODE_RELEASE_BUFFERS;

        /// Sends the fake `TLS_FALLBACK_SCSV` cipher suite in the ClientHello message of a
        /// handshake.
        ///
        /// This should only be enabled if a client has failed to connect to a server which
        /// attempted to downgrade the protocol version of the session.
        ///
        /// Do not use this unless you know what you're doing!
        #[cfg(not(libressl))]
        const SEND_FALLBACK_SCSV = ffi::SSL_MODE_SEND_FALLBACK_SCSV;
    }
}

/// A type specifying the kind of protocol an `SslContext` will speak.
#[derive(Copy, Clone)]
pub struct SslMethod(*const ffi::SSL_METHOD);

impl SslMethod {
    /// Support all versions of the TLS protocol.
    ///
    /// This corresponds to `TLS_method` on OpenSSL 1.1.0 and `SSLv23_method`
    /// on OpenSSL 1.0.x.
    pub fn tls() -> SslMethod {
        SslMethod(compat::tls_method())
    }

    /// Support all versions of the DTLS protocol.
    ///
    /// This corresponds to `DTLS_method` on OpenSSL 1.1.0 and `DTLSv1_method`
    /// on OpenSSL 1.0.x.
    pub fn dtls() -> SslMethod {
        SslMethod(compat::dtls_method())
    }

    /// Constructs an `SslMethod` from a pointer to the underlying OpenSSL value.
    pub unsafe fn from_ptr(ptr: *const ffi::SSL_METHOD) -> SslMethod {
        SslMethod(ptr)
    }

    /// Returns a pointer to the underlying OpenSSL value.
    pub fn as_ptr(&self) -> *const ffi::SSL_METHOD {
        self.0
    }
}

unsafe impl Sync for SslMethod {}
unsafe impl Send for SslMethod {}

bitflags! {
    /// Options controling the behavior of certificate verification.
    pub struct SslVerifyMode: i32 {
        /// Verifies that the peer's certificate is trusted.
        ///
        /// On the server side, this will cause OpenSSL to request a certificate from the client.
        const PEER = ffi::SSL_VERIFY_PEER;

        /// Disables verification of the peer's certificate.
        ///
        /// On the server side, this will cause OpenSSL to not request a certificate from the
        /// client. On the client side, the certificate will be checked for validity, but the
        /// negotiation will continue regardless of the result of that check.
        const NONE = ffi::SSL_VERIFY_NONE;

        /// On the server side, abort the handshake if the client did not send a certificate.
        ///
        /// This should be paired with `SSL_VERIFY_PEER`. It has no effect on the client side.
        const FAIL_IF_NO_PEER_CERT = ffi::SSL_VERIFY_FAIL_IF_NO_PEER_CERT;
    }
}

bitflags! {
    /// Options controlling the behavior of session caching.
    pub struct SslSessionCacheMode: c_long {
        /// No session caching for the client or server takes place.
        const OFF = ffi::SSL_SESS_CACHE_OFF;

        /// Enable session caching on the client side.
        ///
        /// OpenSSL has no way of identifying the proper session to reuse automatically, so the
        /// application is responsible for setting it explicitly via [`SslRef::set_session`].
        ///
        /// [`SslRef::set_session`]: struct.SslRef.html#method.set_session
        const CLIENT = ffi::SSL_SESS_CACHE_CLIENT;

        /// Enable session caching on the server side.
        ///
        /// This is the default mode.
        const SERVER = ffi::SSL_SESS_CACHE_SERVER;

        /// Enable session caching on both the client and server side.
        const BOTH = ffi::SSL_SESS_CACHE_BOTH;

        /// Disable automatic removal of expired sessions from the session cache.
        const NO_AUTO_CLEAR = ffi::SSL_SESS_CACHE_NO_AUTO_CLEAR;

        /// Disable use of the internal session cache for session lookups.
        const NO_INTERNAL_LOOKUP = ffi::SSL_SESS_CACHE_NO_INTERNAL_LOOKUP;

        /// Disable use of the internal session cache for session storage.
        const NO_INTERNAL_STORE = ffi::SSL_SESS_CACHE_NO_INTERNAL_STORE;

        /// Disable use of the internal session cache for storage and lookup.
        const NO_INTERNAL = ffi::SSL_SESS_CACHE_NO_INTERNAL;
    }
}

/// An identifier of the format of a certificate or key file.
#[derive(Copy, Clone)]
pub struct SslFiletype(c_int);

impl SslFiletype {
    /// Constructs an `SslFiletype` from a raw OpenSSL value.
    pub fn from_raw(raw: c_int) -> SslFiletype {
        SslFiletype(raw)
    }

    /// Returns the raw OpenSSL value represented by this type.
    pub fn as_raw(&self) -> c_int {
        self.0
    }

    /// The PEM format.
    ///
    /// This corresponds to `SSL_FILETYPE_PEM`.
    pub const PEM: SslFiletype = SslFiletype(ffi::SSL_FILETYPE_PEM);

    /// The ASN1 format.
    ///
    /// This corresponds to `SSL_FILETYPE_ASN1`.
    pub const ASN1: SslFiletype = SslFiletype(ffi::SSL_FILETYPE_ASN1);
}

/// An identifier of a certificate status type.
#[derive(Copy, Clone)]
pub struct StatusType(c_int);

impl StatusType {
    /// Constructs a `StatusType` from a raw OpenSSL value.
    pub fn from_raw(raw: c_int) -> StatusType {
        StatusType(raw)
    }

    /// Returns the raw OpenSSL value represented by this type.
    pub fn as_raw(&self) -> c_int {
        self.0
    }

    /// An OSCP status.
    pub const OCSP: StatusType = StatusType(ffi::TLSEXT_STATUSTYPE_ocsp);
}

/// An identifier of a session name type.
#[derive(Copy, Clone)]
pub struct NameType(c_int);

impl NameType {
    /// Constructs a `StatusType` from a raw OpenSSL value.
    pub fn from_raw(raw: c_int) -> StatusType {
        StatusType(raw)
    }

    /// Returns the raw OpenSSL value represented by this type.
    pub fn as_raw(&self) -> c_int {
        self.0
    }

    /// A host name.
    pub const HOST_NAME: NameType = NameType(ffi::TLSEXT_NAMETYPE_host_name);
}

lazy_static! {
    static ref INDEXES: Mutex<HashMap<TypeId, c_int>> = Mutex::new(HashMap::new());
    static ref SSL_INDEXES: Mutex<HashMap<TypeId, c_int>> = Mutex::new(HashMap::new());
}

// Creates a static index for user data of type T
// Registers a destructor for the data which will be called
// when context is freed
fn get_callback_idx<T: 'static>() -> c_int {
    *INDEXES
        .lock()
        .unwrap()
        .entry(TypeId::of::<T>())
        .or_insert_with(|| get_new_idx::<T>())
}

fn get_ssl_callback_idx<T: 'static>() -> c_int {
    *SSL_INDEXES
        .lock()
        .unwrap()
        .entry(TypeId::of::<T>())
        .or_insert_with(|| get_new_ssl_idx::<T>())
}

unsafe extern "C" fn free_data_box<T>(
    _parent: *mut c_void,
    ptr: *mut c_void,
    _ad: *mut ffi::CRYPTO_EX_DATA,
    _idx: c_int,
    _argl: c_long,
    _argp: *mut c_void,
) {
    if !ptr.is_null() {
        Box::<T>::from_raw(ptr as *mut T);
    }
}

fn get_new_idx<T>() -> c_int {
    unsafe {
        let idx = compat::get_new_idx(free_data_box::<T>);
        assert!(idx >= 0);
        idx
    }
}

fn get_new_ssl_idx<T>() -> c_int {
    unsafe {
        let idx = compat::get_new_ssl_idx(free_data_box::<T>);
        assert!(idx >= 0);
        idx
    }
}

/// An error returned from the SNI callback.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SniError(c_int);

impl SniError {
    /// Abort the handshake with a fatal alert.
    pub const ALERT_FATAL: SniError = SniError(ffi::SSL_TLSEXT_ERR_ALERT_FATAL);

    /// Send a warning alert to the client and continue the handshake.
    pub const ALERT_WARNING: SniError = SniError(ffi::SSL_TLSEXT_ERR_ALERT_WARNING);

    pub const NOACK: SniError = SniError(ffi::SSL_TLSEXT_ERR_NOACK);
}

/// An SSL/TLS alert.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SslAlert(c_int);

impl SslAlert {
    /// Alert 112 - `unrecognized_name`.
    pub const UNRECOGNIZED_NAME: SslAlert = SslAlert(ffi::SSL_AD_UNRECOGNIZED_NAME);
}

/// An error returned from an ALPN selection callback.
///
/// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
#[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
          all(feature = "v111", ossl111)))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct AlpnError(c_int);

#[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
          all(feature = "v111", ossl111)))]
impl AlpnError {
    /// Terminate the handshake with a fatal alert.
    ///
    /// Requires OpenSSL 1.1.0 or 1.1.1 and the corresponding Cargo feature.
    #[cfg(any(all(feature = "v110", ossl110), all(feature = "v111", ossl111)))]
    pub const ALERT_FATAL: AlpnError = AlpnError(ffi::SSL_TLSEXT_ERR_ALERT_FATAL);

    /// Do not select a protocol, but continue the handshake.
    pub const NOACK: AlpnError = AlpnError(ffi::SSL_TLSEXT_ERR_NOACK);
}

/// An SSL/TLS protocol version.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct SslVersion(c_int);

impl SslVersion {
    /// SSLv3
    pub const SSL3: SslVersion = SslVersion(ffi::SSL3_VERSION);

    /// TLSv1.0
    pub const TLS1: SslVersion = SslVersion(ffi::TLS1_VERSION);

    /// TLSv1.1
    pub const TLS1_1: SslVersion = SslVersion(ffi::TLS1_1_VERSION);

    /// TLSv1.2
    pub const TLS1_2: SslVersion = SslVersion(ffi::TLS1_2_VERSION);

    /// TLSv1.3
    ///
    /// Requires OpenSSL 1.1.1 and the corresponding Cargo feature.
    #[cfg(all(feature = "v111", ossl111))]
    pub const TLS1_3: SslVersion = SslVersion(ffi::TLS1_3_VERSION);
}

/// A standard implementation of protocol selection for Application Layer Protocol Negotiation
/// (ALPN).
///
/// `server` should contain the server's list of supported protocols and `client` the client's. They
/// must both be in the ALPN wire format. See the documentation for
/// [`SslContextBuilder::set_alpn_protos`] for details.
///
/// It will select the first protocol supported by the server which is also supported by the client.
///
/// This corresponds to [`SSL_select_next_proto`].
///
/// [`SslContextBuilder::set_alpn_protos`]: struct.SslContextBuilder.html#method.set_alpn_protos
/// [`SSL_select_next_proto`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_set_alpn_protos.html
pub fn select_next_proto<'a>(server: &[u8], client: &'a [u8]) -> Option<&'a [u8]> {
    unsafe {
        let mut out = ptr::null_mut();
        let mut outlen = 0;
        let r = ffi::SSL_select_next_proto(
            &mut out,
            &mut outlen,
            server.as_ptr(),
            server.len() as c_uint,
            client.as_ptr(),
            client.len() as c_uint,
        );
        if r == ffi::OPENSSL_NPN_NEGOTIATED {
            Some(slice::from_raw_parts(out as *const u8, outlen as usize))
        } else {
            None
        }
    }
}

/// A builder for `SslContext`s.
pub struct SslContextBuilder(SslContext);

impl SslContextBuilder {
    /// Creates a new `SslContextBuilder`.
    ///
    /// This corresponds to [`SSL_CTX_new`].
    ///
    /// [`SSL_CTX_new`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_new.html
    pub fn new(method: SslMethod) -> Result<SslContextBuilder, ErrorStack> {
        unsafe {
            init();
            let ctx = cvt_p(ffi::SSL_CTX_new(method.as_ptr()))?;

            Ok(SslContextBuilder::from_ptr(ctx))
        }
    }

    /// Creates an `SslContextBuilder` from a pointer to a raw OpenSSL value.
    pub unsafe fn from_ptr(ctx: *mut ffi::SSL_CTX) -> SslContextBuilder {
        SslContextBuilder(SslContext::from_ptr(ctx))
    }

    /// Returns a pointer to the raw OpenSSL value.
    pub fn as_ptr(&self) -> *mut ffi::SSL_CTX {
        self.0.as_ptr()
    }

    /// Configures the certificate verification method for new connections.
    ///
    /// This corresponds to [`SSL_CTX_set_verify`].
    ///
    /// [`SSL_CTX_set_verify`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_set_verify.html
    pub fn set_verify(&mut self, mode: SslVerifyMode) {
        unsafe {
            ffi::SSL_CTX_set_verify(self.as_ptr(), mode.bits as c_int, None);
        }
    }

    /// Configures the certificate verification method for new connections and
    /// registers a verification callback.
    ///
    /// The callback is passed a boolean indicating if OpenSSL's internal verification succeeded as
    /// well as a reference to the `X509StoreContext` which can be used to examine the certificate
    /// chain. It should return a boolean indicating if verification succeeded.
    ///
    /// This corresponds to [`SSL_CTX_set_verify`].
    ///
    /// [`SSL_CTX_set_verify`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_set_verify.html
    pub fn set_verify_callback<F>(&mut self, mode: SslVerifyMode, verify: F)
    where
        F: Fn(bool, &mut X509StoreContextRef) -> bool + 'static + Sync + Send,
    {
        unsafe {
            let verify = Box::new(verify);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                mem::transmute(verify),
            );
            ffi::SSL_CTX_set_verify(self.as_ptr(), mode.bits as c_int, Some(raw_verify::<F>));
        }
    }

    /// Configures the server name indication (SNI) callback for new connections.
    ///
    /// SNI is used to allow a single server to handle requests for multiple domains, each of which
    /// has its own certificate chain and configuration.
    ///
    /// Obtain the server name with the `servername` method and then set the corresponding context
    /// with `set_ssl_context`
    ///
    /// This corresponds to [`SSL_CTX_set_tlsext_servername_callback`].
    ///
    /// [`SSL_CTX_set_tlsext_servername_callback`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_set_tlsext_servername_callback.html
    pub fn set_servername_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, &mut SslAlert) -> Result<(), SniError> + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                mem::transmute(callback),
            );
            let f: extern "C" fn(_, _, _) -> _ = raw_sni::<F>;
            let f: extern "C" fn() = mem::transmute(f);
            ffi::SSL_CTX_set_tlsext_servername_callback(self.as_ptr(), Some(f));
        }
    }

    /// Sets the certificate verification depth.
    ///
    /// If the peer's certificate chain is longer than this value, verification will fail.
    ///
    /// This corresponds to [`SSL_CTX_set_verify_depth`].
    ///
    /// [`SSL_CTX_set_verify_depth`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_set_verify_depth.html
    pub fn set_verify_depth(&mut self, depth: u32) {
        unsafe {
            ffi::SSL_CTX_set_verify_depth(self.as_ptr(), depth as c_int);
        }
    }

    /// Sets a custom certificate store for verifying peer certificates.
    ///
    /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
    ///
    /// This corresponds to [`SSL_CTX_set0_verify_cert_store`].
    ///
    /// [`SSL_CTX_set0_verify_cert_store`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_set0_verify_cert_store.html
    #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
              all(feature = "v111", ossl111)))]
    pub fn set_verify_cert_store(&mut self, cert_store: X509Store) -> Result<(), ErrorStack> {
        unsafe {
            let ptr = cert_store.as_ptr();
            cvt(ffi::SSL_CTX_set0_verify_cert_store(self.as_ptr(), ptr) as c_int)?;
            mem::forget(cert_store);

            Ok(())
        }
    }

    /// Controls read ahead behavior.
    ///
    /// If enabled, OpenSSL will read as much data as is available from the underlying stream,
    /// instead of a single record at a time.
    ///
    /// It has no effect when used with DTLS.
    ///
    /// This corresponds to [`SSL_CTX_set_read_ahead`].
    ///
    /// [`SSL_CTX_set_read_ahead`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_set_read_ahead.html
    pub fn set_read_ahead(&mut self, read_ahead: bool) {
        unsafe {
            ffi::SSL_CTX_set_read_ahead(self.as_ptr(), read_ahead as c_long);
        }
    }

    /// Sets the mode used by the context, returning the previous mode.
    ///
    /// This corresponds to [`SSL_CTX_set_mode`].
    ///
    /// [`SSL_CTX_set_mode`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_set_mode.html
    pub fn set_mode(&mut self, mode: SslMode) -> SslMode {
        unsafe {
            let bits = ffi::SSL_CTX_set_mode(self.as_ptr(), mode.bits());
            SslMode { bits }
        }
    }

    /// Sets the parameters to be used during ephemeral Diffie-Hellman key exchange.
    ///
    /// This corresponds to [`SSL_CTX_set_tmp_dh`].
    ///
    /// [`SSL_CTX_set_tmp_dh`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_set_tmp_dh.html
    pub fn set_tmp_dh(&mut self, dh: &DhRef<Params>) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::SSL_CTX_set_tmp_dh(self.as_ptr(), dh.as_ptr()) as c_int).map(|_| ()) }
    }

    /// Sets the callback which will generate parameters to be used during ephemeral Diffie-Hellman
    /// key exchange.
    ///
    /// The callback is provided with a reference to the `Ssl` for the session, as well as a boolean
    /// indicating if the selected cipher is export-grade, and the key length. The export and key
    /// length options are archaic and should be ignored in almost all cases.
    ///
    /// This corresponds to [`SSL_CTX_set_tmp_dh_callback`].
    ///
    /// [`SSL_CTX_set_tmp_dh_callback`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_set_tmp_dh.html
    pub fn set_tmp_dh_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, bool, u32) -> Result<Dh<Params>, ErrorStack> + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                Box::into_raw(callback) as *mut c_void,
            );
            let f: unsafe extern "C" fn(_, _, _) -> _ = raw_tmp_dh::<F>;
            ffi::SSL_CTX_set_tmp_dh_callback(self.as_ptr(), f);
        }
    }

    /// Sets the parameters to be used during ephemeral elliptic curve Diffie-Hellman key exchange.
    ///
    /// This corresponds to `SSL_CTX_set_tmp_ecdh`.
    pub fn set_tmp_ecdh(&mut self, key: &EcKeyRef<Params>) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::SSL_CTX_set_tmp_ecdh(self.as_ptr(), key.as_ptr()) as c_int).map(|_| ()) }
    }

    /// Sets the callback which will generate parameters to be used during ephemeral elliptic curve
    /// Diffie-Hellman key exchange.
    ///
    /// The callback is provided with a reference to the `Ssl` for the session, as well as a boolean
    /// indicating if the selected cipher is export-grade, and the key length. The export and key
    /// length options are archaic and should be ignored in almost all cases.
    ///
    /// Requires the `v101` feature and OpenSSL 1.0.1, or the `v102` feature and OpenSSL 1.0.2.
    ///
    /// This corresponds to `SSL_CTX_set_tmp_ecdh_callback`.
    #[cfg(any(all(feature = "v101", ossl101), all(feature = "v102", ossl102)))]
    pub fn set_tmp_ecdh_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, bool, u32) -> Result<EcKey<Params>, ErrorStack> + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                Box::into_raw(callback) as *mut c_void,
            );
            let f: unsafe extern "C" fn(_, _, _) -> _ = raw_tmp_ecdh::<F>;
            ffi::SSL_CTX_set_tmp_ecdh_callback(self.as_ptr(), f);
        }
    }

    /// Use the default locations of trusted certificates for verification.
    ///
    /// These locations are read from the `SSL_CERT_FILE` and `SSL_CERT_DIR` environment variables
    /// if present, or defaults specified at OpenSSL build time otherwise.
    ///
    /// This corresponds to [`SSL_CTX_set_default_verify_paths`].
    ///
    /// [`SSL_CTX_set_default_verify_paths`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_set_default_verify_paths.html
    pub fn set_default_verify_paths(&mut self) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::SSL_CTX_set_default_verify_paths(self.as_ptr())).map(|_| ()) }
    }

    /// Loads trusted root certificates from a file.
    ///
    /// The file should contain a sequence of PEM-formatted CA certificates.
    ///
    /// This corresponds to [`SSL_CTX_set_default_verify_file`].
    ///
    /// [`SSL_CTX_set_default_verify_file`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_set_default_verify_paths.html
    pub fn set_ca_file<P: AsRef<Path>>(&mut self, file: P) -> Result<(), ErrorStack> {
        let file = CString::new(file.as_ref().as_os_str().to_str().unwrap()).unwrap();
        unsafe {
            cvt(ffi::SSL_CTX_load_verify_locations(
                self.as_ptr(),
                file.as_ptr() as *const _,
                ptr::null(),
            )).map(|_| ())
        }
    }

    /// Sets the list of CA names sent to the client.
    ///
    /// The CA certificates must still be added to the trust root - they are not automatically set
    /// as trusted by this method.
    ///
    /// This corresponds to [`SSL_CTX_set_client_CA_list`].
    ///
    /// [`SSL_CTX_set_client_CA_list`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_set_client_CA_list.html
    pub fn set_client_ca_list(&mut self, list: Stack<X509Name>) {
        unsafe {
            ffi::SSL_CTX_set_client_CA_list(self.as_ptr(), list.as_ptr());
            mem::forget(list);
        }
    }

    /// Set the context identifier for sessions.
    ///
    /// This value identifies the server's session cache to clients, telling them when they're
    /// able to reuse sessions. It should be be set to a unique value per server, unless multiple
    /// servers share a session cache.
    ///
    /// This value should be set when using client certificates, or each request will fail its
    /// handshake and need to be restarted.
    ///
    /// This corresponds to [`SSL_CTX_set_session_id_context`].
    ///
    /// [`SSL_CTX_set_session_id_context`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_set_session_id_context.html
    pub fn set_session_id_context(&mut self, sid_ctx: &[u8]) -> Result<(), ErrorStack> {
        unsafe {
            assert!(sid_ctx.len() <= c_uint::max_value() as usize);
            cvt(ffi::SSL_CTX_set_session_id_context(
                self.as_ptr(),
                sid_ctx.as_ptr(),
                sid_ctx.len() as c_uint,
            )).map(|_| ())
        }
    }

    /// Loads a leaf certificate from a file.
    ///
    /// Only a single certificate will be loaded - use `add_extra_chain_cert` to add the remainder
    /// of the certificate chain, or `set_certificate_chain_file` to load the entire chain from a
    /// single file.
    ///
    /// This corresponds to [`SSL_CTX_use_certificate_file`].
    ///
    /// [`SSL_CTX_use_certificate_file`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_use_certificate_file.html
    pub fn set_certificate_file<P: AsRef<Path>>(
        &mut self,
        file: P,
        file_type: SslFiletype,
    ) -> Result<(), ErrorStack> {
        let file = CString::new(file.as_ref().as_os_str().to_str().unwrap()).unwrap();
        unsafe {
            cvt(ffi::SSL_CTX_use_certificate_file(
                self.as_ptr(),
                file.as_ptr() as *const _,
                file_type.as_raw(),
            )).map(|_| ())
        }
    }

    /// Loads a certificate chain from a file.
    ///
    /// The file should contain a sequence of PEM-formatted certificates, the first being the leaf
    /// certificate, and the remainder forming the chain of certificates up to and including the
    /// trusted root certificate.
    ///
    /// This corresponds to [`SSL_CTX_use_certificate_chain_file`].
    ///
    /// [`SSL_CTX_use_certificate_chain_file`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_use_certificate_file.html
    pub fn set_certificate_chain_file<P: AsRef<Path>>(
        &mut self,
        file: P,
    ) -> Result<(), ErrorStack> {
        let file = CString::new(file.as_ref().as_os_str().to_str().unwrap()).unwrap();
        unsafe {
            cvt(ffi::SSL_CTX_use_certificate_chain_file(
                self.as_ptr(),
                file.as_ptr() as *const _,
            )).map(|_| ())
        }
    }

    /// Sets the leaf certificate.
    ///
    /// Use `add_extra_chain_cert` to add the remainder of the certificate chain.
    ///
    /// This corresponds to [`SSL_CTX_use_certificate`].
    ///
    /// [`SSL_CTX_use_certificate`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_use_certificate_file.html
    pub fn set_certificate(&mut self, cert: &X509Ref) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::SSL_CTX_use_certificate(self.as_ptr(), cert.as_ptr())).map(|_| ()) }
    }

    /// Appends a certificate to the certificate chain.
    ///
    /// This chain should contain all certificates necessary to go from the certificate specified by
    /// `set_certificate` to a trusted root.
    ///
    /// This corresponds to [`SSL_CTX_add_extra_chain_cert`].
    ///
    /// [`SSL_CTX_add_extra_chain_cert`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_add_extra_chain_cert.html
    pub fn add_extra_chain_cert(&mut self, cert: X509) -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::SSL_CTX_add_extra_chain_cert(self.as_ptr(), cert.as_ptr()) as c_int)?;
            mem::forget(cert);
            Ok(())
        }
    }

    /// Loads the private key from a file.
    ///
    /// This corresponds to [`SSL_CTX_use_PrivateKey_file`].
    ///
    /// [`SSL_CTX_use_PrivateKey_file`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_use_PrivateKey_file.html
    pub fn set_private_key_file<P: AsRef<Path>>(
        &mut self,
        file: P,
        file_type: SslFiletype,
    ) -> Result<(), ErrorStack> {
        let file = CString::new(file.as_ref().as_os_str().to_str().unwrap()).unwrap();
        unsafe {
            cvt(ffi::SSL_CTX_use_PrivateKey_file(
                self.as_ptr(),
                file.as_ptr() as *const _,
                file_type.as_raw(),
            )).map(|_| ())
        }
    }

    /// Sets the private key.
    ///
    /// This corresponds to [`SSL_CTX_use_PrivateKey`].
    ///
    /// [`SSL_CTX_use_PrivateKey`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_use_PrivateKey_file.html
    pub fn set_private_key<T>(&mut self, key: &PKeyRef<T>) -> Result<(), ErrorStack>
    where
        T: HasPrivate,
    {
        unsafe { cvt(ffi::SSL_CTX_use_PrivateKey(self.as_ptr(), key.as_ptr())).map(|_| ()) }
    }

    /// Sets the list of supported ciphers.
    ///
    /// See [`ciphers`] for details on the format.
    ///
    /// This corresponds to [`SSL_CTX_set_cipher_list`].
    ///
    /// [`ciphers`]: https://www.openssl.org/docs/man1.1.0/apps/ciphers.html
    /// [`SSL_CTX_set_cipher_list`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_get_client_ciphers.html
    pub fn set_cipher_list(&mut self, cipher_list: &str) -> Result<(), ErrorStack> {
        let cipher_list = CString::new(cipher_list).unwrap();
        unsafe {
            cvt(ffi::SSL_CTX_set_cipher_list(
                self.as_ptr(),
                cipher_list.as_ptr() as *const _,
            )).map(|_| ())
        }
    }

    /// Enables ECDHE key exchange with an automatically chosen curve list.
    ///
    /// Requires the `v102` feature and OpenSSL 1.0.2.
    ///
    /// This corresponds to [`SSL_CTX_set_ecdh_auto`].
    ///
    /// [`SSL_CTX_set_ecdh_auto`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_set_ecdh_auto.html
    #[cfg(all(feature = "v102", any(ossl102, libressl)))]
    pub fn set_ecdh_auto(&mut self, onoff: bool) -> Result<(), ErrorStack> {
        self._set_ecdh_auto(onoff)
    }

    #[cfg(any(ossl102, libressl))]
    fn _set_ecdh_auto(&mut self, onoff: bool) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::SSL_CTX_set_ecdh_auto(self.as_ptr(), onoff as c_int)).map(|_| ()) }
    }

    /// Sets the options used by the context, returning the old set.
    ///
    /// This corresponds to [`SSL_CTX_set_options`].
    ///
    /// # Note
    ///
    /// This *enables* the specified options, but does not disable unspecified options. Use
    /// `clear_options` for that.
    ///
    /// [`SSL_CTX_set_options`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_set_options.html
    pub fn set_options(&mut self, option: SslOptions) -> SslOptions {
        let bits = unsafe { compat::SSL_CTX_set_options(self.as_ptr(), option.bits()) };
        SslOptions { bits }
    }

    /// Returns the options used by the context.
    ///
    /// This corresponds to [`SSL_CTX_get_options`].
    ///
    /// [`SSL_CTX_get_options`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_set_options.html
    pub fn options(&self) -> SslOptions {
        let bits = unsafe { compat::SSL_CTX_get_options(self.as_ptr()) };
        SslOptions { bits }
    }

    /// Clears the options used by the context, returning the old set.
    ///
    /// This corresponds to [`SSL_CTX_clear_options`].
    ///
    /// [`SSL_CTX_clear_options`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_set_options.html
    pub fn clear_options(&mut self, option: SslOptions) -> SslOptions {
        let bits = unsafe { compat::SSL_CTX_clear_options(self.as_ptr(), option.bits()) };
        SslOptions { bits }
    }

    /// Sets the minimum supported protocol version.
    ///
    /// A value of `None` will enable protocol versions down the the lowest version supported by
    /// OpenSSL.
    ///
    /// This corresponds to [`SSL_CTX_set_min_proto_version`].
    ///
    /// Requires OpenSSL 1.1.0 or 1.1.1 and the corresponding Cargo feature.
    ///
    /// [`SSL_CTX_set_min_proto_version`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_set_min_proto_version.html
    #[cfg(any(all(feature = "v110", ossl110), all(feature = "v111", ossl111)))]
    pub fn set_min_proto_version(&mut self, version: Option<SslVersion>) -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::SSL_CTX_set_min_proto_version(
                self.as_ptr(),
                version.map_or(0, |v| v.0),
            )).map(|_| ())
        }
    }

    /// Sets the maximum supported protocol version.
    ///
    /// A value of `None` will enable protocol versions down the the highest version supported by
    /// OpenSSL.
    ///
    /// This corresponds to [`SSL_CTX_set_max_proto_version`].
    ///
    /// Requires OpenSSL 1.1.0 or 1.1.1 and the corresponding Cargo feature.
    ///
    /// [`SSL_CTX_set_max_proto_version`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_set_min_proto_version.html
    #[cfg(any(all(feature = "v110", ossl110), all(feature = "v111", ossl111)))]
    pub fn set_max_proto_version(&mut self, version: Option<SslVersion>) -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::SSL_CTX_set_max_proto_version(
                self.as_ptr(),
                version.map_or(0, |v| v.0),
            )).map(|_| ())
        }
    }

    /// Gets the minimum supported protocol version.
    ///
    /// A value of `None` indicates that all versions down the the lowest version supported by
    /// OpenSSL are enabled.
    ///
    /// This corresponds to [`SSL_CTX_get_min_proto_version`].
    ///
    /// Requires OpenSSL 1.1.0 or 1.1.1 and the corresponding Cargo feature.
    ///
    /// [`SSL_CTX_get_min_proto_version`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_set_min_proto_version.html
    #[cfg(any(all(feature = "v110", ossl110), all(feature = "v111", ossl111)))]
    pub fn min_proto_version(&mut self) -> Option<SslVersion> {
        unsafe {
            let r = ffi::SSL_CTX_get_min_proto_version(self.as_ptr());
            if r == 0 {
                None
            } else {
                Some(SslVersion(r))
            }
        }
    }

    /// Gets the maximum supported protocol version.
    ///
    /// A value of `None` indicates that all versions down the the highest version supported by
    /// OpenSSL are enabled.
    ///
    /// This corresponds to [`SSL_CTX_get_max_proto_version`].
    ///
    /// Requires OpenSSL 1.1.0 or 1.1.1 and the corresponding Cargo feature.
    ///
    /// [`SSL_CTX_get_max_proto_version`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_set_min_proto_version.html
    #[cfg(any(all(feature = "v110", ossl110), all(feature = "v111", ossl111)))]
    pub fn max_proto_version(&mut self) -> Option<SslVersion> {
        unsafe {
            let r = ffi::SSL_CTX_get_max_proto_version(self.as_ptr());
            if r == 0 {
                None
            } else {
                Some(SslVersion(r))
            }
        }
    }

    /// Sets the protocols to sent to the server for Application Layer Protocol Negotiation (ALPN).
    ///
    /// The input must be in ALPN "wire format". It consists of a sequence of supported protocol
    /// names prefixed by their byte length. For example, the protocol list consisting of `spdy/1`
    /// and `http/1.1` is encoded as `b"\x06spdy/1\x08http/1.1"`. The protocols are ordered by
    /// preference.
    ///
    /// This corresponds to [`SSL_CTX_set_alpn_protos`].
    ///
    /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
    ///
    /// [`SSL_CTX_set_alpn_protos`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_set_alpn_protos.html
    #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
              all(feature = "v111", ossl111)))]
    pub fn set_alpn_protos(&mut self, protocols: &[u8]) -> Result<(), ErrorStack> {
        unsafe {
            assert!(protocols.len() <= c_uint::max_value() as usize);
            let r = ffi::SSL_CTX_set_alpn_protos(
                self.as_ptr(),
                protocols.as_ptr(),
                protocols.len() as c_uint,
            );
            // fun fact, SSL_CTX_set_alpn_protos has a reversed return code D:
            if r == 0 {
                Ok(())
            } else {
                Err(ErrorStack::get())
            }
        }
    }

    /// Sets the callback used by a server to select a protocol for Application Layer Protocol
    /// Negotiation (ALPN).
    ///
    /// The callback is provided with the client's protocol list in ALPN wire format. See the
    /// documentation for [`SslContextBuilder::set_alpn_protos`] for details. It should return one
    /// of those protocols on success. The [`select_next_proto`] function implements the standard
    /// protocol selection algorithm.
    ///
    /// This corresponds to [`SSL_CTX_set_alpn_select_cb`].
    ///
    /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
    ///
    /// [`SslContextBuilder::set_alpn_protos`]: struct.SslContextBuilder.html#method.set_alpn_protos
    /// [`select_next_proto`]: fn.select_next_proto.html
    /// [`SSL_CTX_set_alpn_select_cb`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_set_alpn_protos.html
    #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
              all(feature = "v111", ossl111)))]
    pub fn set_alpn_select_callback<F>(&mut self, callback: F)
    where
        F: for<'a> Fn(&mut SslRef, &'a [u8]) -> Result<&'a [u8], AlpnError> + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                Box::into_raw(callback) as *mut c_void,
            );
            ffi::SSL_CTX_set_alpn_select_cb(
                self.as_ptr(),
                callbacks::raw_alpn_select::<F>,
                ptr::null_mut(),
            );
        }
    }

    /// Checks for consistency between the private key and certificate.
    ///
    /// This corresponds to [`SSL_CTX_check_private_key`].
    ///
    /// [`SSL_CTX_check_private_key`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_check_private_key.html
    pub fn check_private_key(&self) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::SSL_CTX_check_private_key(self.as_ptr())).map(|_| ()) }
    }

    /// Returns a shared reference to the context's certificate store.
    ///
    /// This corresponds to [`SSL_CTX_get_cert_store`].
    ///
    /// [`SSL_CTX_get_cert_store`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_get_cert_store.html
    pub fn cert_store(&self) -> &X509StoreBuilderRef {
        unsafe { X509StoreBuilderRef::from_ptr(ffi::SSL_CTX_get_cert_store(self.as_ptr())) }
    }

    /// Returns a mutable reference to the context's certificate store.
    ///
    /// This corresponds to [`SSL_CTX_get_cert_store`].
    ///
    /// [`SSL_CTX_get_cert_store`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_get_cert_store.html
    pub fn cert_store_mut(&mut self) -> &mut X509StoreBuilderRef {
        unsafe { X509StoreBuilderRef::from_ptr_mut(ffi::SSL_CTX_get_cert_store(self.as_ptr())) }
    }

    /// Sets the callback dealing with OCSP stapling.
    ///
    /// On the client side, this callback is responsible for validating the OCSP status response
    /// returned by the server. The status may be retrieved with the `SslRef::ocsp_status` method.
    /// A response of `Ok(true)` indicates that the OCSP status is valid, and a response of
    /// `Ok(false)` indicates that the OCSP status is invalid and the handshake should be
    /// terminated.
    ///
    /// On the server side, this callback is resopnsible for setting the OCSP status response to be
    /// returned to clients. The status may be set with the `SslRef::set_ocsp_status` method. A
    /// response of `Ok(true)` indicates that the OCSP status should be returned to the client, and
    /// `Ok(false)` indicates that the status should not be returned to the client.
    ///
    /// This corresponds to [`SSL_CTX_set_tlsext_status_cb`].
    ///
    /// [`SSL_CTX_set_tlsext_status_cb`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_set_tlsext_status_cb.html
    pub fn set_status_callback<F>(&mut self, callback: F) -> Result<(), ErrorStack>
    where
        F: Fn(&mut SslRef) -> Result<bool, ErrorStack> + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                Box::into_raw(callback) as *mut c_void,
            );
            let f: unsafe extern "C" fn(_, _) -> _ = raw_tlsext_status::<F>;
            cvt(ffi::SSL_CTX_set_tlsext_status_cb(self.as_ptr(), Some(f)) as c_int).map(|_| ())
        }
    }

    /// Sets the callback for providing an identity and pre-shared key for a TLS-PSK client.
    ///
    /// The callback will be called with the SSL context, an identity hint if one was provided
    /// by the server, a mutable slice for each of the identity and pre-shared key bytes. The
    /// identity must be written as a null-terminated C string.
    ///
    /// This corresponds to [`SSL_CTX_set_psk_client_callback`].
    ///
    /// [`SSL_CTX_set_psk_client_callback`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_set_psk_client_callback.html
    #[cfg(not(osslconf = "OPENSSL_NO_PSK"))]
    pub fn set_psk_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, Option<&[u8]>, &mut [u8], &mut [u8]) -> Result<usize, ErrorStack>
            + 'static
            + Sync
            + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                mem::transmute(callback),
            );
            ffi::SSL_CTX_set_psk_client_callback(self.as_ptr(), Some(raw_psk::<F>))
        }
    }

    /// Sets the callback which is called when new sessions are negotiated.
    ///
    /// This can be used by clients to implement session caching. While in TLSv1.2 the session is
    /// available to access via [`SslRef::session`] immediately after the handshake completes, this
    /// is not the case for TLSv1.3. There, a session is not generally available immediately, and
    /// the server may provide multiple session tokens to the client over a single session. The new
    /// session callback is a portable way to deal with both cases.
    ///
    /// Note that session caching must be enabled for the callback to be invoked, and it defaults
    /// off for clients. [`set_session_cache_mode`] controls that behavior.
    ///
    /// This corresponds to [`SSL_CTX_sess_set_new_cb`].
    ///
    /// [`SslRef::session`]: struct.SslRef.html#method.session
    /// [`set_session_cache_mode`]: #method.set_session_cache_mode
    /// [`SSL_CTX_sess_set_new_cb`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_sess_set_new_cb.html
    pub fn set_new_session_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, SslSession) + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                Box::into_raw(callback) as *mut _,
            );
            ffi::SSL_CTX_sess_set_new_cb(self.as_ptr(), Some(callbacks::raw_new_session::<F>));
        }
    }

    /// Sets the callback which is called when sessions are removed from the context.
    ///
    /// Sessions can be removed because they have timed out or because they are considered faulty.
    ///
    /// This corresponds to [`SSL_CTX_sess_set_remove_cb`].
    ///
    /// [`SSL_CTX_sess_set_remove_cb`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_sess_set_new_cb.html
    pub fn set_remove_session_callback<F>(&mut self, callback: F)
    where
        F: Fn(&SslContextRef, &SslSessionRef) + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                Box::into_raw(callback) as *mut _,
            );
            ffi::SSL_CTX_sess_set_remove_cb(
                self.as_ptr(),
                Some(callbacks::raw_remove_session::<F>),
            );
        }
    }

    /// Sets the callback which is called when a client proposed to resume a session but it was not
    /// found in the internal cache.
    ///
    /// The callback is passed a reference to the session ID provided by the client. It should
    /// return the session corresponding to that ID if available. This is only used for servers, not
    /// clients.
    ///
    /// This corresponds to [`SSL_CTX_sess_set_get_cb`].
    ///
    /// # Safety
    ///
    /// The returned `SslSession` must not be associated with a different `SslContext`.
    ///
    /// [`SSL_CTX_sess_set_get_cb`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_sess_set_new_cb.html
    pub unsafe fn set_get_session_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, &[u8]) -> Option<SslSession> + 'static + Sync + Send,
    {
        let callback = Box::new(callback);
        ffi::SSL_CTX_set_ex_data(
            self.as_ptr(),
            get_callback_idx::<F>(),
            Box::into_raw(callback) as *mut _,
        );
        ffi::SSL_CTX_sess_set_get_cb(self.as_ptr(), Some(callbacks::raw_get_session::<F>));
    }

    /// Sets the TLS key logging callback.
    ///
    /// The callback is invoked whenever TLS key material is generated, and is passed a line of NSS
    /// SSLKEYLOGFILE-formatted text. This can be used by tools like Wireshark to decrypt message
    /// traffic. The line does not contain a trailing newline.
    ///
    /// Requires OpenSSL 1.1.1 and the corresponding Cargo feature.
    ///
    /// This corresponds to [`SSL_CTX_set_keylog_callback`].
    ///
    /// [`SSL_CTX_set_keylog_callback`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_set_keylog_callback.html
    #[cfg(all(feature = "v111", ossl111))]
    pub fn set_keylog_callback<F>(&mut self, callback: F)
    where
        F: Fn(&SslRef, &str) + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                Box::into_raw(callback) as *mut _,
            );
            ffi::SSL_CTX_set_keylog_callback(self.as_ptr(), Some(callbacks::raw_keylog::<F>));
        }
    }

    /// Sets the session caching mode use for connections made with the context.
    ///
    /// Returns the previous session caching mode.
    ///
    /// This corresponds to [`SSL_CTX_set_session_cache_mode`].
    ///
    /// [`SSL_CTX_set_session_cache_mode`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_get_session_cache_mode.html
    pub fn set_session_cache_mode(&mut self, mode: SslSessionCacheMode) -> SslSessionCacheMode {
        unsafe {
            let bits = ffi::SSL_CTX_set_session_cache_mode(self.as_ptr(), mode.bits());
            SslSessionCacheMode { bits }
        }
    }

    /// Sets the callback for generating an application cookie for stateless handshakes.
    ///
    /// The callback will be called with the SSL context and a slice into which the cookie
    /// should be written. The callback should return the number of bytes written.
    ///
    /// This corresponds to `SSL_CTX_set_cookie_generate_cb`.
    pub fn set_cookie_generate_cb<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, &mut [u8]) -> Result<usize, ErrorStack> + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                mem::transmute(callback),
            );
            ffi::SSL_CTX_set_cookie_generate_cb(self.as_ptr(), Some(raw_cookie_generate::<F>))
        }
    }

    /// Sets the callback for verifying an application cookie for stateless handshakes.
    ///
    /// The callback will be called with the SSL context and the cookie supplied by the
    /// client. It should return true if and only if the cookie is valid.
    ///
    /// Note that the OpenSSL implementation independently verifies the integrity of
    /// application cookies using an HMAC before invoking the supplied callback.
    ///
    /// This corresponds to `SSL_CTX_set_cookie_verify_cb`.
    pub fn set_cookie_verify_cb<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, &[u8]) -> bool + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                get_callback_idx::<F>(),
                mem::transmute(callback),
            );
            ffi::SSL_CTX_set_cookie_verify_cb(self.as_ptr(), Some(raw_cookie_verify::<F>))
        }
    }

    /// Sets the extra data at the specified index.
    ///
    /// This can be used to provide data to callbacks registered with the context. Use the
    /// `SslContext::new_ex_index` method to create an `Index`.
    ///
    /// This corresponds to [`SSL_CTX_set_ex_data`].
    ///
    /// [`SSL_CTX_set_ex_data`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_set_ex_data.html
    pub fn set_ex_data<T>(&mut self, index: Index<SslContext, T>, data: T) {
        unsafe {
            let data = Box::new(data);
            ffi::SSL_CTX_set_ex_data(
                self.as_ptr(),
                index.as_raw(),
                Box::into_raw(data) as *mut c_void,
            );
        }
    }

    /// Consumes the builder, returning a new `SslContext`.
    pub fn build(self) -> SslContext {
        self.0
    }
}

foreign_type_and_impl_send_sync! {
    type CType = ffi::SSL_CTX;
    fn drop = ffi::SSL_CTX_free;

    /// A context object for TLS streams.
    ///
    /// Applications commonly configure a single `SslContext` that is shared by all of its
    /// `SslStreams`.
    pub struct SslContext;

    /// Reference to [`SslContext`]
    ///
    /// [`SslContext`]: struct.SslContext.html
    pub struct SslContextRef;
}

impl Clone for SslContext {
    fn clone(&self) -> Self {
        unsafe {
            compat::SSL_CTX_up_ref(self.as_ptr());
            SslContext::from_ptr(self.as_ptr())
        }
    }
}

// TODO: add useful info here
impl fmt::Debug for SslContext {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "SslContext")
    }
}

impl SslContext {
    /// Creates a new builder object for an `SslContext`.
    pub fn builder(method: SslMethod) -> Result<SslContextBuilder, ErrorStack> {
        SslContextBuilder::new(method)
    }

    /// Returns a new extra data index.
    ///
    /// Each invocation of this function is guaranteed to return a distinct index. These can be used
    /// to store data in the context that can be retrieved later by callbacks, for example.
    ///
    /// This corresponds to [`SSL_CTX_get_ex_new_index`].
    ///
    /// [`SSL_CTX_get_ex_new_index`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_CTX_get_ex_new_index.html
    pub fn new_ex_index<T>() -> Result<Index<SslContext, T>, ErrorStack>
    where
        T: 'static + Sync + Send,
    {
        unsafe {
            ffi::init();
            let idx = cvt_n(compat::get_new_idx(free_data_box::<T>))?;
            Ok(Index::from_raw(idx))
        }
    }
}

impl SslContextRef {
    /// Returns the certificate associated with this `SslContext`, if present.
    ///
    /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
    ///
    /// This corresponds to [`SSL_CTX_get0_certificate`].
    ///
    /// [`SSL_CTX_get0_certificate`]: https://www.openssl.org/docs/man1.1.0/ssl/ssl.html
    #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
              all(feature = "v111", ossl111)))]
    pub fn certificate(&self) -> Option<&X509Ref> {
        unsafe {
            let ptr = ffi::SSL_CTX_get0_certificate(self.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(X509Ref::from_ptr(ptr))
            }
        }
    }

    /// Returns the private key associated with this `SslContext`, if present.
    ///
    /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
    ///
    /// This corresponds to [`SSL_CTX_get0_privatekey`].
    ///
    /// [`SSL_CTX_get0_privatekey`]: https://www.openssl.org/docs/man1.1.0/ssl/ssl.html
    #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
              all(feature = "v111", ossl111)))]
    pub fn private_key(&self) -> Option<&PKeyRef<Private>> {
        unsafe {
            let ptr = ffi::SSL_CTX_get0_privatekey(self.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(PKeyRef::from_ptr(ptr))
            }
        }
    }

    /// Returns a shared reference to the certificate store used for verification.
    ///
    /// This corresponds to [`SSL_CTX_get_cert_store`].
    ///
    /// [`SSL_CTX_get_cert_store`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_CTX_get_cert_store.html
    pub fn cert_store(&self) -> &X509StoreRef {
        unsafe { X509StoreRef::from_ptr(ffi::SSL_CTX_get_cert_store(self.as_ptr())) }
    }

    /// Returns a shared reference to the stack of certificates making up the chain from the leaf.
    ///
    /// This corresponds to `SSL_CTX_get_extra_chain_certs`.
    pub fn extra_chain_certs(&self) -> &StackRef<X509> {
        unsafe {
            let mut chain = ptr::null_mut();
            ffi::SSL_CTX_get_extra_chain_certs(self.as_ptr(), &mut chain);
            assert!(!chain.is_null());
            StackRef::from_ptr(chain)
        }
    }

    /// Returns a reference to the extra data at the specified index.
    ///
    /// This corresponds to [`SSL_CTX_get_ex_data`].
    ///
    /// [`SSL_CTX_get_ex_data`]: https://www.openssl.org/docs/manmaster/man3/SSL_CTX_get_ex_data.html
    pub fn ex_data<T>(&self, index: Index<SslContext, T>) -> Option<&T> {
        unsafe {
            let data = ffi::SSL_CTX_get_ex_data(self.as_ptr(), index.as_raw());
            if data.is_null() {
                None
            } else {
                Some(&*(data as *const T))
            }
        }
    }
}

/// Information about the state of a cipher.
pub struct CipherBits {
    /// The number of secret bits used for the cipher.
    pub secret: i32,

    /// The number of bits processed by the chosen algorithm.
    pub algorithm: i32,
}

/// Information about a cipher.
pub struct SslCipher(*mut ffi::SSL_CIPHER);

impl ForeignType for SslCipher {
    type CType = ffi::SSL_CIPHER;
    type Ref = SslCipherRef;

    #[inline]
    unsafe fn from_ptr(ptr: *mut ffi::SSL_CIPHER) -> SslCipher {
        SslCipher(ptr)
    }

    #[inline]
    fn as_ptr(&self) -> *mut ffi::SSL_CIPHER {
        self.0
    }
}

impl Deref for SslCipher {
    type Target = SslCipherRef;

    fn deref(&self) -> &SslCipherRef {
        unsafe { SslCipherRef::from_ptr(self.0) }
    }
}

impl DerefMut for SslCipher {
    fn deref_mut(&mut self) -> &mut SslCipherRef {
        unsafe { SslCipherRef::from_ptr_mut(self.0) }
    }
}

/// Reference to an [`SslCipher`].
///
/// [`SslCipher`]: struct.SslCipher.html
pub struct SslCipherRef(Opaque);

impl ForeignTypeRef for SslCipherRef {
    type CType = ffi::SSL_CIPHER;
}

impl SslCipherRef {
    /// Returns the name of the cipher.
    ///
    /// This corresponds to [`SSL_CIPHER_get_name`].
    ///
    /// [`SSL_CIPHER_get_name`]: https://www.openssl.org/docs/manmaster/man3/SSL_CIPHER_get_name.html
    pub fn name(&self) -> &str {
        let name = unsafe {
            let ptr = ffi::SSL_CIPHER_get_name(self.as_ptr());
            CStr::from_ptr(ptr as *const _)
        };

        str::from_utf8(name.to_bytes()).unwrap()
    }

    /// Returns the SSL/TLS protocol version that first defined the cipher.
    ///
    /// This corresponds to [`SSL_CIPHER_get_version`].
    ///
    /// [`SSL_CIPHER_get_version`]: https://www.openssl.org/docs/manmaster/man3/SSL_CIPHER_get_name.html
    pub fn version(&self) -> &str {
        let version = unsafe {
            let ptr = ffi::SSL_CIPHER_get_version(self.as_ptr());
            CStr::from_ptr(ptr as *const _)
        };

        str::from_utf8(version.to_bytes()).unwrap()
    }

    /// Returns the number of bits used for the cipher.
    ///
    /// This corresponds to [`SSL_CIPHER_get_bits`].
    ///
    /// [`SSL_CIPHER_get_bits`]: https://www.openssl.org/docs/manmaster/man3/SSL_CIPHER_get_name.html
    pub fn bits(&self) -> CipherBits {
        unsafe {
            let mut algo_bits = 0;
            let secret_bits = ffi::SSL_CIPHER_get_bits(self.as_ptr(), &mut algo_bits);
            CipherBits {
                secret: secret_bits.into(),
                algorithm: algo_bits.into(),
            }
        }
    }

    /// Returns a textual description of the cipher.
    ///
    /// This corresponds to [`SSL_CIPHER_description`].
    ///
    /// [`SSL_CIPHER_description`]: https://www.openssl.org/docs/manmaster/man3/SSL_CIPHER_get_name.html
    pub fn description(&self) -> String {
        unsafe {
            // SSL_CIPHER_description requires a buffer of at least 128 bytes.
            let mut buf = [0; 128];
            let ptr = ffi::SSL_CIPHER_description(self.as_ptr(), buf.as_mut_ptr(), 128);
            String::from_utf8(CStr::from_ptr(ptr as *const _).to_bytes().to_vec()).unwrap()
        }
    }
}

foreign_type! {
    type CType = ffi::SSL_SESSION;
    fn drop = ffi::SSL_SESSION_free;

    /// An encoded SSL session.
    ///
    /// These can be cached to share sessions across connections.
    pub struct SslSession;

    /// Reference to [`SslSession`].
    ///
    /// [`SslSession`]: struct.SslSession.html
    pub struct SslSessionRef;
}

unsafe impl Sync for SslSession {}
unsafe impl Send for SslSession {}

impl Clone for SslSession {
    fn clone(&self) -> SslSession {
        SslSessionRef::to_owned(self)
    }
}

impl ToOwned for SslSessionRef {
    type Owned = SslSession;

    fn to_owned(&self) -> SslSession {
        unsafe {
            compat::SSL_SESSION_up_ref(self.as_ptr());
            SslSession(self.as_ptr())
        }
    }
}

impl SslSessionRef {
    /// Returns the SSL session ID.
    ///
    /// This corresponds to [`SSL_SESSION_get_id`].
    ///
    /// [`SSL_SESSION_get_id`]: https://www.openssl.org/docs/manmaster/man3/SSL_SESSION_get_id.html
    pub fn id(&self) -> &[u8] {
        unsafe {
            let mut len = 0;
            let p = ffi::SSL_SESSION_get_id(self.as_ptr(), &mut len);
            slice::from_raw_parts(p as *const u8, len as usize)
        }
    }

    /// Returns the length of the master key.
    ///
    /// This corresponds to [`SSL_SESSION_get_master_key`].
    ///
    /// [`SSL_SESSION_get_master_key`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_SESSION_get_master_key.html
    pub fn master_key_len(&self) -> usize {
        unsafe { compat::SSL_SESSION_get_master_key(self.as_ptr(), ptr::null_mut(), 0) }
    }

    /// Copies the master key into the provided buffer.
    ///
    /// Returns the number of bytes written, or the size of the master key if the buffer is empty.
    ///
    /// This corresponds to [`SSL_SESSION_get_master_key`].
    ///
    /// [`SSL_SESSION_get_master_key`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_SESSION_get_master_key.html
    pub fn master_key(&self, buf: &mut [u8]) -> usize {
        unsafe { compat::SSL_SESSION_get_master_key(self.as_ptr(), buf.as_mut_ptr(), buf.len()) }
    }
}

foreign_type! {
    type CType = ffi::SSL;
    fn drop = ffi::SSL_free;

    /// The state of an SSL/TLS session.
    ///
    /// `Ssl` objects are created from an [`SslContext`], which provides configuration defaults.
    /// These defaults can be overridden on a per-`Ssl` basis, however.
    ///
    /// [`SslContext`]: struct.SslContext.html
    pub struct Ssl;

    /// Reference to an [`Ssl`].
    ///
    /// [`Ssl`]: struct.Ssl.html
    pub struct SslRef;
}

impl Ssl {
    /// Returns a new extra data index.
    ///
    /// Each invocation of this function is guaranteed to return a distinct index. These can be used
    /// to store data in the context that can be retrieved later by callbacks, for example.
    ///
    /// This corresponds to [`SSL_get_ex_new_index`].
    ///
    /// [`SSL_get_ex_new_index`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_get_ex_new_index.html
    pub fn new_ex_index<T>() -> Result<Index<Ssl, T>, ErrorStack>
    where
        T: 'static + Sync + Send,
    {
        unsafe {
            ffi::init();
            let idx = cvt_n(compat::get_new_ssl_idx(free_data_box::<T>))?;
            Ok(Index::from_raw(idx))
        }
    }
}

impl fmt::Debug for SslRef {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Ssl")
            .field("state", &self.state_string_long())
            .field("verify_result", &self.verify_result())
            .finish()
    }
}

impl SslRef {
    fn get_raw_rbio(&self) -> *mut ffi::BIO {
        unsafe { ffi::SSL_get_rbio(self.as_ptr()) }
    }

    fn read(&mut self, buf: &mut [u8]) -> c_int {
        let len = cmp::min(c_int::max_value() as usize, buf.len()) as c_int;
        unsafe { ffi::SSL_read(self.as_ptr(), buf.as_ptr() as *mut c_void, len) }
    }

    fn write(&mut self, buf: &[u8]) -> c_int {
        let len = cmp::min(c_int::max_value() as usize, buf.len()) as c_int;
        unsafe { ffi::SSL_write(self.as_ptr(), buf.as_ptr() as *const c_void, len) }
    }

    fn get_error(&self, ret: c_int) -> ErrorCode {
        unsafe { ErrorCode::from_raw(ffi::SSL_get_error(self.as_ptr(), ret)) }
    }

    /// Like [`SslContextBuilder::set_verify`].
    ///
    /// This corresponds to [`SSL_set_verify`].
    ///
    /// [`SslContextBuilder::set_verify`]: struct.SslContextBuilder.html#method.set_verify
    /// [`SSL_set_verify`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_set_verify.html
    pub fn set_verify(&mut self, mode: SslVerifyMode) {
        unsafe { ffi::SSL_set_verify(self.as_ptr(), mode.bits as c_int, None) }
    }

    /// Like [`SslContextBuilder::set_verify_callback`].
    ///
    /// This corresponds to [`SSL_set_verify`].
    ///
    /// [`SslContextBuilder::set_verify_callback`]: struct.SslContextBuilder.html#method.set_verify_callback
    /// [`SSL_set_verify`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_set_verify.html
    pub fn set_verify_callback<F>(&mut self, mode: SslVerifyMode, verify: F)
    where
        F: Fn(bool, &mut X509StoreContextRef) -> bool + 'static + Sync + Send,
    {
        unsafe {
            let verify = Box::new(verify);
            ffi::SSL_set_ex_data(
                self.as_ptr(),
                get_ssl_callback_idx::<F>(),
                mem::transmute(verify),
            );
            ffi::SSL_set_verify(self.as_ptr(), mode.bits as c_int, Some(ssl_raw_verify::<F>));
        }
    }

    /// Like [`SslContextBuilder::set_tmp_dh`].
    ///
    /// This corresponds to [`SSL_set_tmp_dh`].
    ///
    /// [`SslContextBuilder::set_tmp_dh`]: struct.SslContextBuilder.html#method.set_tmp_dh
    /// [`SSL_set_tmp_dh`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_set_tmp_dh.html
    pub fn set_tmp_dh(&mut self, dh: &DhRef<Params>) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::SSL_set_tmp_dh(self.as_ptr(), dh.as_ptr()) as c_int).map(|_| ()) }
    }

    /// Like [`SslContextBuilder::set_tmp_dh_callback`].
    ///
    /// This corresponds to [`SSL_set_tmp_dh_callback`].
    ///
    /// [`SslContextBuilder::set_tmp_dh_callback`]: struct.SslContextBuilder.html#method.set_tmp_dh_callback
    /// [`SSL_set_tmp_dh_callback`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_set_tmp_dh.html
    pub fn set_tmp_dh_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, bool, u32) -> Result<Dh<Params>, ErrorStack> + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_set_ex_data(
                self.as_ptr(),
                get_ssl_callback_idx::<F>(),
                Box::into_raw(callback) as *mut c_void,
            );
            let f: unsafe extern "C" fn(_, _, _) -> _ = raw_tmp_dh_ssl::<F>;
            ffi::SSL_set_tmp_dh_callback(self.as_ptr(), f);
        }
    }

    /// Like [`SslContextBuilder::set_tmp_ecdh`].
    ///
    /// This corresponds to `SSL_set_tmp_ecdh`.
    ///
    /// [`SslContextBuilder::set_tmp_ecdh`]: struct.SslContextBuilder.html#method.set_tmp_ecdh
    pub fn set_tmp_ecdh(&mut self, key: &EcKeyRef<Params>) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::SSL_set_tmp_ecdh(self.as_ptr(), key.as_ptr()) as c_int).map(|_| ()) }
    }

    /// Like [`SslContextBuilder::set_tmp_ecdh_callback`].
    ///
    /// Requires the `v101` feature and OpenSSL 1.0.1, or the `v102` feature and OpenSSL 1.0.2.
    ///
    /// This corresponds to `SSL_set_tmp_ecdh_callback`.
    ///
    /// [`SslContextBuilder::set_tmp_ecdh_callback`]: struct.SslContextBuilder.html#method.set_tmp_ecdh_callback
    #[cfg(any(all(feature = "v101", ossl101), all(feature = "v102", ossl102)))]
    pub fn set_tmp_ecdh_callback<F>(&mut self, callback: F)
    where
        F: Fn(&mut SslRef, bool, u32) -> Result<EcKey<Params>, ErrorStack> + 'static + Sync + Send,
    {
        unsafe {
            let callback = Box::new(callback);
            ffi::SSL_set_ex_data(
                self.as_ptr(),
                get_ssl_callback_idx::<F>(),
                Box::into_raw(callback) as *mut c_void,
            );
            let f: unsafe extern "C" fn(_, _, _) -> _ = raw_tmp_ecdh_ssl::<F>;
            ffi::SSL_set_tmp_ecdh_callback(self.as_ptr(), f);
        }
    }

    /// Like [`SslContextBuilder::set_ecdh_auto`].
    ///
    /// Requires the `v102` feature and OpenSSL 1.0.2.
    ///
    /// This corresponds to [`SSL_set_ecdh_auto`].
    ///
    /// [`SslContextBuilder::set_tmp_ecdh`]: struct.SslContextBuilder.html#method.set_tmp_ecdh
    /// [`SSL_set_ecdh_auto`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_set_ecdh_auto.html
    #[cfg(all(feature = "v102", ossl102))]
    pub fn set_ecdh_auto(&mut self, onoff: bool) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::SSL_set_ecdh_auto(self.as_ptr(), onoff as c_int)).map(|_| ()) }
    }

    /// Returns the current cipher if the session is active.
    ///
    /// This corresponds to [`SSL_get_current_cipher`].
    ///
    /// [`SSL_get_current_cipher`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_get_current_cipher.html
    pub fn current_cipher(&self) -> Option<&SslCipherRef> {
        unsafe {
            let ptr = ffi::SSL_get_current_cipher(self.as_ptr());

            if ptr.is_null() {
                None
            } else {
                Some(SslCipherRef::from_ptr(ptr as *mut _))
            }
        }
    }

    /// Returns a short string describing the state of the session.
    ///
    /// This corresponds to [`SSL_state_string`].
    ///
    /// [`SSL_state_string`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_state_string.html
    pub fn state_string(&self) -> &'static str {
        let state = unsafe {
            let ptr = ffi::SSL_state_string(self.as_ptr());
            CStr::from_ptr(ptr as *const _)
        };

        str::from_utf8(state.to_bytes()).unwrap()
    }

    /// Returns a longer string describing the state of the session.
    ///
    /// This corresponds to [`SSL_state_string_long`].
    ///
    /// [`SSL_state_string_long`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_state_string_long.html
    pub fn state_string_long(&self) -> &'static str {
        let state = unsafe {
            let ptr = ffi::SSL_state_string_long(self.as_ptr());
            CStr::from_ptr(ptr as *const _)
        };

        str::from_utf8(state.to_bytes()).unwrap()
    }

    /// Sets the host name to be sent to the server for Server Name Indication (SNI).
    ///
    /// It has no effect for a server-side connection.
    ///
    /// This corresponds to [`SSL_set_tlsext_host_name`].
    ///
    /// [`SSL_set_tlsext_host_name`]: https://www.openssl.org/docs/manmaster/man3/SSL_get_servername_type.html
    pub fn set_hostname(&mut self, hostname: &str) -> Result<(), ErrorStack> {
        let cstr = CString::new(hostname).unwrap();
        unsafe {
            cvt(ffi::SSL_set_tlsext_host_name(self.as_ptr(), cstr.as_ptr() as *mut _) as c_int)
                .map(|_| ())
        }
    }

    /// Returns the peer's certificate, if present.
    ///
    /// This corresponds to [`SSL_get_peer_certificate`].
    ///
    /// [`SSL_get_peer_certificate`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_get_peer_certificate.html
    pub fn peer_certificate(&self) -> Option<X509> {
        unsafe {
            let ptr = ffi::SSL_get_peer_certificate(self.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(X509::from_ptr(ptr))
            }
        }
    }

    /// Returns the certificate chain of the peer, if present.
    ///
    /// On the client side, the chain includes the leaf certificate, but on the server side it does
    /// not. Fun!
    ///
    /// This corresponds to [`SSL_get_peer_cert_chain`].
    ///
    /// [`SSL_get_peer_cert_chain`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_get_peer_cert_chain.html
    pub fn peer_cert_chain(&self) -> Option<&StackRef<X509>> {
        unsafe {
            let ptr = ffi::SSL_get_peer_cert_chain(self.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(StackRef::from_ptr(ptr))
            }
        }
    }

    /// Like [`SslContext::certificate`].
    ///
    /// This corresponds to `SSL_get_certificate`.
    ///
    /// [`SslContext::certificate`]: struct.SslContext.html#method.certificate
    pub fn certificate(&self) -> Option<&X509Ref> {
        unsafe {
            let ptr = ffi::SSL_get_certificate(self.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(X509Ref::from_ptr(ptr))
            }
        }
    }

    /// Like [`SslContext::private_key`].
    ///
    /// This corresponds to `SSL_get_privatekey`.
    ///
    /// [`SslContext::private_key`]: struct.SslContext.html#method.private_key
    pub fn private_key(&self) -> Option<&PKeyRef<Private>> {
        unsafe {
            let ptr = ffi::SSL_get_privatekey(self.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(PKeyRef::from_ptr(ptr))
            }
        }
    }

    #[deprecated(since = "0.10.5", note = "renamed to `version_str`")]
    pub fn version(&self) -> &str {
        self.version_str()
    }

    /// Returns the protocol version of the session.
    ///
    /// This corresponds to [`SSL_version`].
    ///
    /// [`SSL_version`]: https://www.openssl.org/docs/manmaster/man3/SSL_version.html
    pub fn version2(&self) -> Option<SslVersion> {
        unsafe {
            let r = ffi::SSL_version(self.as_ptr());
            if r == 0 {
                None
            } else {
                Some(SslVersion(r))
            }
        }
    }

    /// Returns a string describing the protocol version of the session.
    ///
    /// This corresponds to [`SSL_get_version`].
    ///
    /// [`SSL_get_version`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_get_version.html
    pub fn version_str(&self) -> &'static str {
        let version = unsafe {
            let ptr = ffi::SSL_get_version(self.as_ptr());
            CStr::from_ptr(ptr as *const _)
        };

        str::from_utf8(version.to_bytes()).unwrap()
    }

    /// Returns the protocol selected via Application Layer Protocol Negotiation (ALPN).
    ///
    /// The protocol's name is returned is an opaque sequence of bytes. It is up to the client
    /// to interpret it.
    ///
    /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
    ///
    /// This corresponds to [`SSL_get0_alpn_selected`].
    ///
    /// [`SSL_get0_alpn_selected`]: https://www.openssl.org/docs/manmaster/man3/SSL_get0_next_proto_negotiated.html
    #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
              all(feature = "v111", ossl111)))]
    pub fn selected_alpn_protocol(&self) -> Option<&[u8]> {
        unsafe {
            let mut data: *const c_uchar = ptr::null();
            let mut len: c_uint = 0;
            // Get the negotiated protocol from the SSL instance.
            // `data` will point at a `c_uchar` array; `len` will contain the length of this array.
            ffi::SSL_get0_alpn_selected(self.as_ptr(), &mut data, &mut len);

            if data.is_null() {
                None
            } else {
                Some(slice::from_raw_parts(data, len as usize))
            }
        }
    }

    /// Returns the number of bytes remaining in the currently processed TLS record.
    ///
    /// If this is greater than 0, the next call to `read` will not call down to the underlying
    /// stream.
    ///
    /// This corresponds to [`SSL_pending`].
    ///
    /// [`SSL_pending`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_pending.html
    pub fn pending(&self) -> usize {
        unsafe { ffi::SSL_pending(self.as_ptr()) as usize }
    }

    /// Returns the servername sent by the client via Server Name Indication (SNI).
    ///
    /// It is only useful on the server side.
    ///
    /// This corresponds to [`SSL_get_servername`].
    ///
    /// [`SSL_get_servername`]: https://www.openssl.org/docs/manmaster/man3/SSL_get_servername.html
    pub fn servername(&self, type_: NameType) -> Option<&str> {
        unsafe {
            let name = ffi::SSL_get_servername(self.as_ptr(), type_.0);
            if name == ptr::null() {
                None
            } else {
                Some(str::from_utf8(CStr::from_ptr(name as *const _).to_bytes()).unwrap())
            }
        }
    }

    /// Changes the context corresponding to the current connection.
    ///
    /// It is most commonly used in the Server Name Indication (SNI) callback.
    ///
    /// This corresponds to `SSL_set_SSL_CTX`.
    pub fn set_ssl_context(&mut self, ctx: &SslContextRef) -> Result<(), ErrorStack> {
        unsafe { cvt_p(ffi::SSL_set_SSL_CTX(self.as_ptr(), ctx.as_ptr())).map(|_| ()) }
    }

    /// Returns the context corresponding to the current connection.
    ///
    /// This corresponds to [`SSL_get_SSL_CTX`].
    ///
    /// [`SSL_get_SSL_CTX`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_get_SSL_CTX.html
    pub fn ssl_context(&self) -> &SslContextRef {
        unsafe {
            let ssl_ctx = ffi::SSL_get_SSL_CTX(self.as_ptr());
            SslContextRef::from_ptr(ssl_ctx)
        }
    }

    /// Returns a mutable reference to the X509 verification configuration.
    ///
    /// Requires OpenSSL 1.0.2, 1.1.0, or 1.1.1 and the corresponding Cargo feature.
    ///
    /// This corresponds to [`SSL_get0_param`].
    ///
    /// [`SSL_get0_param`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_get0_param.html
    #[cfg(any(all(feature = "v102", ossl102), all(feature = "v110", ossl110),
              all(feature = "v111", ossl111)))]
    pub fn param_mut(&mut self) -> &mut X509VerifyParamRef {
        self._param_mut()
    }

    #[cfg(any(ossl102, ossl110))]
    fn _param_mut(&mut self) -> &mut X509VerifyParamRef {
        unsafe { X509VerifyParamRef::from_ptr_mut(ffi::SSL_get0_param(self.as_ptr())) }
    }

    /// Returns the certificate verification result.
    ///
    /// This corresponds to [`SSL_get_verify_result`].
    ///
    /// [`SSL_get_verify_result`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_get_verify_result.html
    pub fn verify_result(&self) -> X509VerifyResult {
        unsafe { X509VerifyResult::from_raw(ffi::SSL_get_verify_result(self.as_ptr()) as c_int) }
    }

    /// Returns a shared reference to the SSL session.
    ///
    /// This corresponds to [`SSL_get_session`].
    ///
    /// [`SSL_get_session`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_get_session.html
    pub fn session(&self) -> Option<&SslSessionRef> {
        unsafe {
            let p = ffi::SSL_get_session(self.as_ptr());
            if p.is_null() {
                None
            } else {
                Some(SslSessionRef::from_ptr(p))
            }
        }
    }

    /// Copies the client_random value sent by the client in the TLS handshake into a buffer.
    ///
    /// Returns the number of bytes copied, or if the buffer is empty, the size of the client_random
    /// value.
    ///
    /// Requires OpenSSL 1.1.0 or 1.1.1 and the corresponding Cargo feature.
    ///
    /// This corresponds to [`SSL_get_client_random`].
    ///
    /// [`SSL_get_client_random`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_get_client_random.html
    #[cfg(any(all(feature = "v110", ossl110), all(feature = "v111", ossl111)))]
    pub fn client_random(&self, buf: &mut [u8]) -> usize {
        unsafe {
            ffi::SSL_get_client_random(self.as_ptr(), buf.as_mut_ptr() as *mut c_uchar, buf.len())
        }
    }

    /// Copies the server_random value sent by the server in the TLS handshake into a buffer.
    ///
    /// Returns the number of bytes copied, or if the buffer is empty, the size of the server_random
    /// value.
    ///
    /// Requires OpenSSL 1.1.0 or 1.1.1 and the corresponding Cargo feature.
    ///
    /// This corresponds to [`SSL_get_server_random`].
    ///
    /// [`SSL_get_server_random`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_get_client_random.html
    #[cfg(any(all(feature = "v110", ossl110), all(feature = "v111", ossl111)))]
    pub fn server_random(&self, buf: &mut [u8]) -> usize {
        unsafe {
            ffi::SSL_get_server_random(self.as_ptr(), buf.as_mut_ptr() as *mut c_uchar, buf.len())
        }
    }

    /// Derives keying material for application use in accordance to RFC 5705.
    ///
    /// This corresponds to [`SSL_export_keying_material`].
    ///
    /// [`SSL_export_keying_material`]: https://www.openssl.org/docs/manmaster/man3/SSL_export_keying_material.html
    pub fn export_keying_material(
        &self,
        out: &mut [u8],
        label: &str,
        context: Option<&[u8]>,
    ) -> Result<(), ErrorStack> {
        unsafe {
            let (context, contextlen, use_context) = match context {
                Some(context) => (context.as_ptr() as *const c_uchar, context.len(), 1),
                None => (ptr::null(), 0, 0),
            };
            cvt(ffi::SSL_export_keying_material(
                self.as_ptr(),
                out.as_mut_ptr() as *mut c_uchar,
                out.len(),
                label.as_ptr() as *const c_char,
                label.len(),
                context,
                contextlen,
                use_context,
            )).map(|_| ())
        }
    }

    /// Sets the session to be used.
    ///
    /// This should be called before the handshake to attempt to reuse a previously established
    /// session. If the server is not willing to reuse the session, a new one will be transparently
    /// negotiated.
    ///
    /// This corresponds to [`SSL_set_session`].
    ///
    /// # Safety
    ///
    /// The caller of this method is responsible for ensuring that the session is associated
    /// with the same `SslContext` as this `Ssl`.
    ///
    /// [`SSL_set_session`]: https://www.openssl.org/docs/manmaster/man3/SSL_set_session.html
    pub unsafe fn set_session(&mut self, session: &SslSessionRef) -> Result<(), ErrorStack> {
        cvt(ffi::SSL_set_session(self.as_ptr(), session.as_ptr())).map(|_| ())
    }

    /// Determines if the session provided to `set_session` was successfully reused.
    ///
    /// This corresponds to [`SSL_session_reused`].
    ///
    /// [`SSL_session_reused`]: https://www.openssl.org/docs/man1.1.0/ssl/SSL_session_reused.html
    pub fn session_reused(&self) -> bool {
        unsafe { ffi::SSL_session_reused(self.as_ptr()) != 0 }
    }

    /// Sets the status response a client wishes the server to reply with.
    ///
    /// This corresponds to [`SSL_set_tlsext_status_type`].
    ///
    /// [`SSL_set_tlsext_status_type`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_set_tlsext_status_type.html
    pub fn set_status_type(&mut self, type_: StatusType) -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::SSL_set_tlsext_status_type(self.as_ptr(), type_.as_raw()) as c_int).map(|_| ())
        }
    }

    /// Returns the server's OCSP response, if present.
    ///
    /// This corresponds to [`SSL_get_tlsext_status_ocsp_resp`].
    ///
    /// [`SSL_get_tlsext_status_ocsp_resp`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_set_tlsext_status_type.html
    pub fn ocsp_status(&self) -> Option<&[u8]> {
        unsafe {
            let mut p = ptr::null_mut();
            let len = ffi::SSL_get_tlsext_status_ocsp_resp(self.as_ptr(), &mut p);

            if len < 0 {
                None
            } else {
                Some(slice::from_raw_parts(p as *const u8, len as usize))
            }
        }
    }

    /// Sets the OCSP response to be returned to the client.
    ///
    /// This corresponds to [`SSL_set_tlsext_status_ocsp_resp`].
    ///
    /// [`SSL_set_tlsext_status_ocsp_resp`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_set_tlsext_status_type.html
    pub fn set_ocsp_status(&mut self, response: &[u8]) -> Result<(), ErrorStack> {
        unsafe {
            assert!(response.len() <= c_int::max_value() as usize);
            let p = cvt_p(ffi::CRYPTO_malloc(
                response.len() as _,
                concat!(file!(), "\0").as_ptr() as *const _,
                line!() as c_int,
            ))?;
            ptr::copy_nonoverlapping(response.as_ptr(), p as *mut u8, response.len());
            cvt(ffi::SSL_set_tlsext_status_ocsp_resp(
                self.as_ptr(),
                p as *mut c_uchar,
                response.len() as c_long,
            ) as c_int)
                .map(|_| ())
        }
    }

    /// Determines if this `Ssl` is configured for server-side or client-side use.
    ///
    /// This corresponds to [`SSL_is_server`].
    ///
    /// [`SSL_is_server`]: https://www.openssl.org/docs/manmaster/man3/SSL_is_server.html
    pub fn is_server(&self) -> bool {
        unsafe { compat::SSL_is_server(self.as_ptr()) != 0 }
    }

    /// Sets the extra data at the specified index.
    ///
    /// This can be used to provide data to callbacks registered with the context. Use the
    /// `Ssl::new_ex_index` method to create an `Index`.
    ///
    /// This corresponds to [`SSL_set_ex_data`].
    ///
    /// [`SSL_set_ex_data`]: https://www.openssl.org/docs/manmaster/man3/SSL_set_ex_data.html
    pub fn set_ex_data<T>(&mut self, index: Index<Ssl, T>, data: T) {
        unsafe {
            let data = Box::new(data);
            ffi::SSL_set_ex_data(
                self.as_ptr(),
                index.as_raw(),
                Box::into_raw(data) as *mut c_void,
            );
        }
    }

    /// Returns a reference to the extra data at the specified index.
    ///
    /// This corresponds to [`SSL_get_ex_data`].
    ///
    /// [`SSL_get_ex_data`]: https://www.openssl.org/docs/manmaster/man3/SSL_set_ex_data.html
    pub fn ex_data<T>(&self, index: Index<Ssl, T>) -> Option<&T> {
        unsafe {
            let data = ffi::SSL_get_ex_data(self.as_ptr(), index.as_raw());
            if data.is_null() {
                None
            } else {
                Some(&*(data as *const T))
            }
        }
    }
}

unsafe impl Sync for Ssl {}
unsafe impl Send for Ssl {}

impl fmt::Debug for Ssl {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, fmt)
    }
}

impl Ssl {
    /// Creates a new `Ssl`.
    ///
    /// This corresponds to [`SSL_new`].
    ///
    /// [`SSL_new`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_new.html
    pub fn new(ctx: &SslContext) -> Result<Ssl, ErrorStack> {
        unsafe {
            let ssl = cvt_p(ffi::SSL_new(ctx.as_ptr()))?;
            Ok(Ssl::from_ptr(ssl))
        }
    }

    /// Initiates a client-side TLS handshake.
    ///
    /// This corresponds to [`SSL_connect`].
    ///
    /// # Warning
    ///
    /// OpenSSL's default configuration is insecure. It is highly recommended to use
    /// `SslConnector` rather than `Ssl` directly, as it manages that configuration.
    ///
    /// [`SSL_connect`]: https://www.openssl.org/docs/manmaster/man3/SSL_connect.html
    pub fn connect<S>(self, stream: S) -> Result<SslStream<S>, HandshakeError<S>>
    where
        S: Read + Write,
    {
        let mut stream = SslStream::new_base(self, stream);
        let ret = unsafe { ffi::SSL_connect(stream.ssl.as_ptr()) };
        if ret > 0 {
            Ok(stream)
        } else {
            let error = stream.make_error(ret);
            match error.code() {
                ErrorCode::WANT_READ | ErrorCode::WANT_WRITE => Err(HandshakeError::WouldBlock(
                    MidHandshakeSslStream { stream, error },
                )),
                _ => Err(HandshakeError::Failure(MidHandshakeSslStream {
                    stream,
                    error,
                })),
            }
        }
    }

    /// Initiates a server-side TLS handshake.
    ///
    /// This corresponds to [`SSL_accept`].
    ///
    /// # Warning
    ///
    /// OpenSSL's default configuration is insecure. It is highly recommended to use
    /// `SslAcceptor` rather than `Ssl` directly, as it manages that configuration.
    ///
    /// [`SSL_accept`]: https://www.openssl.org/docs/manmaster/man3/SSL_accept.html
    pub fn accept<S>(self, stream: S) -> Result<SslStream<S>, HandshakeError<S>>
    where
        S: Read + Write,
    {
        let mut stream = SslStream::new_base(self, stream);
        let ret = unsafe { ffi::SSL_accept(stream.ssl.as_ptr()) };
        if ret > 0 {
            Ok(stream)
        } else {
            let error = stream.make_error(ret);
            match error.code() {
                ErrorCode::WANT_READ | ErrorCode::WANT_WRITE => Err(HandshakeError::WouldBlock(
                    MidHandshakeSslStream { stream, error },
                )),
                _ => Err(HandshakeError::Failure(MidHandshakeSslStream {
                    stream,
                    error,
                })),
            }
        }
    }
}

/// An SSL stream midway through the handshake process.
#[derive(Debug)]
pub struct MidHandshakeSslStream<S> {
    stream: SslStream<S>,
    error: Error,
}

impl<S> MidHandshakeSslStream<S> {
    /// Returns a shared reference to the inner stream.
    pub fn get_ref(&self) -> &S {
        self.stream.get_ref()
    }

    /// Returns a mutable reference to the inner stream.
    pub fn get_mut(&mut self) -> &mut S {
        self.stream.get_mut()
    }

    /// Returns a shared reference to the `Ssl` of the stream.
    pub fn ssl(&self) -> &SslRef {
        self.stream.ssl()
    }

    /// Returns the underlying error which interrupted this handshake.
    pub fn error(&self) -> &Error {
        &self.error
    }

    /// Consumes `self`, returning its error.
    pub fn into_error(self) -> Error {
        self.error
    }

    /// Restarts the handshake process.
    ///
    /// This corresponds to [`SSL_do_handshake`].
    ///
    /// [`SSL_do_handshake`]: https://www.openssl.org/docs/manmaster/man3/SSL_do_handshake.html
    pub fn handshake(mut self) -> Result<SslStream<S>, HandshakeError<S>> {
        let ret = unsafe { ffi::SSL_do_handshake(self.stream.ssl.as_ptr()) };
        if ret > 0 {
            Ok(self.stream)
        } else {
            self.error = self.stream.make_error(ret);
            match self.error.code() {
                ErrorCode::WANT_READ | ErrorCode::WANT_WRITE => {
                    Err(HandshakeError::WouldBlock(self))
                }
                _ => Err(HandshakeError::Failure(self)),
            }
        }
    }
}

/// A TLS session over a stream.
pub struct SslStream<S> {
    ssl: ManuallyDrop<Ssl>,
    method: ManuallyDrop<BioMethod>,
    _p: PhantomData<S>,
}

impl<S> Drop for SslStream<S> {
    fn drop(&mut self) {
        // ssl holds a reference to method internally so it has to drop first
        unsafe {
            ManuallyDrop::drop(&mut self.ssl);
            ManuallyDrop::drop(&mut self.method);
        }
    }
}

impl<S> fmt::Debug for SslStream<S>
where
    S: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("SslStream")
            .field("stream", &self.get_ref())
            .field("ssl", &self.ssl())
            .finish()
    }
}

impl<S: Read + Write> SslStream<S> {
    fn new_base(ssl: Ssl, stream: S) -> Self {
        unsafe {
            let (bio, method) = bio::new(stream).unwrap();
            ffi::SSL_set_bio(ssl.as_ptr(), bio, bio);

            SslStream {
                ssl: ManuallyDrop::new(ssl),
                method: ManuallyDrop::new(method),
                _p: PhantomData,
            }
        }
    }

    /// Like `read`, but returns an `ssl::Error` rather than an `io::Error`.
    ///
    /// It is particularly useful with a nonblocking socket, where the error value will identify if
    /// OpenSSL is waiting on read or write readiness.
    ///
    /// This corresponds to [`SSL_read`].
    ///
    /// [`SSL_read`]: https://www.openssl.org/docs/manmaster/man3/SSL_read.html
    pub fn ssl_read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        // The intepretation of the return code here is a little odd with a
        // zero-length write. OpenSSL will likely correctly report back to us
        // that it read zero bytes, but zero is also the sentinel for "error".
        // To avoid that confusion short-circuit that logic and return quickly
        // if `buf` has a length of zero.
        if buf.len() == 0 {
            return Ok(0);
        }

        let ret = self.ssl.read(buf);
        if ret > 0 {
            Ok(ret as usize)
        } else {
            Err(self.make_error(ret))
        }
    }

    /// Like `write`, but returns an `ssl::Error` rather than an `io::Error`.
    ///
    /// It is particularly useful with a nonblocking socket, where the error value will identify if
    /// OpenSSL is waiting on read or write readiness.
    ///
    /// This corresponds to [`SSL_write`].
    ///
    /// [`SSL_write`]: https://www.openssl.org/docs/manmaster/man3/SSL_write.html
    pub fn ssl_write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        // See above for why we short-circuit on zero-length buffers
        if buf.len() == 0 {
            return Ok(0);
        }

        let ret = self.ssl.write(buf);
        if ret > 0 {
            Ok(ret as usize)
        } else {
            Err(self.make_error(ret))
        }
    }

    /// Shuts down the session.
    ///
    /// The shutdown process consists of two steps. The first step sends a close notify message to
    /// the peer, after which `ShutdownResult::Sent` is returned. The second step awaits the receipt
    /// of a close notify message from the peer, after which `ShutdownResult::Received` is returned.
    ///
    /// While the connection may be closed after the first step, it is recommended to fully shut the
    /// session down. In particular, it must be fully shut down if the connection is to be used for
    /// further communication in the future.
    ///
    /// This corresponds to [`SSL_shutdown`].
    ///
    /// [`SSL_shutdown`]: https://www.openssl.org/docs/man1.0.2/ssl/SSL_shutdown.html
    pub fn shutdown(&mut self) -> Result<ShutdownResult, Error> {
        match unsafe { ffi::SSL_shutdown(self.ssl.as_ptr()) } {
            0 => Ok(ShutdownResult::Sent),
            1 => Ok(ShutdownResult::Received),
            n => Err(self.make_error(n)),
        }
    }
}

impl<S> SslStream<S> {
    fn make_error(&mut self, ret: c_int) -> Error {
        self.check_panic();

        let code = self.ssl.get_error(ret);

        let cause = match code {
            ErrorCode::SSL => Some(InnerError::Ssl(ErrorStack::get())),
            ErrorCode::SYSCALL => {
                let errs = ErrorStack::get();
                if errs.errors().is_empty() {
                    self.get_bio_error().map(InnerError::Io)
                } else {
                    Some(InnerError::Ssl(errs))
                }
            }
            ErrorCode::ZERO_RETURN => None,
            ErrorCode::WANT_READ | ErrorCode::WANT_WRITE => {
                self.get_bio_error().map(InnerError::Io)
            }
            _ => None,
        };

        Error { code, cause }
    }

    fn check_panic(&mut self) {
        if let Some(err) = unsafe { bio::take_panic::<S>(self.ssl.get_raw_rbio()) } {
            resume_unwind(err)
        }
    }

    fn get_bio_error(&mut self) -> Option<io::Error> {
        unsafe { bio::take_error::<S>(self.ssl.get_raw_rbio()) }
    }

    /// Returns a shared reference to the underlying stream.
    pub fn get_ref(&self) -> &S {
        unsafe {
            let bio = self.ssl.get_raw_rbio();
            bio::get_ref(bio)
        }
    }

    /// Returns a mutable reference to the underlying stream.
    ///
    /// # Warning
    ///
    /// It is inadvisable to read from or write to the underlying stream as it
    /// will most likely corrupt the SSL session.
    pub fn get_mut(&mut self) -> &mut S {
        unsafe {
            let bio = self.ssl.get_raw_rbio();
            bio::get_mut(bio)
        }
    }

    /// Returns a shared reference to the `Ssl` object associated with this stream.
    pub fn ssl(&self) -> &SslRef {
        &self.ssl
    }
}

impl<S: Read + Write> Read for SslStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.ssl_read(buf) {
                Ok(n) => return Ok(n),
                Err(ref e) if e.code() == ErrorCode::ZERO_RETURN => return Ok(0),
                Err(ref e) if e.code() == ErrorCode::SYSCALL && e.io_error().is_none() => {
                    return Ok(0)
                }
                Err(ref e) if e.code() == ErrorCode::WANT_READ && e.io_error().is_none() => {}
                Err(e) => {
                    return Err(e.into_io_error()
                        .unwrap_or_else(|e| io::Error::new(io::ErrorKind::Other, e)))
                }
            }
        }
    }
}

impl<S: Read + Write> Write for SslStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        loop {
            match self.ssl_write(buf) {
                Ok(n) => return Ok(n),
                Err(ref e) if e.code() == ErrorCode::WANT_READ && e.io_error().is_none() => {}
                Err(e) => {
                    return Err(e.into_io_error()
                        .unwrap_or_else(|e| io::Error::new(io::ErrorKind::Other, e)))
                }
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.get_mut().flush()
    }
}

/// The result of a shutdown request.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ShutdownResult {
    /// A close notify message has been sent to the peer.
    Sent,

    /// A close notify response message has been received from the peer.
    Received,
}

#[cfg(ossl110)]
mod compat {
    use std::ptr;

    use ffi;
    use libc::c_int;

    pub use ffi::{SSL_CTX_clear_options, SSL_CTX_get_options, SSL_CTX_set_options, SSL_CTX_up_ref,
                  SSL_SESSION_get_master_key, SSL_SESSION_up_ref, SSL_is_server};

    pub unsafe fn get_new_idx(f: ffi::CRYPTO_EX_free) -> c_int {
        ffi::CRYPTO_get_ex_new_index(
            ffi::CRYPTO_EX_INDEX_SSL_CTX,
            0,
            ptr::null_mut(),
            None,
            None,
            Some(f),
        )
    }

    pub unsafe fn get_new_ssl_idx(f: ffi::CRYPTO_EX_free) -> c_int {
        ffi::CRYPTO_get_ex_new_index(
            ffi::CRYPTO_EX_INDEX_SSL,
            0,
            ptr::null_mut(),
            None,
            None,
            Some(f),
        )
    }

    pub fn tls_method() -> *const ffi::SSL_METHOD {
        unsafe { ffi::TLS_method() }
    }

    pub fn dtls_method() -> *const ffi::SSL_METHOD {
        unsafe { ffi::DTLS_method() }
    }
}

#[cfg(ossl10x)]
#[allow(bad_style)]
mod compat {
    use std::ptr;

    use ffi;
    use libc::{self, c_int, c_long, c_uchar, c_ulong, size_t};

    pub unsafe fn SSL_CTX_get_options(ctx: *const ffi::SSL_CTX) -> c_ulong {
        ffi::SSL_CTX_ctrl(ctx as *mut _, ffi::SSL_CTRL_OPTIONS, 0, ptr::null_mut()) as c_ulong
    }

    pub unsafe fn SSL_CTX_set_options(ctx: *const ffi::SSL_CTX, op: c_ulong) -> c_ulong {
        ffi::SSL_CTX_ctrl(
            ctx as *mut _,
            ffi::SSL_CTRL_OPTIONS,
            op as c_long,
            ptr::null_mut(),
        ) as c_ulong
    }

    pub unsafe fn SSL_CTX_clear_options(ctx: *const ffi::SSL_CTX, op: c_ulong) -> c_ulong {
        ffi::SSL_CTX_ctrl(
            ctx as *mut _,
            ffi::SSL_CTRL_CLEAR_OPTIONS,
            op as c_long,
            ptr::null_mut(),
        ) as c_ulong
    }

    pub unsafe fn get_new_idx(f: ffi::CRYPTO_EX_free) -> c_int {
        ffi::SSL_CTX_get_ex_new_index(0, ptr::null_mut(), None, None, Some(f))
    }

    pub unsafe fn get_new_ssl_idx(f: ffi::CRYPTO_EX_free) -> c_int {
        ffi::SSL_get_ex_new_index(0, ptr::null_mut(), None, None, Some(f))
    }

    pub unsafe fn SSL_CTX_up_ref(ssl: *mut ffi::SSL_CTX) -> libc::c_int {
        ffi::CRYPTO_add_lock(
            &mut (*ssl).references,
            1,
            ffi::CRYPTO_LOCK_SSL_CTX,
            "mod.rs\0".as_ptr() as *const _,
            line!() as libc::c_int,
        );
        0
    }

    pub unsafe fn SSL_SESSION_get_master_key(
        session: *const ffi::SSL_SESSION,
        out: *mut c_uchar,
        mut outlen: size_t,
    ) -> size_t {
        if outlen == 0 {
            return (*session).master_key_length as size_t;
        }
        if outlen > (*session).master_key_length as size_t {
            outlen = (*session).master_key_length as size_t;
        }
        ptr::copy_nonoverlapping((*session).master_key.as_ptr(), out, outlen);
        outlen
    }

    pub fn tls_method() -> *const ffi::SSL_METHOD {
        unsafe { ffi::SSLv23_method() }
    }

    pub fn dtls_method() -> *const ffi::SSL_METHOD {
        unsafe { ffi::DTLSv1_method() }
    }

    pub unsafe fn SSL_is_server(s: *mut ffi::SSL) -> c_int {
        (*s).server
    }

    pub unsafe fn SSL_SESSION_up_ref(ses: *mut ffi::SSL_SESSION) -> c_int {
        ffi::CRYPTO_add_lock(
            &mut (*ses).references,
            1,
            ffi::CRYPTO_LOCK_SSL_CTX,
            "mod.rs\0".as_ptr() as *const _,
            line!() as libc::c_int,
        );
        0
    }
}
