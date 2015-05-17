extern crate pkg_config;
extern crate gcc;

use std::env;
use std::path::PathBuf;

fn main() {
    let target = env::var("TARGET").unwrap();

    // libressl_pnacl_sys links the libs needed.
    if target.ends_with("nacl") { return; }

    let lib_dir = env::var("OPENSSL_LIB_DIR").ok();
    let include_dir = env::var("OPENSSL_INCLUDE_DIR").ok();

    if lib_dir.is_none() && include_dir.is_none() {
        if let Ok(info) = pkg_config::find_library("openssl") {
            build_old_openssl_shim(&info.include_paths);
            return;
        }
    }

    let libs_env = env::var("OPENSSL_LIBS").ok();
    let libs = match libs_env {
        Some(ref v) => v.split(":").collect(),
        None => if target.contains("windows") {
            vec!("eay32", "ssl32")
        } else {
            vec!("crypto", "ssl")
        }
    };

    let mode = if env::var_os("OPENSSL_STATIC").is_some() {
    	"static"
    } else {
    	"dylib"
    };

    if let Some(lib_dir) = lib_dir {
    	println!("cargo:rustc-flags=-L native={}", lib_dir);
    }

    let libs_arg = libs.iter().fold(String::new(), |args, lib| args + &format!(" -l {0}={1}", mode, lib));
    println!("cargo:rustc-flags={0}", libs_arg);

    let mut include_dirs = vec![];

    if let Some(include_dir) = include_dir {
        include_dirs.push(PathBuf::from(&include_dir));
    }

    build_old_openssl_shim(&include_dirs);
}

fn build_old_openssl_shim(include_paths: &[PathBuf]) {
    let mut config = gcc::Config::new();

    for path in include_paths {
        config.include(path);
    }

    config.file("src/old_openssl_shim.c")
        .compile("libold_openssl_shim.a");
}
