
macro_rules! type_ {
    ($n:ident, $r:ident, $c:path, $d:path) => {
        pub struct $n(*mut $c);

        impl ::types::OpenSslType for $n {
            type CType = $c;
            type Ref = $r;

            unsafe fn from_ptr(ptr: *mut $c) -> $n {
                $n(ptr)
            }
        }

        impl Drop for $n {
            fn drop(&mut self) {
                unsafe { $d(self.0) }
            }
        }

        impl ::std::ops::Deref for $n {
            type Target = $r;

            fn deref(&self) -> &$r {
                unsafe { ::types::OpenSslTypeRef::from_ptr(self.0) }
            }
        }

        impl ::std::ops::DerefMut for $n {
            fn deref_mut(&mut self) -> &mut $r {
                unsafe { ::types::OpenSslTypeRef::from_ptr_mut(self.0) }
            }
        }

        pub struct $r(::util::Opaque);

        impl ::types::OpenSslTypeRef for $r {
            type CType = $c;
        }
    }
}

macro_rules! private_key_from_pem {
    ($t:ident, $f:path) => {
        /// Deserializes a PEM-formatted private key.
        pub fn private_key_from_pem(pem: &[u8]) -> Result<$t, ::error::ErrorStack> {
            unsafe {
                ::init();
                let bio = try!(::bio::MemBioSlice::new(pem));
                cvt_p($f(bio.as_ptr(), ::std::ptr::null_mut(), None, ::std::ptr::null_mut()))
                    .map($t)
            }
        }

        /// Deserializes a PEM-formatted private key, using the supplied password if the key is
        /// encrypted.
        ///
        /// # Panics
        ///
        /// Panics if `passphrase` contains an embedded null.
        pub fn private_key_from_pem_passphrase(pem: &[u8],
                                               passphrase: &[u8])
                                               -> Result<$t, ::error::ErrorStack> {
            unsafe {
                ffi::init();
                let bio = try!(::bio::MemBioSlice::new(pem));
                let passphrase = ::std::ffi::CString::new(passphrase).unwrap();
                cvt_p($f(bio.as_ptr(),
                         ptr::null_mut(),
                         None,
                         passphrase.as_ptr() as *const _ as *mut _))
                    .map($t)
            }
        }

        /// Deserializes a PEM-formatted private key, using a callback to retrieve a password if the
        /// key is encrypted.
        ///
        /// The callback should copy the password into the provided buffer and return the number of
        /// bytes written.
        pub fn private_key_from_pem_callback<F>(pem: &[u8],
                                                callback: F)
                                                -> Result<$t, ::error::ErrorStack>
            where F: FnOnce(&mut [u8]) -> Result<usize, ::error::ErrorStack>
        {
            unsafe {
                ffi::init();
                let mut cb = ::util::CallbackState::new(callback);
                let bio = try!(::bio::MemBioSlice::new(pem));
                cvt_p($f(bio.as_ptr(),
                         ptr::null_mut(),
                         Some(::util::invoke_passwd_cb::<F>),
                         &mut cb as *mut _ as *mut _))
                    .map($t)
            }
        }
    }
}

macro_rules! private_key_to_pem {
    ($f:path) => {
        /// Serializes the private key to PEM.
        pub fn private_key_to_pem(&self) -> Result<Vec<u8>, ::error::ErrorStack> {
            unsafe {
                let bio = try!(::bio::MemBio::new());
                try!(cvt($f(bio.as_ptr(),
                            self.as_ptr(),
                            ptr::null(),
                            ptr::null_mut(),
                            -1,
                            None,
                            ptr::null_mut())));
                Ok(bio.get_buf().to_owned())
            }
        }

        /// Serializes the private key to PEM, encrypting it with the specified symmetric cipher and
        /// passphrase.
        pub fn private_key_to_pem_passphrase(&self,
                                             cipher: ::symm::Cipher,
                                             passphrase: &[u8])
                                             -> Result<Vec<u8>, ::error::ErrorStack> {
            unsafe {
                let bio = try!(::bio::MemBio::new());
                assert!(passphrase.len() <= ::libc::c_int::max_value() as usize);
                try!(cvt($f(bio.as_ptr(),
                            self.as_ptr(),
                            cipher.as_ptr(),
                            passphrase.as_ptr() as *const _ as *mut _,
                            passphrase.len() as ::libc::c_int,
                            None,
                            ptr::null_mut())));
                Ok(bio.get_buf().to_owned())
            }
        }
    }
}

macro_rules! to_der_inner {
    (#[$m:meta] $n:ident, $f:path) => {
        #[$m]
        pub fn $n(&self) -> Result<Vec<u8>, ::error::ErrorStack> {
            unsafe {
                let len = try!(::cvt($f(::types::OpenSslTypeRef::as_ptr(self), ptr::null_mut())));
                let mut buf = vec![0; len as usize];
                try!(::cvt($f(::types::OpenSslTypeRef::as_ptr(self), &mut buf.as_mut_ptr())));
                Ok(buf)
            }
        }
    };
}

macro_rules! to_der {
    ($f:path) => {
        to_der_inner!(/// Serializes this value to DER.
            to_der, $f);
    }
}

macro_rules! private_key_to_der {
    ($f:path) => {
        to_der_inner!(/// Serializes the private key to DER.
            private_key_to_der, $f);
    }
}

macro_rules! public_key_to_der {
    ($f:path) => {
        to_der_inner!(/// Serializes the public key to DER.
            public_key_to_der, $f);
    }
}
