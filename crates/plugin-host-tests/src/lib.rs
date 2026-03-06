//! Shared test helpers for the WASM plugin loader integration tests.
//!
//! Provides helper functions used by both `tests/integration.rs` and
//! `tests/client_e2e/` test modules.

use std::path::PathBuf;

/// Resolve workspace root from Cargo manifest dir.
///
/// This crate lives at `crates/plugin-host-tests/`,
/// so the workspace root is `../../`.
pub fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Directory containing the compiled WASM plugin binaries.
pub fn wasm_dir() -> PathBuf {
    workspace_root().join("target/wasm32-wasip1/debug")
}

/// Load a single plugin into a fresh `PluginRegistry` and instantiate it.
///
/// Returns the instantiated `PluginBackend` ready for testing.
///
/// # Errors
///
/// Returns an error if the WASM file doesn't exist or fails to load/instantiate.
pub async fn load_plugin(
    plugin_id: &str,
    wasm_filename: &str,
) -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    let path = wasm_dir().join(wasm_filename);
    if !path.exists() {
        return Err(format!(
            "WASM plugin not found: {}\nBuild with: cargo component build -p <crate> --target wasm32-wasip2",
            path.display()
        ).into());
    }

    let mut registry = poly_plugin_host::PluginRegistry::new()?;
    registry.load_from_file(plugin_id, &path)?;
    let backend = registry.instantiate(plugin_id).await?;
    Ok(backend)
}
