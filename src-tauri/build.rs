fn main() {
    tauri_build::build();

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let manifest_path = std::path::Path::new(&manifest_dir).join("windows-app.manifest");
            if manifest_path.exists() {
                println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
                println!(
                    "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
                    manifest_path.display()
                );
            }
        }
    }
}

