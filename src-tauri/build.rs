fn main() {
    tauri_build::build();

    // Note: the integration-test `rustc-link-arg-tests` directive for
    // resource.lib (Windows Common Controls v6 manifest) was removed as part
    // of the architecture-fix rework, which deleted the `tests/` integration
    // suite. The bin/cdylib targets still receive the manifest via
    // tauri_build's own `cargo:rustc-link-arg`.
}
