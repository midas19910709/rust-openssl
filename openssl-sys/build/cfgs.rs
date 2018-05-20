pub fn get(openssl_version: Option<u64>, libressl_version: Option<u64>) -> Vec<&'static str> {
    let mut cfgs = vec![];

    if let Some(libressl_version) = libressl_version {
        cfgs.push("libressl");

        if libressl_version >= 0x2_05_01_00_0 {
            cfgs.push("libressl251");
        }

        if libressl_version >= 0x2_06_01_00_0 {
            cfgs.push("libressl261");
        }

        if libressl_version >= 0x2_07_00_00_0 {
            cfgs.push("libressl270");
        }
    } else {
        let openssl_version = openssl_version.unwrap();

        if openssl_version >= 0x1_00_02_08_0 {
            cfgs.push("ossl102h");
        }

        if openssl_version >= 0x1_01_00_07_0 {
            cfgs.push("ossl110g");
        }

        if openssl_version >= 0x1_01_01_00_0 {
            cfgs.push("ossl111");
            cfgs.push("ossl110");
        } else if openssl_version >= 0x1_01_00_06_0 {
            cfgs.push("ossl110");
            cfgs.push("ossl110f");
        } else if openssl_version >= 0x1_01_00_00_0 {
            cfgs.push("ossl110");
        } else if openssl_version >= 0x1_00_02_00_0 {
            cfgs.push("ossl102");
        } else if openssl_version >= 0x1_00_01_00_0 {
            cfgs.push("ossl101");
        }
    }

    cfgs
}
