use serialize::hex::FromHex;
use std::io::{Writer};
use std::io::net::tcp::TcpStream;

use crypto::hash::HashType::{SHA256};
use ssl::SslMethod::Sslv23;
use ssl::{SslContext, SslStream};
use ssl::SslVerifyMode::SslVerifyPeer;
use x509::{X509StoreContext};

#[test]
fn test_new_ctx() {
    SslContext::new(Sslv23).unwrap();
}

#[test]
fn test_new_sslstream() {
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    SslStream::new(&SslContext::new(Sslv23).unwrap(), stream).unwrap();
}

#[test]
fn test_verify_untrusted() {
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SslVerifyPeer, None);
    match SslStream::new(&ctx, stream) {
        Ok(_) => panic!("expected failure"),
        Err(err) => println!("error {}", err)
    }
}

#[test]
fn test_verify_trusted() {
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SslVerifyPeer, None);
    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        None => {}
        Some(err) => panic!("Unexpected error {}", err)
    }
    match SslStream::new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => panic!("Expected success, got {}", err)
    }
}

#[test]
fn test_verify_untrusted_callback_override_ok() {
    fn callback(_preverify_ok: bool, _x509_ctx: &X509StoreContext) -> bool {
        true
    }
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SslVerifyPeer, Some(callback));
    match SslStream::new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => panic!("Expected success, got {}", err)
    }
}

#[test]
fn test_verify_untrusted_callback_override_bad() {
    fn callback(_preverify_ok: bool, _x509_ctx: &X509StoreContext) -> bool {
        false
    }
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SslVerifyPeer, Some(callback));
    assert!(SslStream::new(&ctx, stream).is_err());
}

#[test]
fn test_verify_trusted_callback_override_ok() {
    fn callback(_preverify_ok: bool, _x509_ctx: &X509StoreContext) -> bool {
        true
    }
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SslVerifyPeer, Some(callback));
    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        None => {}
        Some(err) => panic!("Unexpected error {}", err)
    }
    match SslStream::new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => panic!("Expected success, got {}", err)
    }
}

#[test]
fn test_verify_trusted_callback_override_bad() {
    fn callback(_preverify_ok: bool, _x509_ctx: &X509StoreContext) -> bool {
        false
    }
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SslVerifyPeer, Some(callback));
    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        None => {}
        Some(err) => panic!("Unexpected error {}", err)
    }
    assert!(SslStream::new(&ctx, stream).is_err());
}

#[test]
fn test_verify_callback_load_certs() {
    fn callback(_preverify_ok: bool, x509_ctx: &X509StoreContext) -> bool {
        assert!(x509_ctx.get_current_cert().is_some());
        true
    }
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SslVerifyPeer, Some(callback));
    assert!(SslStream::new(&ctx, stream).is_ok());
}

#[test]
fn test_verify_trusted_get_error_ok() {
    fn callback(_preverify_ok: bool, x509_ctx: &X509StoreContext) -> bool {
        assert!(x509_ctx.get_error().is_none());
        true
    }
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SslVerifyPeer, Some(callback));
    match ctx.set_CA_file(&Path::new("test/cert.pem")) {
        None => {}
        Some(err) => panic!("Unexpected error {}", err)
    }
    assert!(SslStream::new(&ctx, stream).is_ok());
}

#[test]
fn test_verify_trusted_get_error_err() {
    fn callback(_preverify_ok: bool, x509_ctx: &X509StoreContext) -> bool {
        assert!(x509_ctx.get_error().is_some());
        false
    }
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();
    ctx.set_verify(SslVerifyPeer, Some(callback));
    assert!(SslStream::new(&ctx, stream).is_err());
}

#[test]
fn test_verify_callback_data() {
    fn callback(_preverify_ok: bool, x509_ctx: &X509StoreContext, node_id: &Vec<u8>) -> bool {
        let cert = x509_ctx.get_current_cert();
        match cert {
            None => false,
            Some(cert) => {
                let fingerprint = cert.fingerprint(SHA256).unwrap();
                fingerprint.as_slice() == node_id.as_slice()
            }
        }
    }
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut ctx = SslContext::new(Sslv23).unwrap();

    // Node id was generated as SHA256 hash of certificate "test/cert.pem"
    // in DER format.
    // Command: openssl x509 -in test/cert.pem  -outform DER | openssl dgst -sha256
    // Please update if "test/cert.pem" will ever change
    let node_hash_str = "46e3f1a6d17a41ce70d0c66ef51cee2ab4ba67cac8940e23f10c1f944b49fb5c";
    let node_id = node_hash_str.from_hex().unwrap();
    ctx.set_verify_with_data(SslVerifyPeer, callback, node_id);
    ctx.set_verify_depth(1);

    match SslStream::new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => panic!("Expected success, got {}", err)
    }
}


#[test]
fn test_write() {
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut stream = SslStream::new(&SslContext::new(Sslv23).unwrap(), stream).unwrap();
    stream.write("hello".as_bytes()).unwrap();
    stream.flush().unwrap();
    stream.write(" there".as_bytes()).unwrap();
    stream.flush().unwrap();
}

#[test]
fn test_read() {
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut stream = SslStream::new(&SslContext::new(Sslv23).unwrap(), stream).unwrap();
    stream.write("GET /\r\n\r\n".as_bytes()).unwrap();
    stream.flush().unwrap();
    stream.read_to_end().ok().expect("read error");
}

#[test]
fn test_clone() {
    let stream = TcpStream::connect("127.0.0.1:15418").unwrap();
    let mut stream = SslStream::new(&SslContext::new(Sslv23).unwrap(), stream).unwrap();
    let mut stream2 = stream.clone();
    spawn(proc() {
        stream2.write("GET /\r\n\r\n".as_bytes()).unwrap();
        stream2.flush().unwrap();
    });
    stream.read_to_end().ok().expect("read error");
}
