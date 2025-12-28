use std::path::Path;
use std::process::Command;
fn main() {
    compile_cpp_storage();
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let tailwind_input = Path::new(manifest_dir).join("tailwind.css");
    let tailwind_output = Path::new(manifest_dir).join("assets/tailwind.css");
    println!("cargo:rerun-if-changed={}", tailwind_input.display());
    println!(
        "cargo:rerun-if-changed={}",
        Path::new(manifest_dir).join("tailwind.config.js").display(),
    );
    let output = Command::new("npx")
        .arg("tailwindcss")
        .args([
            "-i",
            tailwind_input.to_str().unwrap(),
            "-o",
            tailwind_output.to_str().unwrap(),
        ])
        .current_dir(manifest_dir)
        .output();
    match output {
        Ok(output) => {
            if !output.status.success() {
                println!("cargo:error=Failed to generate Tailwind CSS");
                println!(
                    "cargo:error=STDERR: {}",
                    String::from_utf8_lossy(&output.stderr),
                );
                println!(
                    "cargo:error=STDOUT: {}",
                    String::from_utf8_lossy(&output.stdout),
                );
            } else {
                println!("cargo:warning=Tailwind CSS generated successfully");
            }
        }
        Err(e) => {
            println!("cargo:error=Failed to run tailwindcss: {}", e);
        }
    }
}
fn compile_cpp_storage() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let cpp_dir = Path::new(manifest_dir).join("cpp");
    if !cpp_dir.exists() {
        println!("cargo:warning=CPP directory not found, skipping C++ compilation");
        return;
    }
    let header = cpp_dir.join("bae_storage.h");
    let source = cpp_dir.join("bae_storage.cpp");
    let helpers_header = cpp_dir.join("bae_storage_helpers.h");
    let helpers_source = cpp_dir.join("bae_storage_helpers.cpp");
    let ffi_rs = Path::new(manifest_dir).join("src/torrent/ffi.rs");
    if !header.exists() || !source.exists() || !helpers_header.exists() || !helpers_source.exists()
    {
        println!("cargo:warning=Custom storage C++ files not found, skipping compilation",);
        return;
    }
    let wrappers_source = cpp_dir.join("bae_storage_cxx_wrappers.cpp");
    cxx_build::bridge(&ffi_rs)
        .file(&source)
        .file(&helpers_source)
        .file(&wrappers_source)
        .include(&cpp_dir)
        .include("/opt/homebrew/include")
        .include("/usr/local/include")
        .flag("-std=c++17")
        .compile("bae_storage");
    println!("cargo:rerun-if-changed={}", ffi_rs.display());
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", helpers_source.display());
    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", helpers_header.display());
    println!("cargo:rerun-if-changed={}", wrappers_source.display());
    let out_dir = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=bae_storage");
    println!("cargo:rustc-link-lib=torrent-rasterbar");
    println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
    println!("cargo:rustc-link-search=native=/usr/local/lib");
}
