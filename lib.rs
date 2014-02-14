#[feature(struct_variant, macro_rules)];
#[crate_id="github.com/sfackler/rust-openssl#openssl:0.0"];
#[crate_type="rlib"];
#[crate_type="dylib"];
#[doc(html_root_url="http://www.rust-ci.org/sfackler/rust-openssl/doc")];

extern mod extra;
#[cfg(test)]
extern mod serialize;
extern mod sync;

pub mod ssl;
pub mod crypto;
