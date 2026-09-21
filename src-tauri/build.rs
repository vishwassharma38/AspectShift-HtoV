fn main() {
    tauri_build::build();

    // tauri_build emits cargo:rustc-link-arg for the resource.lib (which contains the
    // Windows manifest with Common Controls v6), but that directive only covers bin/cdylib
    // targets. Integration tests also need the manifest to resolve comctl32 v6 imports
    // (e.g. TaskDialogIndirect). We re-emit the same resource.lib via the tests variant.
    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        let resource_lib = std::path::Path::new(&out_dir).join("resource.lib");
        if resource_lib.exists() {
            println!("cargo:rustc-link-arg-tests={}", resource_lib.display());
        }
    }
}
