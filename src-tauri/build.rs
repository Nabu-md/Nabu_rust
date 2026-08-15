fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Vision");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
    }

    // Ensure a placeholder file exists for tauri.bundle.resources validation
    // during `cargo check`.  The real native-messaging-host binary is built and
    // staged into this location by `scripts/build-dioxus.sh` (the Tauri
    // beforeBuildCommand) so that the bundler includes the actual executable
    // in Contents/Resources/.  On a clean checkout the placeholder lets the
    // build-script proceed; it is overwritten before the final bundle is
    // produced.
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let staged = manifest_dir.join("native-messaging-host");
    if !staged.exists() {
        let _ = std::fs::write(&staged, b"placeholder-not-yet-built");
    }

    tauri_build::build()
}
