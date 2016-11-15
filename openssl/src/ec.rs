use ffi;
use std::ptr;

use {cvt, cvt_n, cvt_p, init};
use bn::{BigNumRef, BigNumContextRef};
use error::ErrorStack;
use nid::Nid;
use types::OpenSslTypeRef;

pub const POINT_CONVERSION_COMPRESSED: PointConversionForm =
    PointConversionForm(ffi::point_conversion_form_t::POINT_CONVERSION_COMPRESSED);

pub const POINT_CONVERSION_UNCOMPRESSED: PointConversionForm =
    PointConversionForm(ffi::point_conversion_form_t::POINT_CONVERSION_UNCOMPRESSED);

pub const POINT_CONVERSION_HYBRID: PointConversionForm =
    PointConversionForm(ffi::point_conversion_form_t::POINT_CONVERSION_HYBRID);

#[derive(Copy, Clone)]
pub struct PointConversionForm(ffi::point_conversion_form_t);

type_!(EcGroup, EcGroupRef, ffi::EC_GROUP, ffi::EC_GROUP_free);

impl EcGroup {
    /// Returns the group of a standard named curve.
    pub fn from_curve_name(nid: Nid) -> Result<EcGroup, ErrorStack> {
        unsafe {
            init();
            cvt_p(ffi::EC_GROUP_new_by_curve_name(nid.as_raw())).map(EcGroup)
        }
    }

    /// Constructs a curve over a prime field from its components.
    pub fn from_components_gfp(p: &BigNumRef,
                               a: &BigNumRef,
                               b: &BigNumRef,
                               ctx: &mut BigNumContextRef)
                               -> Result<EcGroup, ErrorStack> {
        unsafe {
            cvt_p(ffi::EC_GROUP_new_curve_GFp(p.as_ptr(), a.as_ptr(), b.as_ptr(), ctx.as_ptr()))
                .map(EcGroup)
        }
    }

    /// Constructs a curve over a binary field from its components.
    pub fn from_components_gf2m(p: &BigNumRef,
                                a: &BigNumRef,
                                b: &BigNumRef,
                                ctx: &mut BigNumContextRef)
                                -> Result<EcGroup, ErrorStack> {
        unsafe {
            cvt_p(ffi::EC_GROUP_new_curve_GF2m(p.as_ptr(), a.as_ptr(), b.as_ptr(), ctx.as_ptr()))
                .map(EcGroup)
        }
    }
}

impl EcGroupRef {
    /// Places the components of a curve over a prime field in the provided `BigNum`s.
    pub fn components_gfp(&self,
                          p: &mut BigNumRef,
                          a: &mut BigNumRef,
                          b: &mut BigNumRef,
                          ctx: &mut BigNumContextRef)
                          -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::EC_GROUP_get_curve_GFp(self.as_ptr(),
                                            p.as_ptr(),
                                            a.as_ptr(),
                                            b.as_ptr(),
                                            ctx.as_ptr()))
                .map(|_| ())
        }
    }

    /// Places the components of a curve over a binary field in the provided `BigNum`s.
    pub fn components_gf2m(&self,
                           p: &mut BigNumRef,
                           a: &mut BigNumRef,
                           b: &mut BigNumRef,
                           ctx: &mut BigNumContextRef)
                           -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::EC_GROUP_get_curve_GF2m(self.as_ptr(),
                                             p.as_ptr(),
                                             a.as_ptr(),
                                             b.as_ptr(),
                                             ctx.as_ptr()))
                .map(|_| ())
        }
    }

    /// Returns the degree of the curve.
    pub fn degree(&self) -> u32 {
        unsafe { ffi::EC_GROUP_get_degree(self.as_ptr()) as u32 }
    }

    /// Places the order of the curve in the provided `BigNum`.
    pub fn order(&self,
                 order: &mut BigNumRef,
                 ctx: &mut BigNumContextRef)
                 -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::EC_GROUP_get_order(self.as_ptr(), order.as_ptr(), ctx.as_ptr())).map(|_| ())
        }
    }
}

type_!(EcPoint, EcPointRef, ffi::EC_POINT, ffi::EC_POINT_free);

impl EcPointRef {
    /// Computes `a + b`, storing the result in `self`.
    pub fn add(&mut self,
               group: &EcGroupRef,
               a: &EcPointRef,
               b: &EcPointRef,
               ctx: &mut BigNumContextRef)
               -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::EC_POINT_add(group.as_ptr(),
                                  self.as_ptr(),
                                  a.as_ptr(),
                                  b.as_ptr(),
                                  ctx.as_ptr()))
                .map(|_| ())
        }
    }

    /// Computes `q * m`, storing the result in `self`.
    pub fn mul(&mut self,
               group: &EcGroupRef,
               q: &EcPointRef,
               m: &BigNumRef,
               ctx: &BigNumContextRef)
               -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::EC_POINT_mul(group.as_ptr(),
                                  self.as_ptr(),
                                  ptr::null(),
                                  q.as_ptr(),
                                  m.as_ptr(),
                                  ctx.as_ptr()))
                .map(|_| ())
        }
    }

    /// Computes `generator * n`, storing the result ing `self`.
    pub fn mul_generator(&mut self,
                         group: &EcGroupRef,
                         n: &BigNumRef,
                         ctx: &BigNumContextRef)
                         -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::EC_POINT_mul(group.as_ptr(),
                                  self.as_ptr(),
                                  n.as_ptr(),
                                  ptr::null(),
                                  ptr::null(),
                                  ctx.as_ptr()))
                .map(|_| ())
        }
    }

    /// Computes `generator * n + q * m`, storing the result in `self`.
    pub fn mul_full(&mut self,
                    group: &EcGroupRef,
                    n: &BigNumRef,
                    q: &EcPointRef,
                    m: &BigNumRef,
                    ctx: &mut BigNumContextRef)
                    -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::EC_POINT_mul(group.as_ptr(),
                                  self.as_ptr(),
                                  n.as_ptr(),
                                  q.as_ptr(),
                                  m.as_ptr(),
                                  ctx.as_ptr()))
                .map(|_| ())
        }
    }

    /// Inverts `self`.
    pub fn invert(&mut self, group: &EcGroupRef, ctx: &BigNumContextRef) -> Result<(), ErrorStack> {
        unsafe {
            cvt(ffi::EC_POINT_invert(group.as_ptr(), self.as_ptr(), ctx.as_ptr())).map(|_| ())
        }
    }

    /// Serializes the point to a binary representation.
    pub fn to_bytes(&self,
                    group: &EcGroupRef,
                    form: PointConversionForm,
                    ctx: &mut BigNumContextRef)
                    -> Result<Vec<u8>, ErrorStack> {
        unsafe {
            let len = ffi::EC_POINT_point2oct(group.as_ptr(),
                                              self.as_ptr(),
                                              form.0,
                                              ptr::null_mut(),
                                              0,
                                              ctx.as_ptr());
            if len == 0 {
                return Err(ErrorStack::get());
            }
            let mut buf = vec![0; len];
            let len = ffi::EC_POINT_point2oct(group.as_ptr(),
                                              self.as_ptr(),
                                              form.0,
                                              buf.as_mut_ptr(),
                                              len,
                                              ctx.as_ptr());
            if len == 0 {
                Err(ErrorStack::get())
            } else {
                Ok(buf)
            }
        }
    }

    /// Determines if this point is equal to another.
    pub fn eq(&self,
              group: &EcGroupRef,
              other: &EcPointRef,
              ctx: &mut BigNumContextRef)
              -> Result<bool, ErrorStack> {
        unsafe {
            let res = try!(cvt_n(ffi::EC_POINT_cmp(group.as_ptr(),
                                                   self.as_ptr(),
                                                   other.as_ptr(),
                                                   ctx.as_ptr())));
            Ok(res == 0)
        }
    }
}

impl EcPoint {
    /// Creates a new point on the specified curve.
    pub fn new(group: &EcGroupRef) -> Result<EcPoint, ErrorStack> {
        unsafe { cvt_p(ffi::EC_POINT_new(group.as_ptr())).map(EcPoint) }
    }

    pub fn from_bytes(group: &EcGroupRef,
                      buf: &[u8],
                      ctx: &mut BigNumContextRef)
                      -> Result<EcPoint, ErrorStack> {
        let point = try!(EcPoint::new(group));
        unsafe {
            try!(cvt(ffi::EC_POINT_oct2point(group.as_ptr(),
                                             point.as_ptr(),
                                             buf.as_ptr(),
                                             buf.len(),
                                             ctx.as_ptr())));
        }
        Ok(point)
    }
}

type_!(EcKey, EcKeyRef, ffi::EC_KEY, ffi::EC_KEY_free);

impl EcKeyRef {
    private_key_to_pem!(ffi::PEM_write_bio_ECPrivateKey);
    private_key_to_der!(ffi::i2d_ECPrivateKey);

    pub fn group(&self) -> &EcGroupRef {
        unsafe {
            let ptr = ffi::EC_KEY_get0_group(self.as_ptr());
            assert!(!ptr.is_null());
            EcGroupRef::from_ptr(ptr as *mut _)
        }
    }

    pub fn public_key(&self) -> Option<&EcPointRef> {
        unsafe {
            let ptr = ffi::EC_KEY_get0_public_key(self.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(EcPointRef::from_ptr(ptr as *mut _))
            }
        }
    }

    pub fn private_key(&self) -> Option<&BigNumRef> {
        unsafe {
            let ptr = ffi::EC_KEY_get0_private_key(self.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(BigNumRef::from_ptr(ptr as *mut _))
            }
        }
    }

    /// Checks the key for validity.
    pub fn check_key(&self) -> Result<(), ErrorStack> {
        unsafe { cvt(ffi::EC_KEY_check_key(self.as_ptr())).map(|_| ()) }
    }
}

impl EcKey {
    /// Constructs an `EcKey` corresponding to a known curve.
    ///
    /// It will not have an associated public or private key. This kind of key is primarily useful
    /// to be provided to the `set_tmp_ecdh` methods on `Ssl` and `SslContextBuilder`.
    pub fn from_curve_name(nid: Nid) -> Result<EcKey, ErrorStack> {
        unsafe {
            init();
            cvt_p(ffi::EC_KEY_new_by_curve_name(nid.as_raw())).map(EcKey)
        }
    }

    /// Generates a new public/private key pair on the specified curve.
    pub fn generate(group: &EcGroupRef) -> Result<EcKey, ErrorStack> {
        unsafe {
            let key = EcKey(try!(cvt_p(ffi::EC_KEY_new())));
            try!(cvt(ffi::EC_KEY_set_group(key.as_ptr(), group.as_ptr())));
            try!(cvt(ffi::EC_KEY_generate_key(key.as_ptr())));
            Ok(key)
        }
    }

    #[deprecated(since = "0.9.2", note = "use from_curve_name")]
    pub fn new_by_curve_name(nid: Nid) -> Result<EcKey, ErrorStack> {
        EcKey::from_curve_name(nid)
    }

    private_key_from_pem!(EcKey, ffi::PEM_read_bio_ECPrivateKey);
    private_key_from_der!(EcKey, ffi::d2i_ECPrivateKey);
}

#[cfg(test)]
mod test {
    use bn::{BigNum, BigNumContext};
    use nid;
    use super::*;

    #[test]
    fn key_new_by_curve_name() {
        EcKey::from_curve_name(nid::X9_62_PRIME256V1).unwrap();
    }

    #[test]
    fn round_trip_prime256v1() {
        let group = EcGroup::from_curve_name(nid::X9_62_PRIME256V1).unwrap();
        let mut p = BigNum::new().unwrap();
        let mut a = BigNum::new().unwrap();
        let mut b = BigNum::new().unwrap();
        let mut ctx = BigNumContext::new().unwrap();
        group.components_gfp(&mut p, &mut a, &mut b, &mut ctx).unwrap();
        EcGroup::from_components_gfp(&p, &a, &b, &mut ctx).unwrap();
    }

    #[test]
    fn generate() {
        let group = EcGroup::from_curve_name(nid::X9_62_PRIME256V1).unwrap();
        let key = EcKey::generate(&group).unwrap();
        key.public_key().unwrap();
        key.private_key().unwrap();
    }

    #[test]
    fn point_new() {
        let group = EcGroup::from_curve_name(nid::X9_62_PRIME256V1).unwrap();
        EcPoint::new(&group).unwrap();
    }

    #[test]
    fn point_bytes() {
        let group = EcGroup::from_curve_name(nid::X9_62_PRIME256V1).unwrap();
        let key = EcKey::generate(&group).unwrap();
        let point = key.public_key().unwrap();
        let mut ctx = BigNumContext::new().unwrap();
        let bytes = point.to_bytes(&group, POINT_CONVERSION_COMPRESSED, &mut ctx).unwrap();
        let point2 = EcPoint::from_bytes(&group, &bytes, &mut ctx).unwrap();
        assert!(point.eq(&group, &point2, &mut ctx).unwrap());
    }

    #[test]
    fn mul_generator() {
        let group = EcGroup::from_curve_name(nid::X9_62_PRIME256V1).unwrap();
        let key = EcKey::generate(&group).unwrap();
        let mut ctx = BigNumContext::new().unwrap();
        let mut public_key = EcPoint::new(&group).unwrap();
        public_key.mul_generator(&group, key.private_key().unwrap(), &mut ctx).unwrap();
        assert!(public_key.eq(&group, key.public_key().unwrap(), &mut ctx).unwrap());
    }
}
