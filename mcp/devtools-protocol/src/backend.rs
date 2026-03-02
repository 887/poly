//! Backend trait for devtools implementations.
//!
//! Each backend (desktop HTTP bridge, Chrome CDP, etc.) implements this trait.
//! The MCP main loop dispatches tool calls to the active backend.

use async_trait::async_trait;

/// Result of a screenshot capture.
pub struct ScreenshotResult {
    /// Raw PNG image data.
    pub png_bytes: Vec<u8>,
}

/// A single console log entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsoleEntry {
    pub level: String,
    pub text: String,
    #[serde(default)]
    pub timestamp: Option<f64>,
}

/// Trait implemented by each devtools backend.
///
/// Methods return `anyhow::Result` — the MCP layer converts errors to
/// `isError: true` tool results automatically.
#[async_trait]
pub trait DevtoolsBackend: Send + Sync {
    /// Human-readable backend name (e.g. "desktop-http", "web-cdp").
    fn name(&self) -> &str;

    // ── Lifecycle ───────────────────────────────────────────────────────

    /// Build (if needed) and launch the application under test.
    /// `workspace` is the path to the workspace root.
    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String>;

    /// Kill the running application.
    async fn kill_app(&self) -> anyhow::Result<String>;

    /// Verify connectivity to the running application.
    /// Returns a human-readable status message on success.
    async fn connect(&self) -> anyhow::Result<String>;

    // ── Inspection ──────────────────────────────────────────────────────

    /// Capture a screenshot of the current UI state.
    async fn screenshot(&self) -> anyhow::Result<ScreenshotResult>;

    /// Evaluate JavaScript in the app's webview and return the result string.
    async fn js_eval(&self, expression: &str) -> anyhow::Result<String>;

    /// Return the full DOM HTML (`document.documentElement.outerHTML`).
    async fn get_dom(&self) -> anyhow::Result<String>;

    /// Return buffered console log messages.
    async fn get_console(&self) -> anyhow::Result<String>;

    // ── Interaction ─────────────────────────────────────────────────────

    /// Simulate a mouse click at (x, y) CSS pixel coordinates.
    async fn click(&self, x: i64, y: i64) -> anyhow::Result<String>;

    /// Type text into the currently focused element.
    async fn type_text(&self, text: &str) -> anyhow::Result<String>;

    // ── Navigation / Reset ──────────────────────────────────────────────

    /// Trigger a Dioxus full rebuild (recompilation + app restart).
    ///
    /// For `dx serve --hotpatch` setups, RSX-only changes are applied
    /// automatically via hot-reload. Use this for structural code changes
    /// that require a full recompilation.
    ///
    /// `workspace` is the path to the workspace root.
    async fn rebuild_app(&self, workspace: &str) -> anyhow::Result<String> {
        let _ = workspace;
        anyhow::bail!("rebuild_app not supported by this backend")
    }

    /// Hard-kill the `dx serve` process and the running app with SIGKILL.
    ///
    /// Use when [`kill_app`] doesn't work (e.g. the process is stuck).
    /// After this call, use [`launch_app`] to restart.
    async fn hard_kill(&self) -> anyhow::Result<String> {
        anyhow::bail!("hard_kill not supported by this backend")
    }

    /// Reload the active page/webview (F5 equivalent).
    ///
    /// For desktop this reloads the webview content; for web it reloads the
    /// browser tab. Useful after hot-reload patches a component.
    async fn browser_reload(&self) -> anyhow::Result<String> {
        anyhow::bail!("browser_reload not supported by this backend")
    }

    /// Delete the local database and restart at the setup wizard.
    /// Default implementation returns an error (backends override as appropriate).
    async fn reset_app(&self) -> anyhow::Result<String> {
        anyhow::bail!("reset_app not supported by this backend")
    }

    /// Navigate to a specific route/view within the running app.
    /// Default implementation uses `window.location.hash` via js_eval.
    async fn navigate(&self, route: &str) -> anyhow::Result<String> {
        self.js_eval(&format!(
            r#"(function(){{ window.location.hash = '{}'; return 'navigated to {}'; }})()"#,
            route, route
        ))
        .await
    }

    // ── Extension point ─────────────────────────────────────────────────

    /// Handle a backend-specific tool call not covered by the standard set.
    /// Returns `None` if the tool name is unrecognised (MCP layer reports error).
    async fn handle_extension_tool(
        &self,
        _name: &str,
        _args: &serde_json::Value,
    ) -> Option<anyhow::Result<String>> {
        None
    }

    /// Return extra tool definitions specific to this backend.
    /// These are appended to the standard tool list.
    fn extension_tools(&self) -> Vec<serde_json::Value> {
        vec![]
    }
}
