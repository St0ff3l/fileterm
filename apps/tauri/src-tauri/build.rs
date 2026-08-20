fn main() {
    // tauri_build embeds the Windows app manifest into the .rc resource that
    // only bin targets link, so cargo test executables ship without a manifest.
    // The loader then resolves comctl32.dll to v5 and the statically imported
    // TaskDialogIndirect (tauri/muda common-controls-v6 feature) kills the
    // process at startup with STATUS_ENTRYPOINT_NOT_FOUND. cargo:rustc-link-arg-tests
    // cannot fix this because it only covers integration test targets
    // (rust-lang/cargo#10937), not the lib unit-test binary.
    //
    // Workaround used by tauri's own examples (tauri-apps/tauri PR #4383):
    // drop the per-bin winres manifest and embed the same manifest through the
    // linker instead, which reaches every MSVC link target including tests.
    let is_windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");

    let windows_attributes = if is_windows_msvc {
        tauri_build::WindowsAttributes::new_without_app_manifest()
    } else {
        tauri_build::WindowsAttributes::new()
    };

    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows_attributes))
        .expect("failed to run tauri-build");

    if is_windows_msvc {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        // Embed the Windows application manifest file.
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
