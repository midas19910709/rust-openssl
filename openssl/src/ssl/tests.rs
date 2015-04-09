#![allow(unused_imports)]

use std::net::TcpStream;
use std::io;
use std::io::prelude::*;
use std::path::Path;
#[cfg(feature = "npn")]
use std::net::TcpListener;
#[cfg(feature = "npn")]
use std::thread;
use std::fs::File;

use crypto::hash::Type::{SHA256};
use ssl;
use ssl::SslMethod;
use ssl::SslMethod::Sslv23;
use ssl::{SslContext, SslStream, VerifyCallback};
use ssl::SSL_VERIFY_PEER;
use x509::X509StoreContext;
#[cfg(feature = "npn")]
use x509::X509FileType;
use x509::X509;
use crypto::pkey::PKey;

#[cfg(feature="dtlsv1")]
use std::net::UdpSocket;
#[cfg(feature="dtlsv1")]
use ssl::SslMethod::Dtlsv1;
#[cfg(feature="dtlsv1")]
use connected_socket::Connect;

#[cfg(feature = "dtlsv1")]
mod udp {
    use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

    static UDP_PORT: AtomicUsize = ATOMIC_USIZE_INIT;

    pub fn next_server<'a>() -> String {
        let diff = UDP_PORT.fetch_add(1, Ordering::SeqCst);
        format!("127.0.0.1:{}", 15411 + diff)
    }
}

macro_rules! run_test(
    ($module:ident, $blk:expr) => (
        #[cfg(test)]
        mod $module {
            use super::udp;
            use std::io;
            use std::io::prelude::*;
            use std::path::Path;
            use std::net::UdpSocket;
            use std::net::TcpStream;
            use ssl;
            use ssl::SslMethod;
            use ssl::{SslContext, SslStream, VerifyCallback};
            #[cfg(feature="dtlsv1")]
            use connected_socket::Connect;
            use ssl::SSL_VERIFY_PEER;
            use crypto::hash::Type::SHA256;
            use x509::X509StoreContext;
            use serialize::hex::FromHex;

            #[test]
            fn sslv23() {
                let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
                $blk(SslMethod::Sslv23, stream);
            }

            #[test]
            #[cfg(feature="dtlsv1")]
            fn dtlsv1() {
                let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
                let server = udp::next_server();
                let stream = sock.connect(&server[..]).unwrap();

                $blk(SslMethod::Dtlsv1, stream);
            }
        }
    );
);

run_test!(new_ctx, |method, _| {
    SslContext::new(method).unwrap();
});

run_test!(new_sslstream, |method, stream| {
    SslStream::new(&SslContext::new(method).unwrap(), stream).unwrap();
});

run_test!(verify_untrusted, |method, stream| {
    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, None);

    match SslStream::new(&ctx, stream) {
        Ok(_) => panic!("expected failure"),
        Err(err) => println!("error {:?}", err)
    }
});

run_test!(verify_trusted, |method, stream| {
    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, None);

    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        Ok(_) => {}
        Err(err) => panic!("Unexpected error {:?}", err)
    }
    match SslStream::new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => panic!("Expected success, got {:?}", err)
    }
});

run_test!(verify_untrusted_callback_override_ok, |method, stream| {
    fn callback(_preverify_ok: bool, _x509_ctx: &X509StoreContext) -> bool {
        true
    }

    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, Some(callback as VerifyCallback));

    match SslStream::new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => panic!("Expected success, got {:?}", err)
    }
});

run_test!(verify_untrusted_callback_override_bad, |method, stream| {
    fn callback(_preverify_ok: bool, _x509_ctx: &X509StoreContext) -> bool {
        false
    }

    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, Some(callback as VerifyCallback));

    assert!(SslStream::new(&ctx, stream).is_err());
});

run_test!(verify_trusted_callback_override_ok, |method, stream| {
    fn callback(_preverify_ok: bool, _x509_ctx: &X509StoreContext) -> bool {
        true
    }

    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, Some(callback as VerifyCallback));

    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        Ok(_) => {}
        Err(err) => panic!("Unexpected error {:?}", err)
    }
    match SslStream::new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => panic!("Expected success, got {:?}", err)
    }
});

run_test!(verify_trusted_callback_override_bad, |method, stream| {
    fn callback(_preverify_ok: bool, _x509_ctx: &X509StoreContext) -> bool {
        false
    }

    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, Some(callback as VerifyCallback));

    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        Ok(_) => {}
        Err(err) => panic!("Unexpected error {:?}", err)
    }
    assert!(SslStream::new(&ctx, stream).is_err());
});

run_test!(verify_callback_load_certs, |method, stream| {
    fn callback(_preverify_ok: bool, x509_ctx: &X509StoreContext) -> bool {
        assert!(x509_ctx.get_current_cert().is_some());
        true
    }

    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, Some(callback as VerifyCallback));

    assert!(SslStream::new(&ctx, stream).is_ok());
});

run_test!(verify_trusted_get_error_ok, |method, stream| {
    fn callback(_preverify_ok: bool, x509_ctx: &X509StoreContext) -> bool {
        assert!(x509_ctx.get_error().is_none());
        true
    }

    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, Some(callback as VerifyCallback));

    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        Ok(_) => {}
        Err(err) => panic!("Unexpected error {:?}", err)
    }
    assert!(SslStream::new(&ctx, stream).is_ok());
});

run_test!(verify_trusted_get_error_err, |method, stream| {
    fn callback(_preverify_ok: bool, x509_ctx: &X509StoreContext) -> bool {
        assert!(x509_ctx.get_error().is_some());
        false
    }

    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, Some(callback as VerifyCallback));

    assert!(SslStream::new(&ctx, stream).is_err());
});

run_test!(verify_callback_data, |method, stream| {
    fn callback(_preverify_ok: bool, x509_ctx: &X509StoreContext, node_id: &Vec<u8>) -> bool {
        let cert = x509_ctx.get_current_cert();
        match cert {
            None => false,
            Some(cert) => {
                let fingerprint = cert.fingerprint(SHA256).unwrap();
                &fingerprint == node_id
            }
        }
    }
    let mut ctx = SslContext::new(method).unwrap();

    // Node id was generated as SHA256 hash of certificate "test/cert.pem"
    // in DER format.
    // Command: openssl x509 -in test/cert.pem  -outform DER | openssl dgst -sha256
    // Please update if "test/cert.pem" will ever change
    let node_hash_str = "46e3f1a6d17a41ce70d0c66ef51cee2ab4ba67cac8940e23f10c1f944b49fb5c";
    let node_id = node_hash_str.from_hex().unwrap();
    ctx.set_verify_with_data(SSL_VERIFY_PEER, callback, node_id);
    ctx.set_verify_depth(1);

    match SslStream::new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => panic!("Expected success, got {:?}", err)
    }
});

#[test]
fn test_set_certificate_and_private_key() {
    let key_path = Path::new("test/key.pem");
    let cert_path = Path::new("test/cert.pem");
    let mut key_file = File::open(&key_path)
        .ok()
        .expect("Failed to open `test/key.pem`");
    let mut cert_file = File::open(&cert_path)
        .ok()
        .expect("Failed to open `test/cert.pem`");

    let key = PKey::private_key_from_pem(&mut key_file).unwrap();
    let cert = X509::from_pem(&mut cert_file).unwrap();

    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_private_key(&key).unwrap();
    ctx.set_certificate(&cert).unwrap();

    assert!(ctx.check_private_key().is_ok());
}

run_test!(get_ctx_options, |method, _| {
    let mut ctx = SslContext::new(method).unwrap();
    ctx.get_options();
});

run_test!(set_ctx_options, |method, _| {
    let mut ctx = SslContext::new(method).unwrap();
    let opts = ctx.set_options(ssl::SSL_OP_NO_TICKET);
    assert!(opts.contains(ssl::SSL_OP_NO_TICKET));
    assert!(!opts.contains(ssl::SSL_OP_CISCO_ANYCONNECT));
    let more_opts = ctx.set_options(ssl::SSL_OP_CISCO_ANYCONNECT);
    assert!(more_opts.contains(ssl::SSL_OP_NO_TICKET));
    assert!(more_opts.contains(ssl::SSL_OP_CISCO_ANYCONNECT));
});

run_test!(clear_ctx_options, |method, _| {
    let mut ctx = SslContext::new(method).unwrap();
    ctx.set_options(ssl::SSL_OP_ALL);
    let opts = ctx.clear_options(ssl::SSL_OP_ALL);
    assert!(!opts.contains(ssl::SSL_OP_ALL));
});

#[test]
fn test_write() {
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut stream = SslStream::new(&SslContext::new(Sslv23).unwrap(), stream).unwrap();
    stream.write_all("hello".as_bytes()).unwrap();
    stream.flush().unwrap();
    stream.write_all(" there".as_bytes()).unwrap();
    stream.flush().unwrap();
}

#[test]
#[cfg(feature = "dtlsv1")]
fn test_write_dtlsv1() {
    let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    let stream = sock.connect("127.0.0.1:15410").unwrap();

    let mut stream = SslStream::new(&SslContext::new(Dtlsv1).unwrap(), stream).unwrap();
    stream.write_all("hello".as_bytes()).unwrap();
    stream.flush().unwrap();
    stream.write_all(" there".as_bytes()).unwrap();
    stream.flush().unwrap();
}

#[test]
fn test_read() {
    let tcp = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut stream = SslStream::new(&SslContext::new(Sslv23).unwrap(), tcp).unwrap();
    stream.write_all("GET /\r\n\r\n".as_bytes()).unwrap();
    stream.flush().unwrap();
    io::copy(&mut stream, &mut io::sink()).ok().expect("read error");
}

/// Tests that connecting with the client using NPN, but the server not does not
/// break the existing connection behavior.
#[test]
#[cfg(feature = "npn")]
fn test_connect_with_unilateral_npn() {
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, None);
    ctx.set_npn_protocols(&[b"http/1.1", b"spdy/3.1"]);
    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        Ok(_) => {}
        Err(err) => panic!("Unexpected error {:?}", err)
    }
    let stream = match SslStream::new(&ctx, stream) {
        Ok(stream) => stream,
        Err(err) => panic!("Expected success, got {:?}", err)
    };
    // Since the socket to which we connected is not configured to use NPN,
    // there should be no selected protocol...
    assert!(stream.get_selected_npn_protocol().is_none());
}

/// Tests that when both the client as well as the server use NPN and their
/// lists of supported protocols have an overlap, the correct protocol is chosen.
#[test]
#[cfg(feature = "npn")]
fn test_connect_with_npn_successful_multiple_matching() {
    // A different port than the other tests: an `openssl` process that has
    // NPN enabled.
    let stream = TcpStream::connect("127.0.0.1:15419").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, None);
    ctx.set_npn_protocols(&[b"spdy/3.1", b"http/1.1"]);
    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        Ok(_) => {}
        Err(err) => panic!("Unexpected error {:?}", err)
    }
    let stream = match SslStream::new(&ctx, stream) {
        Ok(stream) => stream,
        Err(err) => panic!("Expected success, got {:?}", err)
    };
    // The server prefers "http/1.1", so that is chosen, even though the client
    // would prefer "spdy/3.1"
    assert_eq!(b"http/1.1", stream.get_selected_npn_protocol().unwrap());
}

/// Tests that when both the client as well as the server use NPN and their
/// lists of supported protocols have an overlap -- with only ONE protocol
/// being valid for both.
#[test]
#[cfg(feature = "npn")]
fn test_connect_with_npn_successful_single_match() {
    // A different port than the other tests: an `openssl` process that has
    // NPN enabled.
    let stream = TcpStream::connect("127.0.0.1:15419").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, None);
    ctx.set_npn_protocols(&[b"spdy/3.1"]);
    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        Ok(_) => {}
        Err(err) => panic!("Unexpected error {:?}", err)
    }
    let stream = match SslStream::new(&ctx, stream) {
        Ok(stream) => stream,
        Err(err) => panic!("Expected success, got {:?}", err)
    };
    // The client now only supports one of the server's protocols, so that one
    // is used.
    assert_eq!(b"spdy/3.1", stream.get_selected_npn_protocol().unwrap());
}

/// Tests that when the `SslStream` is created as a server stream, the protocols
/// are correctly advertised to the client.
#[test]
#[cfg(feature = "npn")]
fn test_npn_server_advertise_multiple() {
    let localhost = "127.0.0.1:15420";
    let listener = TcpListener::bind(localhost).unwrap();
    // We create a different context instance for the server...
    let listener_ctx = {
        let mut ctx = SslContext::new(Sslv23).unwrap();
        ctx.set_verify(SSL_VERIFY_PEER, None);
        ctx.set_npn_protocols(&[b"http/1.1", b"spdy/3.1"]);
        assert!(ctx.set_certificate_file(
                &Path::new("test/cert.pem"), X509FileType::PEM).is_ok());
        ctx.set_private_key_file(
                &Path::new("test/key.pem"), X509FileType::PEM).unwrap();
        ctx
    };
    // Have the listener wait on the connection in a different thread.
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let _ = SslStream::new_server(&listener_ctx, stream).unwrap();
    });

    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SSL_VERIFY_PEER, None);
    ctx.set_npn_protocols(&[b"spdy/3.1"]);
    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        Ok(_) => {}
        Err(err) => panic!("Unexpected error {:?}", err)
    }
    // Now connect to the socket and make sure the protocol negotiation works...
    let stream = TcpStream::connect(localhost).unwrap();
    let stream = match SslStream::new(&ctx, stream) {
        Ok(stream) => stream,
        Err(err) => panic!("Expected success, got {:?}", err)
    };
    // SPDY is selected since that's the only thing the client supports.
    assert_eq!(b"spdy/3.1", stream.get_selected_npn_protocol().unwrap());
}

#[cfg(feature="dtlsv1")]
#[cfg(test)]
mod dtlsv1 {
    use serialize::hex::FromHex;
    use std::net::TcpStream;
    use std::old_io::{Writer};
    use std::thread;

    use crypto::hash::Type::{SHA256};
    use ssl::SslMethod;
    use ssl::SslMethod::Dtlsv1;
    use ssl::{SslContext, SslStream, VerifyCallback};
    use ssl::SSL_VERIFY_PEER;
    use x509::{X509StoreContext};

    const PROTOCOL:SslMethod = Dtlsv1;

    #[test]
    fn test_new_ctx() {
        SslContext::new(PROTOCOL).unwrap();
    }
}

#[test]
#[cfg(feature = "dtlsv1")]
fn test_read_dtlsv1() {
    let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    let server = udp::next_server();
    let stream = sock.connect(&server[..]).unwrap();

    let mut stream = SslStream::new(&SslContext::new(Dtlsv1).unwrap(), stream).unwrap();
    let mut buf = [0u8;100];
    assert!(stream.read(&mut buf).is_ok());
}
