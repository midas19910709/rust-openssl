use libc::{c_int, c_char, c_void};
use std::any::Any;
use std::cell::UnsafeCell;
use std::panic::{self, AssertUnwindSafe};
use std::slice;

/// Wraps a user-supplied callback and a slot for panics thrown inside the callback (while FFI
/// frames are on the stack).
///
/// When dropped, checks if the callback has panicked, and resumes unwinding if so.
pub struct CallbackState<F> {
    /// The user callback. Taken out of the `Option` when called.
    cb: Option<F>,
    /// If the callback panics, we place the panic object here, to be re-thrown once OpenSSL
    /// returns.
    panic: Option<Box<Any + Send + 'static>>,
}

impl<F> CallbackState<F> {
    pub fn new(callback: F) -> Self {
        CallbackState {
            cb: Some(callback),
            panic: None,
        }
    }
}

impl<F> Drop for CallbackState<F> {
    fn drop(&mut self) {
        if let Some(panic) = self.panic.take() {
            panic::resume_unwind(panic);
        }
    }
}

/// Password callback function, passed to private key loading functions.
///
/// `cb_state` is expected to be a pointer to a `CallbackState`.
pub unsafe extern "C" fn invoke_passwd_cb<F>(buf: *mut c_char,
                                             size: c_int,
                                             _rwflag: c_int,
                                             cb_state: *mut c_void)
                                             -> c_int
    where F: FnOnce(&mut [c_char]) -> usize
{
    let callback = &mut *(cb_state as *mut CallbackState<F>);

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        // build a `i8` slice to pass to the user callback
        let pass_slice = slice::from_raw_parts_mut(buf, size as usize);

        callback.cb.take().unwrap()(pass_slice)
    }));

    match result {
        Ok(len) => len as c_int,
        Err(err) => {
            callback.panic = Some(err);
            0
        }
    }
}

/// This is intended to be used as the inner type for `FooRef` types converted from raw C pointers.
/// It has an `UnsafeCell` internally to inform the compiler about aliasability and doesn't
/// implement `Copy`, so it can't be dereferenced.
pub struct Opaque(UnsafeCell<()>);
