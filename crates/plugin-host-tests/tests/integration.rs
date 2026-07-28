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

/// `(plugin id, wasm file name, optional exact (backend_type, backend_name))`.
///
/// `None` in the third slot means the manifest name is not asserted exactly —
/// the plugin is still loaded, instantiated, and required to report a
/// non-empty name.
type PluginExpectation = (&'static str, &'static str, Option<(BackendType, &'static str)>);

/// Load all 8 WASM plugin files and verify they can be instantiated
/// (load + unload) and report a backend type and name. The 6 plugins with
/// stable manifest names also assert their exact `backend_type`/`backend_name`.
///
/// ## Prerequisites
///
/// Build the plugin WASM binaries first (default features OFF — see
/// `docs/plans/plan-wasm-plugin-wasip2-build.md`):
/// ```sh
/// cargo component build -p poly-demo -p poly-stoat -p poly-matrix \
///     -p poly-discord -p poly-teams -p poly-lemmy -p poly-forgejo \
///     -p poly-server-client --target wasm32-wasip2 --no-default-features
/// ```
#[allow(clippy::unwrap_used)]
#[tokio::test]
async fn load_all_wasm_plugins() {
    let wasm_dir = wasm_dir();

    // (id, file, expected_type, expected_name). expected_name is None for
    // plugins whose manifest name is not asserted exactly — we still prove
    // they load, instantiate, and report a non-empty name.
    let plugins: [PluginExpectation; 8] = [
        ("demo", "poly_demo.wasm", Some((BackendType::from("demo"), "Demo"))),
        ("stoat", "poly_stoat.wasm", Some((BackendType::from("stoat"), "Stoat"))),
        ("matrix", "poly_matrix.wasm", Some((BackendType::from("matrix"), "Matrix"))),
        ("discord", "poly_discord.wasm", Some((BackendType::from("discord"), "Discord"))),
        ("teams", "poly_teams.wasm", Some((BackendType::from("teams"), "Teams"))),
        ("server", "poly_server_client.wasm", Some((BackendType::from("poly"), "Poly Server"))),
        ("lemmy", "poly_lemmy.wasm", None),
        ("forgejo", "poly_forgejo.wasm", None),
    ];

    // Skip the test entirely if any plugin binary is missing. Binaries are
    // produced by `cargo component build --target wasm32-wasip2
    // --no-default-features` and are not checked in.
    for (_, file, _) in &plugins {
        let path = wasm_dir.join(file);
        if !path.exists() {
            // stderr, not `tracing` — this test binary installs no subscriber,
            // so a `tracing::warn!` would vanish and the early return would be
            // an invisible vacuous pass. `cargo test -- --nocapture` shows this.
            #[allow(clippy::print_stderr)]
            {
                eprintln!("SKIP load_all_wasm_plugins: {file} not found (run `cargo component build --target wasm32-wasip2 --no-default-features` to produce it)");
            }
            return;
        }
    }

    let mut registry = PluginRegistry::new().unwrap();

    // Load all plugins from disk
    for (id, file, _) in &plugins {
        let path = wasm_dir.join(file);
        registry.load_from_file(id, &path).unwrap();
    }

    assert_eq!(
        registry.loaded_plugins().len(),
        8,
        "Expected 8 plugins loaded"
    );

    // Instantiate each (proves load + unload through the real wasmtime host)
    // and verify backend_type + backend_name where the name is stable.
    for (id, _, expected) in &plugins {
        let backend = registry.instantiate(id).await.unwrap();

        assert!(
            !backend.backend_name().is_empty(),
            "Plugin '{id}' returned an empty backend_name"
        );

        if let Some((expected_type, expected_name)) = expected {
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
}
