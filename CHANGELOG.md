# Change Log

## [Unreleased]

## [v0.10.13] - 2018-10-14

### Fixed

* Fixed a double-free in the `SslContextBuilder::set_get_session_callback` API.

### Added

* Added `SslContextBuilder::set_client_hello_callback`.
* Added support for LibreSSL 2.8.1.
* Added `EcdsaSig::from_der` and `EcdsaSig::to_der`.
* Added PKCS#7 support.

## [v0.10.12] - 2018-09-13

### Fixed

* Fixed handling of SNI callbacks during renegotiation.

### Added

* Added `SslRef::get_shutdown` and `SslRef::set_shutdown`.
* Added support for SRTP in DTLS sessions.
* Added support for LibreSSL 2.8.0.

## [v0.10.11] - 2018-08-04

### Added

* The new `vendored` cargo feature will cause openssl-sys to compile and statically link to a
    vendored copy of OpenSSL.
* Added `SslContextBuilder::set_psk_server_callback`.
* Added `DsaRef::pub_key` and `DsaRef::priv_key`.
* Added `Dsa::from_private_components` and `Dsa::from_public_components`.
* Added `X509NameRef::entries`.

### Deprecated

* `SslContextBuilder::set_psk_callback` has been renamed to
    `SslContextBuilder::set_psk_client_callback` and deprecated.

## [v0.10.10] - 2018-06-06

### Added

* Added `SslRef::set_alpn_protos`.
* Added `SslContextBuilder::set_ciphersuites`.

## [v0.10.9] - 2018-06-01

### Fixed

* Fixed a use-after-free in `CmsContentInfo::sign`.
* `SslRef::servername` now returns `None` rather than panicking on a non-UTF8 name.

### Added

* Added `MessageDigest::from_nid`.
* Added `Nid::signature_algorithms`, `Nid::long_name`, and `Nid::short_name`.
* Added early data and early keying material export support for TLS 1.3.
* Added `SslRef::verified_chain`.
* Added `SslRef::servername_raw` which returns a `&[u8]` rather than `&str`.
* Added `SslRef::finished` and `SslRef::peer_finished`.
* Added `X509Ref::digest` to replace `X509Ref::fingerprint`.
* `X509StoreBuilder` and `X509Store` now implement `Sync` and `Send`.

### Deprecated

* `X509Ref::fingerprint` has been deprecated in favor of `X509Ref::digest`.

## [v0.10.8] - 2018-05-20

### Fixed

* `openssl-sys` will now detect Homebrew-installed OpenSSL when installed to a non-default
    directory.
* The `X509_V_ERR_INVALID_CALL`, `X509_V_ERR_STORE_LOOKUP`, and
    `X509_V_ERR_PROXY_SUBJECT_NAME_VIOLATION` constants in `openssl-sys` are now only present when
    building against 1.1.0g and up rather than 1.1.0.
* `SslContextBuilder::max_proto_version` and `SslContextBuilder::min_proto_version` are only present
    when building against 1.1.0g and up rather than 1.1.0.

### Added

* Added `CmsContentInfo::sign`.
* Added `Clone` and `ToOwned` implementations to `Rsa` and `RsaRef` respectively.
* The `min_proto_version` and `max_proto_version` methods are available when linking against
    LibreSSL 2.6.1 and up in addition to OpenSSL.
* `X509VerifyParam` is available when linking against LibreSSL 2.6.1 and up in addition to OpenSSL.
* ALPN support is available when linking against LibreSSL 2.6.1 and up in addition to OpenSSL.
* `Stack` and `StackRef` are now `Sync` and `Send`.

## [v0.10.7] - 2018-04-30

### Added

* Added `X509Req::public_key` and `X509Req::extensions`.
* Added `RsaPrivateKeyBuilder` to allow control over initialization of optional components of an RSA
    private key.
* Added DER encode/decode support to `SslSession`.
* openssl-sys now provides the `DEP_OPENSSL_VERSION_NUMBER` and
    `DEP_OPENSSL_LIBRESSL_VERSION_NUMBER` environment variables to downstream build scripts which
    contains the hex-encoded version number of the OpenSSL or LibreSSL distribution being built
    against. The other variables are deprecated.

## [v0.10.6] - 2018-03-05

### Added

* Added `SslOptions::ENABLE_MIDDLEBOX_COMPAT`.
* Added more `Sync` and `Send` implementations.
* Added `PKeyRef::id`.
* Added `Padding::PKCS1_PSS`.
* Added `Signer::set_rsa_pss_saltlen`, `Signer::set_rsa_mgf1_md`, `Signer::set_rsa_pss_saltlen`, and
    `Signer::set_rsa_mgf1_md`
* Added `X509StoreContextRef::verify` to directly verify certificates.
* Added low level ECDSA support.
* Added support for TLSv1.3 custom extensions. (OpenSSL 1.1.1 only)
* Added AES-CCM support.
* Added `EcKey::from_private_components`.
* Added CMAC support.
* Added support for LibreSSL 2.7.
* Added `X509Ref::serial_number`.
* Added `Asn1IntegerRef::to_bn`.
* Added support for TLSv1.3 stateless handshakes. (OpenSSL 1.1.1 only)

### Changed

* The Cargo features previously used to gate access to version-specific OpenSSL APIs have been
    removed. Those APIs will be available automatically when building against an appropriate OpenSSL
    version.
* Fixed `PKey::private_key_from_der` to return a `PKey<Private>` rather than a `PKey<Public>`. This
    is technically a breaking change but the function was pretty useless previously.

### Deprecated

* `X509CheckFlags::FLAG_NO_WILDCARDS` has been renamed to `X509CheckFlags::NO_WILDCARDS` and the old
    name deprecated.

## [v0.10.5] - 2018-02-28

### Fixed

* `ErrorStack`'s `Display` implementation no longer writes an empty string if it contains no errors.

### Added

* Added `SslRef::version2`.
* Added `Cipher::des_ede3_cbc`.
* Added `SslRef::export_keying_material`.
* Added the ability to push an `Error` or `ErrorStack` back onto OpenSSL's error stack. Various
    callback bindings use this to propagate errors properly.
* Added `SslContextBuilder::set_cookie_generate_cb` and `SslContextBuilder::set_cookie_verify_cb`.
* Added `SslContextBuilder::set_max_proto_version`, `SslContextBuilder::set_min_proto_version`,
    `SslContextBuilder::max_proto_version`, and `SslContextBuilder::min_proto_version`.

### Changed

* Updated `SslConnector`'s default cipher list to match Python's.

### Deprecated

* `SslRef::version` has been deprecated. Use `SslRef::version_str` instead.

## [v0.10.4] - 2018-02-18

### Added

* Added OpenSSL 1.1.1 support.
* Added `Rsa::public_key_from_pem_pkcs1`.
* Added `SslOptions::NO_TLSV1_3`. (OpenSSL 1.1.1 only)
* Added `SslVersion`.
* Added `SslSessionCacheMode` and `SslContextBuilder::set_session_cache_mode`.
* Added `SslContextBuilder::set_new_session_callback`,
    `SslContextBuilder::set_remove_session_callback`, and
    `SslContextBuilder::set_get_session_callback`.
* Added `SslContextBuilder::set_keylog_callback`. (OpenSSL 1.1.1 only)
* Added `SslRef::client_random` and `SslRef::server_random`. (OpenSSL 1.1.0+ only)

### Fixed

* The `SslAcceptorBuilder::mozilla_modern` constructor now disables TLSv1.0 and TLSv1.1 in
    accordance with Mozilla's recommendations.

## [v0.10.3] - 2018-02-12

### Added

* OpenSSL is now automatically detected on FreeBSD systems.
* Added `GeneralName` accessors for `rfc822Name` and `uri` variants.
* Added DES-EDE3 support.

### Fixed

* Fixed a memory leak in `X509StoreBuilder::add_cert`.

## [v0.10.2] - 2018-01-11

### Added

* Added `ConnectConfiguration::set_use_server_name_indication` and
    `ConnectConfiguration::set_verify_hostname` for use in contexts where you don't have ownership
    of the `ConnectConfiguration`.

## [v0.10.1] - 2018-01-10

### Added

* Added a `From<ErrorStack> for ssl::Error` implementation.

## [v0.10.0] - 2018-01-10

### Compatibility

* openssl 0.10 still uses openssl-sys 0.9, so openssl 0.9 and 0.10 can coexist without issue.

### Added

* The `ssl::select_next_proto` function can be used to easily implement the ALPN selection callback
    in a "standard" way.
* FIPS mode support is available in the `fips` module.
* Accessors for the Issuer and Issuer Alternative Name fields of X509 certificates have been added.
* The `X509VerifyResult` can now be set in the certificate verification callback via
    `X509StoreContextRef::set_error`.

### Changed

* All constants have been moved to associated constants of their type. For example, `bn::MSB_ONE`
    is now `bn::MsbOption::ONE`.
* Asymmetric key types are now parameterized over what they contain. In OpenSSL, the same type is
    used for key parameters, public keys, and private keys. Unfortunately, some APIs simply assume
    that certain components are present and will segfault trying to use things that aren't there.

    The `pkey` module contains new tag types named `Params`, `Public`, and `Private`, and the
    `Dh`, `Dsa`, `EcKey`, `Rsa`, and `PKey` have a type parameter set to one of those values. This
    allows the `Signer` constructor to indicate that it requires a private key at compile time for
    example. Previously, `Signer` would simply segfault if provided a key without private
    components.
* ALPN support has been changed to more directly model OpenSSL's own APIs. Instead of a single
    method used for both the server and client sides which performed everything automatically, the
    `SslContextBuilder::set_alpn_protos` and `SslContextBuilder::set_alpn_select_callback` handle
    the client and server sides respectively.
* `SslConnector::danger_connect_without_providing_domain_for_certificate_verification_and_server_name_indication`
    has been removed in favor of new methods which provide more control. The
    `ConnectConfiguration::use_server_name_indication` method controls the use of Server Name
    Indication (SNI), and the `ConnectConfiguration::verify_hostname` method controls the use of
    hostname verification. These can be controlled independently, and if both are disabled, the
    domain argument to `ConnectConfiguration::connect` is ignored.
* Shared secret derivation is now handled by the new `derive::Deriver` type rather than
    `pkey::PKeyContext`, which has been removed.
* `ssl::Error` is now no longer an enum, and provides more direct access to the relevant state.
* `SslConnectorBuilder::new` has been moved and renamed to `SslConnector::builder`.
* `SslAcceptorBuilder::mozilla_intermediate` and `SslAcceptorBuilder::mozilla_modern` have been
    moved to `SslAcceptor` and no longer take the private key and certificate chain. Install those
    manually after creating the builder.
* `X509VerifyError` is now `X509VerifyResult` and can now have the "ok" value in addition to error
    values.
* `x509::X509FileType` is now `ssl::SslFiletype`.
* Asymmetric key serialization and deserialization methods now document the formats that they
    correspond to, and some have been renamed to better indicate that.

### Removed

* All deprecated APIs have been removed.
* NPN support has been removed. It has been supersceded by ALPN, and is hopefully no longer being
    used in practice. If you still depend on it, please file an issue!
* `SslRef::compression` has been removed.
* Some `ssl::SslOptions` flags have been removed as they no longer do anything.

## Older

Look at the [release tags] for information about older releases.

[Unreleased]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.13...master
[v0.10.13]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.12...openssl-v0.10.13
[v0.10.12]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.11...openssl-v0.10.12
[v0.10.11]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.10...openssl-v0.10.11
[v0.10.10]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.9...openssl-v0.10.10
[v0.10.9]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.8...openssl-v0.10.9
[v0.10.8]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.7...openssl-v0.10.8
[v0.10.7]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.6...openssl-v0.10.7
[v0.10.6]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.5...openssl-v0.10.6
[v0.10.5]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.4...openssl-v0.10.5
[v0.10.4]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.3...openssl-v0.10.4
[v0.10.3]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.2...openssl-v0.10.3
[v0.10.2]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.1...openssl-v0.10.2
[v0.10.1]: https://github.com/sfackler/rust-openssl/compare/openssl-v0.10.0...openssl-v0.10.1
[v0.10.0]: https://github.com/sfackler/rust-openssl/compare/v0.9.23...openssl-v0.10.0
[release tags]: https://github.com/sfackler/rust-openssl/releases
