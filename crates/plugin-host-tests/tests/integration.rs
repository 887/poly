//! Integration tests for the WASM plugin loader.
//!
//! These tests load the compiled `.wasm` plugin binaries,
//! instantiate them through `PluginRegistry`, and verify
//! they correctly report backend types and names.


use poly_client::{
    IsBackend, BackendType,
};
use poly_plugin_host::PluginRegistry;
use poly_plugin_loader_tests::wasm_dir;

/// Load all 6 WASM plugin files and verify they can be instantiated
/// and return correct backend types and names.
///
/// ## Prerequisites
///
/// Build the plugin WASM binaries first:
/// ```sh
/// cargo component build -p poly-demo -p poly-stoat -p poly-matrix \
///     -p poly-discord -p poly-teams -p poly-server-client \
///     --target wasm32-wasip2
/// ```
#[allow(clippy::unwrap_used)]
#[tokio::test]
async fn load_all_wasm_plugins() {
    let wasm_dir = wasm_dir();

    let plugins = [
        ("demo", "poly_demo.wasm", BackendType::from("demo"), "Demo"),
        ("stoat", "poly_stoat.wasm", BackendType::from("stoat"), "Stoat"),
        ("matrix", "poly_matrix.wasm", BackendType::from("matrix"), "Matrix"),
        (
            "discord",
            "poly_discord.wasm",
            BackendType::from("discord"),
            "Discord",
        ),
        ("teams", "poly_teams.wasm", BackendType::from("teams"), "Teams"),
        (
            "server",
            "poly_server_client.wasm",
            BackendType::from("poly"),
            "Poly Server",
        ),
    ];

    // Skip the test entirely if any plugin binary is missing.
    // Binaries are produced by `cargo component build --target wasm32-wasip1` and
    // are not checked in; they cannot all be built from within CI because several
    // plugins depend on native-only crates (openssl-sys, full tokio).
    for (_, file, _, _) in &plugins {
        let path = wasm_dir.join(file);
        if !path.exists() {
            eprintln!("SKIP load_all_wasm_plugins: {file} not found (run `cargo component build --target wasm32-wasip1` to produce it)");
            return;
        }
    }

    let mut registry = PluginRegistry::new().unwrap();

    // Load all plugins from disk
    for (id, file, _, _) in &plugins {
        let path = wasm_dir.join(file);
        registry.load_from_file(id, &path).unwrap();
    }

    assert_eq!(
        registry.loaded_plugins().len(),
        6,
        "Expected 6 plugins loaded"
    );

    // Instantiate each and verify backend_type + backend_name
    for (id, _, expected_type, expected_name) in &plugins {
        let backend = registry.instantiate(id).await.unwrap();

        assert_eq!(
            backend.backend_type(),
            *expected_type,
            "Plugin '{id}' returned wrong backend_type"
        );
        assert_eq!(
            backend.backend_name(),
            *expected_name,
            "Plugin '{id}' returned wrong backend_name"
        );
    }
}
