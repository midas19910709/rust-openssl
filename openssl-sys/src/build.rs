extern crate "pkg-config" as pkg_config;

use std::os;

fn main() {
    if pkg_config::find_library("openssl").is_err() {
        let mut flags = " -l crypto -l ssl".to_string();

        let target = os::getenv("TARGET").unwrap();

        let win_pos = target.find_str("windows")
                            .or(target.find_str("win32"))
                            .or(target.find_str("win64"));

        // It's fun, but it looks like win32 and win64 both
        // have all the libs with 32 sufix
        if win_pos.is_some() {
           flags.push_str(" -l gdi32 -l wsock32");
        }

        // Android doesn't have libcrypto/libssl,
        // the toplevel Rust program should compile it themselves
        if target.find_str("android").is_some() {
            os::getenv("OPENSSL_PATH").expect("Android does not provide openssl libraries, please \
                                               build them yourselves (instructions in the README) \
                                               and provide their location through $OPENSSL_PATH.");
            flags.push_str(" -L ${OPENSSL_PATH}");
        }

        println!("cargo:rustc-flags={}", flags);
    }
}
