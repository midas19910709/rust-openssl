
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

        /// Deserializes a PEM-formatted private key, using a callback to retrieve a password if the
        /// key is encrypted.
        ///
        /// The callback should copy the password into the provided buffer and return the number of
        /// bytes written.
        pub fn private_key_from_pem_callback<F>(pem: &[u8],
                                                callback: F)
                                                -> Result<$t, ::error::ErrorStack>
            where F: FnOnce(&mut [u8]) -> usize
        {
            unsafe {
                ffi::init();
                let mut cb = ::util::CallbackState::new(callback);
                let bio = try!(::bio::MemBioSlice::new(pem));
                cvt_p($f(bio.as_ptr(),
                         ptr::null_mut(),
                         Some(::util::invoke_passwd_cb::<F>),
                         &mut cb as *mut _ as *mut ::libc::c_void))
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
                let bio = try!(MemBio::new());
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
    }
}
