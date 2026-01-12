use std::path::Path;
use std::process::Command;

fn main() {
    copy_shared_assets();
    generate_tailwind();
}

fn copy_shared_assets() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let shared_main_css = Path::new(manifest_dir).join("../bae-ui/assets/main.css");
    let local_main_css = Path::new(manifest_dir).join("assets/main.css");

    println!("cargo:rerun-if-changed={}", shared_main_css.display());

    if shared_main_css.exists() {
        std::fs::copy(&shared_main_css, &local_main_css)
            .expect("Failed to copy main.css from bae-ui");
    }
}

fn generate_tailwind() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let tailwind_input = Path::new(manifest_dir).join("tailwind.css");
    let tailwind_output = Path::new(manifest_dir).join("assets/tailwind.css");

    println!("cargo:rerun-if-changed={}", tailwind_input.display());
    println!(
        "cargo:rerun-if-changed={}",
        Path::new(manifest_dir)
            .join("../bae-ui/src")
            .canonicalize()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "../bae-ui/src".to_string())
    );

    // Use node_modules/.bin/tailwindcss directly - more reliable than npx in CI
    let tailwind_bin = Path::new(manifest_dir).join("node_modules/.bin/tailwindcss");
    let output = Command::new(&tailwind_bin)
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
                eprintln!("Failed to generate Tailwind CSS");
                eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
                eprintln!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
                panic!("Tailwind CSS generation failed");
            } else {
                println!("cargo:warning=Tailwind CSS generated successfully");
            }
        }
        Err(e) => {
            panic!("Failed to run tailwindcss: {}", e);
        }
    }
}
