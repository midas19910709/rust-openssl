use std::rt::io::Writer;
use std::rt::io::extensions::ReaderUtil;
use std::rt::io::net::tcp::TcpStream;
use std::str;

use super::{Sslv23, SslContext, SslStream, SslVerifyPeer};

#[test]
fn test_new_ctx() {
    SslContext::new(Sslv23);
}

#[test]
fn test_new_sslstream() {
    let stream = TcpStream::connect(FromStr::from_str("127.0.0.1:15418").unwrap()).unwrap();
    SslStream::new(&SslContext::new(Sslv23), stream);
}

#[test]
fn test_verify_untrusted() {
    let stream = TcpStream::connect(FromStr::from_str("127.0.0.1:15418").unwrap()).unwrap();
    let mut ctx = SslContext::new(Sslv23);
    ctx.set_verify(SslVerifyPeer, None);
    match SslStream::try_new(&ctx, stream) {
        Ok(_) => fail!("expected failure"),
        Err(err) => println!("error {:?}", err)
    }
}

#[test]
fn test_verify_trusted() {
    let stream = TcpStream::connect(FromStr::from_str("127.0.0.1:15418").unwrap()).unwrap();
    let mut ctx = SslContext::new(Sslv23);
    ctx.set_verify(SslVerifyPeer, None);
    assert!(ctx.set_CA_file("cert.pem").is_none());
    match SslStream::try_new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => fail!("Expected success, got {:?}", err)
    }
}

#[test]
fn test_verify_untrusted_callback_override_ok() {
    fn callback(_preverify_ok: bool) -> bool {
        true
    }
    let stream = TcpStream::connect(FromStr::from_str("127.0.0.1:15418").unwrap()).unwrap();
    let mut ctx = SslContext::new(Sslv23);
    ctx.set_verify(SslVerifyPeer, Some(callback));
    match SslStream::try_new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => fail!("Expected success, got {:?}", err)
    }
}

#[test]
fn test_verify_untrusted_callback_override_bad() {
    fn callback(_preverify_ok: bool) -> bool {
        false
    }
    let stream = TcpStream::connect(FromStr::from_str("127.0.0.1:15418").unwrap()).unwrap();
    let mut ctx = SslContext::new(Sslv23);
    ctx.set_verify(SslVerifyPeer, Some(callback));
    assert!(SslStream::try_new(&ctx, stream).is_err());
}

#[test]
fn test_verify_trusted_callback_override_ok() {
    fn callback(_preverify_ok: bool) -> bool {
        true
    }
    let stream = TcpStream::connect(FromStr::from_str("127.0.0.1:15418").unwrap()).unwrap();
    let mut ctx = SslContext::new(Sslv23);
    ctx.set_verify(SslVerifyPeer, Some(callback));
    assert!(ctx.set_CA_file("cert.pem").is_none());
    match SslStream::try_new(&ctx, stream) {
        Ok(_) => (),
        Err(err) => fail!("Expected success, got {:?}", err)
    }
}

#[test]
fn test_verify_trusted_callback_override_bad() {
    fn callback(_preverify_ok: bool) -> bool {
        false
    }
    let stream = TcpStream::connect(FromStr::from_str("127.0.0.1:15418").unwrap()).unwrap();
    let mut ctx = SslContext::new(Sslv23);
    ctx.set_verify(SslVerifyPeer, Some(callback));
    assert!(ctx.set_CA_file("cert.pem").is_none());
    assert!(SslStream::try_new(&ctx, stream).is_err());
}

#[test]
fn test_write() {
    let stream = TcpStream::connect(FromStr::from_str("127.0.0.1:15418").unwrap()).unwrap();
    let mut stream = SslStream::new(&SslContext::new(Sslv23), stream);
    stream.write("hello".as_bytes());
    stream.flush();
    stream.write(" there".as_bytes());
    stream.flush();
}

#[test]
fn test_read() {
    let stream = TcpStream::connect(FromStr::from_str("127.0.0.1:15418").unwrap()).unwrap();
    let mut stream = SslStream::new(&SslContext::new(Sslv23), stream);
    stream.write("GET /\r\n\r\n".as_bytes());
    stream.flush();
    let buf = stream.read_to_end();
    print!("{}", str::from_utf8(buf));
}
