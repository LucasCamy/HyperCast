fn main() {
    println!("cargo:rerun-if-changed=native/nvenc.cpp");
    println!("cargo:rerun-if-changed=native/vendor/nvEncodeAPI.h");
    #[cfg(feature = "nvenc")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        cc::Build::new()
            .cpp(true)
            .static_crt(true)
            .file("native/nvenc.cpp")
            .include("native/vendor")
            .flag_if_supported("/std:c++17")
            .flag_if_supported("/EHsc")
            .compile("hypercast_nvenc");
        println!("cargo:rustc-link-lib=d3d11");
    }
}
