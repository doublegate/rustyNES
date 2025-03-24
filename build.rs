use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let profile = env::var("PROFILE").unwrap();
    let target = env::var("TARGET").unwrap();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Copy SDL2.dll to the output directory
    let sdl2_dll = if target.contains("x86_64") {
        "C:/vcpkg/installed/x64-windows/bin/SDL2.dll"
    } else {
        "C:/vcpkg/installed/x86-windows/bin/SDL2.dll"
    };

    let out_path = Path::new(&manifest_dir)
        .join("target")
        .join(&profile)
        .join("SDL2.dll");

    fs::copy(sdl2_dll, out_path).unwrap();
} 