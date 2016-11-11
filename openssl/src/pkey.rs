use libc::{c_void, c_char, c_int};
use std::ptr;
use std::mem;
use ffi;

use {cvt, cvt_p};
use bio::{MemBio, MemBioSlice};
use dh::Dh;
use dsa::Dsa;
use ec_key::EcKey;
use rsa::Rsa;
use error::ErrorStack;
use util::{CallbackState, invoke_passwd_cb};
use types::{OpenSslType, OpenSslTypeRef};

type_!(PKey, PKeyRef, ffi::EVP_PKEY, ffi::EVP_PKEY_free);

impl PKeyRef {
    /// Returns a copy of the internal RSA key.
    pub fn rsa(&self) -> Result<Rsa, ErrorStack> {
        unsafe {
            let rsa = try!(cvt_p(ffi::EVP_PKEY_get1_RSA(self.as_ptr())));
            Ok(Rsa::from_ptr(rsa))
        }
    }

    /// Returns a copy of the internal DSA key.
    pub fn dsa(&self) -> Result<Dsa, ErrorStack> {
        unsafe {
            let dsa = try!(cvt_p(ffi::EVP_PKEY_get1_DSA(self.as_ptr())));
            Ok(Dsa::from_ptr(dsa))
        }
    }

    /// Returns a copy of the internal DH key.
    pub fn dh(&self) -> Result<Dh, ErrorStack> {
        unsafe {
            let dh = try!(cvt_p(ffi::EVP_PKEY_get1_DH(self.as_ptr())));
            Ok(Dh::from_ptr(dh))
        }
    }

    /// Returns a copy of the internal elliptic curve key.
    pub fn ec_key(&self) -> Result<EcKey, ErrorStack> {
        unsafe {
            let ec_key = try!(cvt_p(ffi::EVP_PKEY_get1_EC_KEY(self.as_ptr())));
            Ok(EcKey::from_ptr(ec_key))
        }
    }

    /// Stores private key as a PEM
    // FIXME: also add password and encryption
    pub fn private_key_to_pem(&self) -> Result<Vec<u8>, ErrorStack> {
        let mem_bio = try!(MemBio::new());
        unsafe {
            try!(cvt(ffi::PEM_write_bio_PrivateKey(mem_bio.as_ptr(),
                                                   self.as_ptr(),
                                                   ptr::null(),
                                                   ptr::null_mut(),
                                                   -1,
                                                   None,
                                                   ptr::null_mut())));

        }
        Ok(mem_bio.get_buf().to_owned())
    }

    /// Encode public key in PEM format
    pub fn public_key_to_pem(&self) -> Result<Vec<u8>, ErrorStack> {
        let mem_bio = try!(MemBio::new());
        unsafe {
            try!(cvt(ffi::PEM_write_bio_PUBKEY(mem_bio.as_ptr(), self.as_ptr())));
        }
        Ok(mem_bio.get_buf().to_owned())
    }

    /// Encode public key in DER format
    pub fn public_key_to_der(&self) -> Result<Vec<u8>, ErrorStack> {
        let mem_bio = try!(MemBio::new());
        unsafe {
            try!(cvt(ffi::i2d_PUBKEY_bio(mem_bio.as_ptr(), self.as_ptr())));
        }
        Ok(mem_bio.get_buf().to_owned())
    }

    pub fn public_eq(&self, other: &PKeyRef) -> bool {
        unsafe { ffi::EVP_PKEY_cmp(self.as_ptr(), other.as_ptr()) == 1 }
    }
}

unsafe impl Send for PKey {}
unsafe impl Sync for PKey {}

impl PKey {
    /// Creates a new `PKey` containing an RSA key.
    pub fn from_rsa(rsa: Rsa) -> Result<PKey, ErrorStack> {
        unsafe {
            let evp = try!(cvt_p(ffi::EVP_PKEY_new()));
            let pkey = PKey(evp);
            try!(cvt(ffi::EVP_PKEY_assign(pkey.0, ffi::EVP_PKEY_RSA, rsa.as_ptr() as *mut _)));
            mem::forget(rsa);
            Ok(pkey)
        }
    }

    /// Creates a new `PKey` containing a DSA key.
    pub fn from_dsa(dsa: Dsa) -> Result<PKey, ErrorStack> {
        unsafe {
            let evp = try!(cvt_p(ffi::EVP_PKEY_new()));
            let pkey = PKey(evp);
            try!(cvt(ffi::EVP_PKEY_assign(pkey.0, ffi::EVP_PKEY_DSA, dsa.as_ptr() as *mut _)));
            mem::forget(dsa);
            Ok(pkey)
        }
    }

    /// Creates a new `PKey` containing a Diffie-Hellman key.
    pub fn from_dh(dh: Dh) -> Result<PKey, ErrorStack> {
        unsafe {
            let evp = try!(cvt_p(ffi::EVP_PKEY_new()));
            let pkey = PKey(evp);
            try!(cvt(ffi::EVP_PKEY_assign(pkey.0, ffi::EVP_PKEY_DH, dh.as_ptr() as *mut _)));
            mem::forget(dh);
            Ok(pkey)
        }
    }

    /// Creates a new `PKey` containing an elliptic curve key.
    pub fn from_ec_key(ec_key: EcKey) -> Result<PKey, ErrorStack> {
        unsafe {
            let evp = try!(cvt_p(ffi::EVP_PKEY_new()));
            let pkey = PKey(evp);
            try!(cvt(ffi::EVP_PKEY_assign(pkey.0, ffi::EVP_PKEY_EC, ec_key.as_ptr() as *mut _)));
            mem::forget(ec_key);
            Ok(pkey)
        }
    }

    /// Creates a new `PKey` containing an HMAC key.
    pub fn hmac(key: &[u8]) -> Result<PKey, ErrorStack> {
        unsafe {
            assert!(key.len() <= c_int::max_value() as usize);
            let key = try!(cvt_p(ffi::EVP_PKEY_new_mac_key(ffi::EVP_PKEY_HMAC,
                                                           ptr::null_mut(),
                                                           key.as_ptr() as *const _,
                                                           key.len() as c_int)));
            Ok(PKey(key))
        }
    }

    /// Reads private key from PEM, takes ownership of handle
    pub fn private_key_from_pem(buf: &[u8]) -> Result<PKey, ErrorStack> {
        ffi::init();
        let mem_bio = try!(MemBioSlice::new(buf));
        unsafe {
            let evp = try!(cvt_p(ffi::PEM_read_bio_PrivateKey(mem_bio.as_ptr(),
                                                              ptr::null_mut(),
                                                              None,
                                                              ptr::null_mut())));
            Ok(PKey::from_ptr(evp))
        }
    }

    /// Read a private key from PEM, supplying a password callback to be invoked if the private key
    /// is encrypted.
    ///
    /// The callback will be passed the password buffer and should return the number of characters
    /// placed into the buffer.
    pub fn private_key_from_pem_cb<F>(buf: &[u8], pass_cb: F) -> Result<PKey, ErrorStack>
        where F: FnOnce(&mut [c_char]) -> usize
    {
        ffi::init();
        let mut cb = CallbackState::new(pass_cb);
        let mem_bio = try!(MemBioSlice::new(buf));
        unsafe {
            let evp = try!(cvt_p(ffi::PEM_read_bio_PrivateKey(mem_bio.as_ptr(),
                                                              ptr::null_mut(),
                                                              Some(invoke_passwd_cb::<F>),
                                                              &mut cb as *mut _ as *mut c_void)));
            Ok(PKey::from_ptr(evp))
        }
    }

    /// Reads public key from PEM, takes ownership of handle
    pub fn public_key_from_pem(buf: &[u8]) -> Result<PKey, ErrorStack> {
        ffi::init();
        let mem_bio = try!(MemBioSlice::new(buf));
        unsafe {
            let evp = try!(cvt_p(ffi::PEM_read_bio_PUBKEY(mem_bio.as_ptr(),
                                                          ptr::null_mut(),
                                                          None,
                                                          ptr::null_mut())));
            Ok(PKey::from_ptr(evp))
        }
    }
}

#[cfg(test)]
mod tests {
    use dh::Dh;
    use dsa::Dsa;
    use ec_key::EcKey;
    use rsa::Rsa;
    use nid;

    use super::*;

    #[test]
    fn test_private_key_from_pem() {
        let key = include_bytes!("../test/key.pem");
        PKey::private_key_from_pem(key).unwrap();
    }

    #[test]
    fn test_public_key_from_pem() {
        let key = include_bytes!("../test/key.pem.pub");
        PKey::public_key_from_pem(key).unwrap();
    }

    #[test]
    fn test_pem() {
        let key = include_bytes!("../test/key.pem");
        let key = PKey::private_key_from_pem(key).unwrap();

        let priv_key = key.private_key_to_pem().unwrap();
        let pub_key = key.public_key_to_pem().unwrap();

        // As a super-simple verification, just check that the buffers contain
        // the `PRIVATE KEY` or `PUBLIC KEY` strings.
        assert!(priv_key.windows(11).any(|s| s == b"PRIVATE KEY"));
        assert!(pub_key.windows(10).any(|s| s == b"PUBLIC KEY"));
    }

    #[test]
    fn test_rsa_accessor() {
        let rsa = Rsa::generate(2048).unwrap();
        let pkey = PKey::from_rsa(rsa).unwrap();
        pkey.rsa().unwrap();
        assert!(pkey.dsa().is_err());
    }

    #[test]
    fn test_dsa_accessor() {
        let dsa = Dsa::generate(2048).unwrap();
        let pkey = PKey::from_dsa(dsa).unwrap();
        pkey.dsa().unwrap();
        assert!(pkey.rsa().is_err());
    }

    #[test]
    fn test_dh_accessor() {
        let dh = include_bytes!("../test/dhparams.pem");
        let dh = Dh::from_pem(dh).unwrap();
        let pkey = PKey::from_dh(dh).unwrap();
        pkey.dh().unwrap();
        assert!(pkey.rsa().is_err());
    }

    #[test]
    fn test_ec_key_accessor() {
        let ec_key = EcKey::new_by_curve_name(nid::X9_62_PRIME256V1).unwrap();
        let pkey = PKey::from_ec_key(ec_key).unwrap();
        pkey.ec_key().unwrap();
        assert!(pkey.rsa().is_err());
    }
}
