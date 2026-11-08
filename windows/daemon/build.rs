use std::path::PathBuf;

fn main() {
    // The AAC-ELD decoder (FFmpeg) is only used on Windows.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ff = manifest.join("vendor/ffmpeg");

    // Compile the tiny C shim against the vendored FFmpeg headers.
    cc::Build::new()
        .file("src/eld_shim.c")
        .include(ff.join("include"))
        .compile("eld_shim");

    // Link the FFmpeg import libs (avcodec pulls avutil + swresample).
    println!(
        "cargo:rustc-link-search=native={}",
        ff.join("lib").display()
    );
    println!("cargo:rustc-link-lib=avcodec");
    println!("cargo:rustc-link-lib=avutil");
    println!("cargo:rustc-link-lib=swresample");
    println!("cargo:rerun-if-changed=src/eld_shim.c");

    // Put the FFmpeg DLLs next to podatuid.exe so the build output runs as is.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile_dir = out.ancestors().nth(3).unwrap();
    for entry in std::fs::read_dir(ff.join("bin")).unwrap() {
        let dll = entry.unwrap().path();
        std::fs::copy(&dll, profile_dir.join(dll.file_name().unwrap())).unwrap();
    }
}
