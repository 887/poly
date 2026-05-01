//! Shared test helpers for the WASM plugin loader integration tests.
//!
//! Provides helper functions used by both `tests/integration.rs` and
//! `tests/client_e2e/` test modules.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use poly_plugin_host::host_impl::PluginHostState;

/// Resolve workspace root from Cargo manifest dir.
///
/// This crate lives at `crates/plugin-host-tests/`,
/// so the workspace root is `../../`.
#[must_use]
pub fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Directory containing the compiled WASM plugin binaries.
#[must_use]
pub fn wasm_dir() -> PathBuf {
    workspace_root().join("target/wasm32-wasip1/debug")
}

/// Map a plugin id → the crate name `cargo component build` expects.
fn plugin_crate_name(plugin_id: &str) -> &'static str {
    match plugin_id {
        "stoat" => "poly-stoat",
        "matrix" => "poly-matrix",
        "discord" => "poly-discord",
        "teams" => "poly-teams",
        "lemmy" => "poly-lemmy",
        "server" | "poly-server" | "server-client" => "poly-server-client",
        // "demo" and any unknown plugin id both map to poly-demo
        _ => "poly-demo",
    }
}

/// Build the WASM artifact for a single plugin crate. Idempotent per process:
/// records a `Mutex`-guarded set of already-built crate names to avoid redundant
/// cargo invocations when multiple tests load the same plugin in the same run.
fn ensure_wasm_built(crate_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashSet;
    use std::sync::OnceLock;

    static BUILT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let set = BUILT.get_or_init(|| Mutex::new(HashSet::new()));
    {
        let guard = set.lock().map_err(|e| e.to_string())?;
        if guard.contains(crate_name) {
            return Ok(());
        }
    }

    let status = std::process::Command::new("cargo")
        .args([
            "component",
            "build",
            "-p",
            crate_name,
            "--target",
            "wasm32-wasip2",
        ])
        .current_dir(workspace_root())
        .status()?;

    if !status.success() {
        return Err(format!("cargo component build -p {crate_name} failed").into());
    }

    let mut guard = set.lock().map_err(|e| e.to_string())?;
    guard.insert(crate_name.to_string());
    Ok(())
}

fn resolve_plugin_path(
    plugin_id: &str,
    wasm_filename: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = wasm_dir().join(wasm_filename);
    if !path.exists() {
        ensure_wasm_built(plugin_crate_name(plugin_id))?;
    }
    if !Path::new(&path).exists() {
        return Err(format!(
            "WASM plugin still missing after build: {}",
            path.display()
        )
        .into());
    }
    Ok(path)
}

/// Load a single plugin into a fresh `PluginRegistry` and instantiate it.
///
/// Runs `cargo component build -p <crate> --target wasm32-wasip2` on first
/// use if the artifact is missing, so `cargo test` works regardless of whether
/// the WASM binary was pre-built.
///
/// # Errors
///
/// Returns an error if the WASM file cannot be built or fails to load/instantiate.
pub async fn load_plugin(
    plugin_id: &str,
    wasm_filename: &str,
) -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    let path = resolve_plugin_path(plugin_id, wasm_filename)?;
    let mut registry = poly_plugin_host::PluginRegistry::new()?;
    registry.load_from_file(plugin_id, &path)?;
    let backend = registry.instantiate(plugin_id).await?;
    Ok(backend)
}

/// Load a plugin into a fresh registry with a caller-provided host state.
///
/// Used by plugin tests that need deterministic mocked host I/O while still
/// executing the real WASM guest path.
pub async fn load_plugin_with_host_state(
    plugin_id: &str,
    wasm_filename: &str,
    host_state: PluginHostState,
) -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    let path = resolve_plugin_path(plugin_id, wasm_filename)?;
    let mut registry = poly_plugin_host::PluginRegistry::new()?;
    registry.load_from_file(plugin_id, &path)?;
    let backend = registry
        .instantiate_with_host_state(plugin_id, host_state)
        .await?;
    Ok(backend)
}
