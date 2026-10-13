fn main() {
    println!("cargo:rerun-if-changed=native/nvenc.cpp");
    println!("cargo:rerun-if-changed=native/capture_color.h");
    println!("cargo:rerun-if-changed=native/system_audio.cpp");
    println!("cargo:rerun-if-changed=native/window_capture.cpp");
    println!("cargo:rerun-if-changed=native/presenter.cpp");
    println!("cargo:rerun-if-changed=native/vendor/nvEncodeAPI.h");
    println!("cargo:rerun-if-changed=assets/hypercast.ico");
    println!("cargo:rerun-if-changed=assets/hypercast.rc");
    #[cfg(feature = "desktop")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let output =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("hypercast.res");
        let status = std::process::Command::new("rc.exe")
            .arg("/nologo")
            .arg(format!("/fo{}", output.display()))
            .arg("assets/hypercast.rc")
            .status()
            .expect("Windows SDK rc.exe");
        assert!(status.success(), "Windows resource compilation failed");
        println!("cargo:rustc-link-arg-bin=hypercast={}", output.display());
    }
    #[cfg(feature = "nvenc")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        cc::Build::new()
            .cpp(true)
            .static_crt(true)
            .file("native/nvenc.cpp")
            .file("native/presenter.cpp")
            .file("native/system_audio.cpp")
            .file("native/window_capture.cpp")
            .include("native/vendor")
            .flag_if_supported("/std:c++20")
            .flag_if_supported("/EHsc")
            .compile("hypercast_nvenc");
        println!("cargo:rustc-link-lib=d3d11");
        println!("cargo:rustc-link-lib=dxgi");
        println!("cargo:rustc-link-lib=d3dcompiler");
        println!("cargo:rustc-link-lib=ole32");
        println!("cargo:rustc-link-lib=mmdevapi");
        println!("cargo:rustc-link-lib=windowsapp");
        println!("cargo:rustc-link-lib=dwmapi");
    }
}
