#![allow(
    dead_code,
    unused_imports,
    clippy::unnecessary_map_or,
    clippy::enum_variant_names,
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]
//! # poly-core
//!
//! The shared library crate for the Poly messenger.
//!
//! Contains all UI components, application state, database abstractions,
//! internationalization, theme engine, cryptography, and backup sync logic.
//!
//! **This crate MUST support Dioxus subsecond hot-reload via `dx serve --hotpatch`.**
//!
//! ## Feature Flags
//!
//! - `demo` — Include the demo/mock client (default, for UI testing)
//! - `stoat` — Include the Stoat (Revolt) client
//! - `matrix` — Include the Matrix client
//! - `discord` — Include the Discord client
//! - `teams` — Include the Microsoft Teams client

pub mod bundled_plugins;
pub mod client_manager;
pub mod client_manager_timeout;
pub mod crypto;
pub mod plugin_admin;
// Legacy database module (native-only and only available with the SurrealDB backend).
#[cfg(all(not(target_arch = "wasm32"), feature = "storage-surreal"))]
pub mod db;
pub mod i18n;
/// WASM plugin host runtime (native-only — wasmtime cannot target wasm32).
///
/// Re-exports the `poly-plugin-host` dylib crate, which isolates the heavy
/// `wasmtime` runtime behind a dynamic linking boundary. Changes to poly-core
/// never trigger wasmtime recompilation.
///
/// Loads messenger backend plugins as Component Model WASM binaries and
/// bridges them to the `ClientBackend` trait. See [`plugin_host::PluginRegistry`].
/// DECISION(D21): WASM Plugin Backends.
/// DECISION(D22): Dynamic linking boundary for wasmtime isolation.
#[cfg(not(target_arch = "wasm32"))]
pub use poly_plugin_host as plugin_host;
pub mod state;
pub mod storage;
pub mod sync;
pub mod theme;
pub mod ui;
#[cfg(target_arch = "wasm32")]
mod wasm_crash_handler;

#[cfg(target_arch = "wasm32")]
pub use wasm_crash_handler::install_wasm_crash_handler;

// Re-export the client trait crate
pub use poly_client;

/// Translate a localization key (with optional named arguments).
///
/// Thin macro wrapper over [`i18n::t`] and [`i18n::t_args`].
/// All user-facing strings **must** go through this macro.
///
/// ## Examples
/// ```rust,ignore
/// // Simple key lookup — returns the translated string.
/// let s = t!("app-title");
///
/// // With named arguments (matches `{ $name }` in the .ftl file).
/// let s = t!("hello-user", name => "Alice");
/// let s = t!("server-count", count => "5", kind => "text");
/// ```
#[macro_export]
macro_rules! t {
    // Simple key, no arguments.
    ($key:expr) => {
        $crate::i18n::t($key)
    };
    // Key + one or more `name => value` pairs.
    ($key:expr, $($name:ident => $value:expr),+ $(,)?) => {
        $crate::i18n::t_args($key, &[$( (stringify!($name), $value) ),+])
    };
}

/// `nav!(Route::X { ... })` — push a route on the router history.
///
/// Preferred over bare `navigator().push(Route::...)`. The lint-gate
/// `nav_push_ban.rs` scan bans the bare form outside of this macro so every
/// navigation callsite is greppable by a single name (`nav!`). Expansion is
/// a zero-cost call through `dioxus::prelude::navigator().push(route)`.
#[macro_export]
macro_rules! nav {
    ($route:expr $(,)?) => {{
        ::dioxus::prelude::navigator().push($route)
    }};
}

/// Global storage handle — initialised exactly once at app startup.
///
/// Access via `poly_core::STORAGE.get()`. Returns `None` until initialised.
///
/// DECISION(DX-STORAGE-3): OnceLock global mirrors the eval-bridge pattern
/// already used in desktop-devtools. Components and event handlers can call
/// storage without prop-drilling or context gymnastics.
pub static STORAGE: std::sync::OnceLock<storage::Storage> = std::sync::OnceLock::new();

/// Install the shared browser/WASM crash handler.
///
/// On `wasm32`, this registers:
/// - a Rust panic hook,
/// - `window.onerror`, and
/// - `window.unhandledrejection`
///
/// Each failure path records crash metadata on `window.__polyCrashState` and
/// injects a fixed overlay into the DOM so renderer crashes are immediately
/// visible during manual testing and MCP automation.
#[cfg(not(target_arch = "wasm32"))]
pub fn install_wasm_crash_handler() {}

/// Initialize the Poly application.
///
/// This sets up all core subsystems: database, i18n, theme, and crypto.
/// Called once at application startup from each platform's `main.rs`.
///
/// Note: native plugin FTL registration (e.g. demo) is handled inside
/// [`i18n::init`] so that ALL entry points (including web WASM, which calls
/// `i18n::init()` directly) receive translations before any component renders.
pub async fn init() -> anyhow::Result<()> {
    tracing::info!("Initializing Poly core...");

    // Initialize i18n with system locale (also registers native plugin FTL).
    i18n::init();

    // Initialize theme engine with default theme
    theme::init();

    tracing::info!("Poly core initialized successfully");
    Ok(())
}
