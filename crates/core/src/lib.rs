#![allow(
    dead_code,
    unused_imports,
    clippy::unnecessary_map_or,
    clippy::enum_variant_names,
    // ── Curated, defensible crate-wide allows for this Dioxus/WASM UI crate ──
    // Each is a false-positive class or a genuinely-N/A lint for this crate's
    // domain — NOT a blanket "make cranky quiet" dump. Everything else is
    // fixed per-site or scoped+marked at its definition.
    //
    // rsx! closures clone captured values; clippy can't see through the macro's
    // closure boundaries and flags each capture-clone as redundant.
    clippy::redundant_clone,
    clippy::implicit_clone,
    // WASM is single-threaded; async fns holding Signal guards are deliberately !Send.
    clippy::future_not_send,
    // Signal/lock guards intentionally span the whole operation for correctness;
    // tightening their scope would reintroduce the read-guard-across-write hang class.
    clippy::significant_drop_tightening,
    // Dioxus `use_*` hooks must follow other let-bindings per the hooks API contract.
    clippy::items_after_statements,
    // Component state structs legitimately carry several independent bool flags.
    clippy::struct_excessive_bools,
    // `todo!()` stubs are intentional phase placeholders awaiting feature work;
    // clippy::todo only flags their existence, which is by design here.
    clippy::todo,
    // asset!()/include-style macros emit composite consts that trip this nursery lint.
    clippy::volatile_composites,
    // nursery: const-ifying ~80 UI helper/component fns is pure churn with no
    // runtime benefit and is brittle across the WASM/native cfg split.
    clippy::missing_const_for_fn,
    // nursery: `pub(crate)` inside private modules is intentional intent-signaling.
    clippy::redundant_pub_crate,
    // nursery: large event-dispatch / state-machine fns; splitting risks Dioxus reactive bugs.
    clippy::cognitive_complexity,
    // pedantic: `&Option<T>` is the idiomatic Dioxus prop-passing shape.
    clippy::ref_option,
    // restriction: `mod.rs` is this crate's deliberate module-layout convention.
    clippy::mod_module_files,
    // async kept to satisfy trait signatures even where the body is currently sync.
    clippy::unused_async,
    // pedantic: field-name repetition (e.g. `server_id` in `Server`) aids discoverability.
    clippy::struct_field_names,
    // pedantic: short-scoped local names are intentional and not actually confusable.
    clippy::similar_names,
    // nursery readability-subjective lints whose suggested rewrite is frequently
    // LESS readable in this crate's match-heavy UI logic. clippy itself ships
    // these as nursery for exactly this reason; suppressed crate-wide by choice.
    clippy::option_if_let_else,
    clippy::single_match_else,
    // nursery: `match { Ok=>.., Err=>return }` vs `let Ok(x) = .. else` is a taste
    // call; converting the remaining sites trips the lint-gate render-read text
    // heuristic via line shifts in files with grandfathered reads (dm_view). Allow.
    clippy::manual_let_else,
    // pedantic taste: `if !cond { A } else { B }` is sometimes the clearer order.
    clippy::if_not_else,
    // restriction: build-baked FTL locale consts (i18n/baked_locales_*.rs) are
    // generated single string literals; hash count is mechanical, not a code smell.
    clippy::needless_raw_string_hashes,
    // pedantic: short single-char locals (date-component y/mo/d/h/m/s parsing) are
    // clearer than verbose names in tight numeric-parse scopes.
    clippy::many_single_char_names,
    // pedantic: large value types are moved into rsx!/spawn closures by design;
    // passing by-ref would fight the move-into-closure capture pattern.
    clippy::large_types_passed_by_value,
    // pedantic: `&T` for small Copy types is the idiomatic Dioxus prop shape.
    clippy::trivially_copy_pass_by_ref,
    // pedantic: `&mut T` params kept for API uniformity across sibling handlers.
    clippy::needless_pass_by_ref_mut,
    // restriction: identical match arms kept distinct on purpose — each documents a
    // route/variant expected to diverge as features land.
    clippy::match_same_arms,
    // pedantic: a 3-way if/else on an ordering reads clearer than match a.cmp(b) here.
    clippy::comparison_chain,
    // restriction: wildcard enum arm is intentional — handles the cases it cares about,
    // drops the rest (incl. future-added variants) by design.
    clippy::wildcard_enum_match_arm,
    // pedantic: the flagged Result/closure type aliases are clear in context.
    clippy::type_complexity,
    // restriction: fire-and-forget voice/video/theme-string calls intentionally discard
    // the Result; the UI does not surface these failures.
    clippy::let_underscore_must_use,
    // restriction: the two branches are intentionally identical placeholders that will
    // diverge as the camera-on/off video-toggle copy lands.
    clippy::if_same_then_else,
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

pub mod account_restore;
pub mod bundled_plugins;
pub mod client_manager;
pub mod client_manager_timeout;
pub mod crypto;
pub(crate) mod event_stream;
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
