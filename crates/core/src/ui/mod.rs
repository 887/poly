//! UI components for Poly.
//!
//! All Dioxus UI components live here. The main entry point is [`App`],
//! which renders the setup wizard or the main app layout.
//!
//! ## Multi-Backend UI Architecture
//!
//! Account-scoped components live in `account/` with a **per-backend**
//! override structure:
//! - `account/common/` — Shared components used by ALL backends
//! - `account/demo/` — Demo backend UI overrides
//! - `account/stoat/` — Stoat backend UI overrides
//! - `account/discord/` — Discord backend UI overrides
//! - `account/matrix/` — Matrix backend UI overrides
//! - `account/teams/` — Teams backend UI overrides
//! - `account/poly_native/` — Poly native server UI overrides
//!
//! Dispatch is by `BackendType` match at render time. See
//! `docs/multi-client-architecture.md` for the full architecture guide.
//!
//! ## Component Hierarchy
//! - [`App`] — Root component (setup wizard or main layout)
//!   - [`SetupWizard`] — First-launch key generation
//!   - [`MainLayout`] — 4-column desktop layout
//!     - [`FavoritesBar`] — Left server icon list
//!     - [`account::ChannelList`] — Channel list for selected server
//!       - [`account::VoiceBar`] — Voice connection status bar
//!       - [`account::AccountBar`] — User info + quick controls
//!     - [`account::ChatView`] — Messages and input (text channels)
//!     - [`account::VoiceChannelView`] — Voice/video call view (voice channels)
//!     - [`account::EmojiPicker`] — Emoji grid for reactions and input
//!     - [`account::UserSidebar`] — Right user list
//!
//! ## Module layout
//! | Module | Contents |
//! |---|---|
//! | `account` | Multi-backend account-scoped UI (common + per-backend) |
//! | `account::common` | Shared components across all backends |
//! | `account::demo` | Demo backend overrides |
//! | `account::stoat` | Stoat backend overrides |
//! | `account::discord` | Discord backend overrides |
//! | `account::matrix` | Matrix backend overrides |
//! | `account::teams` | Teams backend overrides |
//! | `account::poly_native` | Poly native server overrides |
//! | `account::server` | Server-scoped UI (context menu, settings) |
//! | `account::settings` | Account-scoped settings (notifications) |
//! | `settings` | App-level settings page |
//! | `favorites_sidebar` | Left-most server icon list |
//! | `main_layout` | 4-column desktop shell |
//! | `voice_banner` | Top-spanning voice connection banner |
//! | `setup_wizard` | First-launch key generation wizard |
//!
//! ## 150-line component rule
//! Every `#[component]` fn body in any file under `src/ui/` MUST stay under
//! **150 lines** of RSX + logic. Extract sub-components rather than growing.
//! **NEVER hardcode demo/test data in UI components** — all data must flow
//! through the `ClientBackend` trait via `ClientManager`.

pub mod account;
pub mod actions;
mod agent;
pub mod client_ui;
pub use actions::{ActionCx, UiAction};
pub(crate) mod code_explorer;
pub mod dialogs;
pub(crate) mod context_menu;
pub(crate) mod create_channel;
pub(crate) mod create_forum_post;
pub(crate) mod create_server;
pub(crate) mod demo;
mod electron_titlebar;
mod favorites_sidebar;
pub(crate) mod main_layout;
pub mod routes;
pub(crate) mod search;
mod settings;
pub(crate) mod signup;
mod split_shell;
// Re-export the demo settings render function so demo.rs can register it in
// ClientManager::plugin_settings without a pub(crate) path through private modules.
#[cfg(feature = "demo")]
pub(crate) use settings::demo_settings_render_fn;
#[cfg(feature = "stoat")]
pub(crate) use settings::stoat_settings_render_fn;
// Re-export the poly server settings render function for the same reason.
#[cfg(feature = "server")]
pub(crate) use settings::poly_settings_render_fn;
mod runtime_js;
mod setup_wizard;
mod voice_banner;

pub use account::{AccountSwitcher, FriendsPanel};
pub use electron_titlebar::ElectronTitleBar;
pub use main_layout::MainLayout;
pub use routes::Route;
pub(crate) use runtime_js::load_js_asset;
pub use setup_wizard::SetupWizard;

use crate::client_manager::{ClientManager, SignupEntry};
use crate::state::{AppState, BatchedSignal, ChatData, LayoutMode, SettingsSection, View};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};
use routes::{route_targets_unknown_account, sync_route_to_app_state};
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::{AtomicBool, Ordering};

const STARTUP_OVERLAY_MIN_MS: u32 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StartupOverlayConfig {
    enabled: bool,
    min_visible_ms: u32,
}

struct StartupOverlayParams {
    enabled: bool,
    visible: bool,
    storage_ready: bool,
    setup_complete: bool,
}

#[derive(Clone, PartialEq, Debug)]
struct StartupOverlayAccount {
    id: String,
    label: String,
    avatar_url: Option<String>,
    status_class: String,
    status_symbol: String,
}

#[derive(Clone, PartialEq, Debug)]
struct StartupOverlayState {
    enabled: bool,
    visible: bool,
    compact: bool,
    headline: String,
    subline: String,
    logs: Vec<String>,
    accounts: Vec<StartupOverlayAccount>,
}

#[cfg(target_arch = "wasm32")]
fn startup_overlay_config_from_query() -> StartupOverlayConfig {
    let Some(window) = web_sys::window() else {
        return StartupOverlayConfig {
            enabled: true,
            min_visible_ms: STARTUP_OVERLAY_MIN_MS,
        };
    };
    let Ok(search) = window.location().search() else {
        return StartupOverlayConfig {
            enabled: true,
            min_visible_ms: STARTUP_OVERLAY_MIN_MS,
        };
    };

    let mut enabled = true;
    let mut min_visible_ms = STARTUP_OVERLAY_MIN_MS;

    for (key, value) in search
        .trim_start_matches('?')
        .split('&')
        .filter(|segment| !segment.is_empty())
        .filter_map(|segment| {
            let mut parts = segment.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next().unwrap_or_default();
            Some((key, value))
        })
    {
        if matches!(key, "boot" | "startup") {
            if matches!(value, "off" | "0" | "false") {
                enabled = false;
            } else if matches!(value, "on" | "1" | "true") {
                enabled = true;
            }
        }
        if key == "bootmin" {
            if let Ok(parsed) = value.parse::<u32>() {
                min_visible_ms = parsed;
            }
        }
    }

    StartupOverlayConfig {
        enabled,
        min_visible_ms,
    }
}

#[cfg(not(target_arch = "wasm32"))]
const fn startup_overlay_config_from_query() -> StartupOverlayConfig {
    StartupOverlayConfig {
        enabled: true,
        min_visible_ms: STARTUP_OVERLAY_MIN_MS,
    }
}

#[cfg(target_arch = "wasm32")]
fn startup_overlay_compact_mode() -> bool {
    web_sys::window()
        .and_then(|window| window.inner_width().ok())
        .and_then(|value| value.as_f64())
        .is_some_and(|width| width <= 640.0)
}

#[cfg(not(target_arch = "wasm32"))]
const fn startup_overlay_compact_mode() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn startup_now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
const fn startup_now_ms() -> f64 {
    0.0
}

fn startup_status_symbol(status_class: &str) -> &'static str {
    match status_class {
        "connected" => "check",
        "connecting" => "sync",
        "error" => "error",
        _ => "idle",
    }
}

fn startup_display_name(label: &str, fallback_id: &str) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        fallback_id.to_string()
    } else {
        trimmed.to_string()
    }
}

fn startup_log_lines(
    storage_ready: bool,
    setup_complete: bool,
    app_state: &AppState,
    client_manager: &ClientManager,
    chat_data: &ChatData,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(if storage_ready {
        "[ok] storage ready; persisted settings restored".to_string()
    } else {
        "[..] opening local storage and reading persisted state".to_string()
    });
    lines.push(if setup_complete {
        "[ok] app setup complete; preparing main shell".to_string()
    } else {
        "[..] awaiting setup wizard state".to_string()
    });
    lines.push(format!(
        "[..] layout mode {:?}; mirrored menus: {}",
        app_state.layout_mode, app_state.mirror_menu_layout
    ));

    if client_manager.sessions.is_empty() {
        lines.push("[..] no active accounts yet".to_string());
    } else {
        for (account_id, session) in &client_manager.sessions {
            let status_class = client_manager
                .connection_statuses
                .get(account_id)
                .map(poly_client::ConnectionStatus::css_class)
                .unwrap_or("disconnected");
            let verb = match status_class {
                "connected" => "connected",
                "connecting" => "connecting",
                "error" => "error",
                _ => "cached",
            };
            lines.push(format!(
                "[{}] {} ({})",
                if status_class == "connected" {
                    "ok"
                } else {
                    ".."
                },
                startup_display_name(&session.user.display_name, account_id),
                verb
            ));
        }
    }

    lines.push(if chat_data.loading {
        "[..] chat data still populating for active route".to_string()
    } else {
        "[ok] route data stable enough to reveal UI".to_string()
    });
    lines
}

fn startup_overlay_state(
    params: StartupOverlayParams,
    app_state: &AppState,
    client_manager: &ClientManager,
    chat_data: &ChatData,
) -> StartupOverlayState {
    let StartupOverlayParams {
        enabled,
        visible,
        storage_ready,
        setup_complete,
    } = params;
    let accounts = client_manager
        .sessions
        .iter()
        .map(|(account_id, session)| {
            let status_class = client_manager
                .connection_statuses
                .get(account_id)
                .map(poly_client::ConnectionStatus::css_class)
                .unwrap_or("disconnected")
                .to_string();
            StartupOverlayAccount {
                id: account_id.clone(),
                label: startup_display_name(&session.user.display_name, account_id),
                avatar_url: session.user.avatar_url.clone(),
                status_symbol: startup_status_symbol(&status_class).to_string(),
                status_class,
            }
        })
        .collect::<Vec<_>>();

    let compact = startup_overlay_compact_mode();
    let logs = startup_log_lines(
        storage_ready,
        setup_complete,
        app_state,
        client_manager,
        chat_data,
    );
    let ready = storage_ready && setup_complete && !chat_data.loading;

    StartupOverlayState {
        enabled,
        visible,
        compact,
        headline: if ready {
            "Boot sequence complete".to_string()
        } else {
            "Starting Poly".to_string()
        },
        subline: if ready {
            "Swapping the live workspace in smoothly...".to_string()
        } else {
            "Restoring shell state, accounts, and local cache...".to_string()
        },
        logs,
        accounts,
    }
}

// Include generated CSS asset definitions from build.rs.
// In release builds: single concatenated tailwind.css.
// In debug builds: individual CSS partial files (no giant tailwind.css to confuse agents).
// This file is .gitignored — do NOT edit it, it is overwritten on every build.
include!("css.rs");

#[cfg(target_arch = "wasm32")]
const LAYOUT_OVERRIDE_SESSION_KEY: &str = "poly_layout_query_override";

#[cfg(target_arch = "wasm32")]
static LAYOUT_OVERRIDE_BOOTSTRAPPED_THIS_PAGE: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "wasm32")]
const fn layout_mode_query_value(mode: LayoutMode) -> &'static str {
    match mode {
        LayoutMode::ForceMobile => "mobile",
        LayoutMode::ForceDesktop => "desktop",
        LayoutMode::AutoWidth => "auto-width",
        LayoutMode::AutoPortrait => "auto-portrait",
    }
}

#[cfg(target_arch = "wasm32")]
fn layout_mode_from_query_value(value: &str) -> Option<LayoutMode> {
    match value {
        "mobile" => Some(LayoutMode::ForceMobile),
        "desktop" => Some(LayoutMode::ForceDesktop),
        "auto-width" => Some(LayoutMode::AutoWidth),
        "auto-portrait" => Some(LayoutMode::AutoPortrait),
        _ => None,
    }
}

#[cfg(target_arch = "wasm32")]
fn layout_query_override_from_search(window: &web_sys::Window) -> Option<LayoutMode> {
    let Ok(search) = window.location().search() else {
        return None;
    };

    search
        .trim_start_matches('?')
        .split('&')
        .filter(|segment| !segment.is_empty())
        .filter_map(|segment| {
            let mut parts = segment.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next().unwrap_or_default();
            Some((key, value))
        })
        .find_map(|(key, value)| {
            if key == "layout" {
                return layout_mode_from_query_value(value);
            }

            if matches!(key, "mobile" | "polyMobile" | "forceMobile") {
                if matches!(value, "1" | "true" | "yes" | "on") {
                    return Some(LayoutMode::ForceMobile);
                }

                if matches!(value, "0" | "false" | "no" | "off") {
                    return Some(LayoutMode::ForceDesktop);
                }
            }

            None
        })
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn layout_query_override() -> Option<LayoutMode> {
    let Some(window) = web_sys::window() else {
        return None;
    };

    if let Some(override_mode) = layout_query_override_from_search(&window) {
        LAYOUT_OVERRIDE_BOOTSTRAPPED_THIS_PAGE.store(true, Ordering::SeqCst);
        if let Ok(Some(storage)) = window.session_storage() {
            let _ = storage.set_item(
                LAYOUT_OVERRIDE_SESSION_KEY,
                layout_mode_query_value(override_mode),
            );
        }
        return Some(override_mode);
    }

    // Fresh page load without an explicit layout override should clear any
    // previously remembered session override, so manually removing ?layout=...
    // from the URL restores normal behavior. Internal SPA navigations in the
    // same page lifetime skip this branch after the first bootstrap call.
    if !LAYOUT_OVERRIDE_BOOTSTRAPPED_THIS_PAGE.swap(true, Ordering::SeqCst) {
        if let Ok(Some(storage)) = window.session_storage() {
            let _ = storage.remove_item(LAYOUT_OVERRIDE_SESSION_KEY);
        }
        return None;
    }

    window
        .session_storage()
        .ok()
        .flatten()
        .and_then(|storage| storage.get_item(LAYOUT_OVERRIDE_SESSION_KEY).ok().flatten())
        .and_then(|value| layout_mode_from_query_value(&value))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn preserve_layout_override_query_in_url() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(mode) = layout_query_override() else {
        return;
    };

    let canonical_search = format!("?layout={}", layout_mode_query_value(mode));
    let Ok(current_search) = window.location().search() else {
        return;
    };
    if current_search == canonical_search {
        return;
    }

    let Ok(pathname) = window.location().pathname() else {
        return;
    };
    let hash = window.location().hash().unwrap_or_default();
    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(
            &wasm_bindgen::JsValue::NULL,
            "",
            Some(&format!("{pathname}{canonical_search}{hash}")),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn preserve_layout_override_query_in_url() {}

#[cfg(target_arch = "wasm32")]
pub(crate) fn layout_mode_is_mobile(mode: LayoutMode) -> bool {
    match mode {
        LayoutMode::ForceMobile => true,
        LayoutMode::ForceDesktop => false,
        LayoutMode::AutoPortrait => {
            let Some(window) = web_sys::window() else {
                return false;
            };
            let width = window
                .inner_width()
                .ok()
                .and_then(|value| value.as_f64())
                .unwrap_or_default();
            let height = window
                .inner_height()
                .ok()
                .and_then(|value| value.as_f64())
                .unwrap_or_default();
            height > width
        }
        LayoutMode::AutoWidth => web_sys::window()
            .and_then(|window| window.inner_width().ok())
            .and_then(|value| value.as_f64())
            .is_some_and(|width| width <= 640.0),
    }
}

#[cfg(not(target_arch = "wasm32"))]
const fn layout_mode_is_mobile(mode: LayoutMode) -> bool {
    matches!(mode, LayoutMode::ForceMobile)
}

pub(crate) fn effective_layout_mode(
    configured: LayoutMode,
    legacy_force_mobile: bool,
) -> LayoutMode {
    #[cfg(target_arch = "wasm32")]
    if let Some(override_mode) = layout_query_override() {
        return override_mode;
    }

    if legacy_force_mobile && configured == LayoutMode::AutoWidth {
        LayoutMode::ForceMobile
    } else {
        configured
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn load_persisted_layout_mode_from_window(
    window: &web_sys::Window,
) -> (LayoutMode, bool) {
    let persisted_mode = window
        .local_storage()
        .ok()
        .flatten()
        .and_then(|storage| storage.get_item("app_settings").ok().flatten())
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());

    let configured_mode = match persisted_mode
        .as_ref()
        .and_then(|json| json.get("layout_mode"))
        .and_then(serde_json::Value::as_str)
    {
        Some("ForceMobile") => LayoutMode::ForceMobile,
        Some("ForceDesktop") => LayoutMode::ForceDesktop,
        Some("AutoPortrait") => LayoutMode::AutoPortrait,
        Some("AutoWidth") | Some(_) | None => LayoutMode::AutoWidth,
    };

    let legacy_force_mobile = persisted_mode
        .as_ref()
        .and_then(|json| json.get("force_mobile_layout"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    (configured_mode, legacy_force_mobile)
}

const fn layout_mode_class(mode: LayoutMode) -> &'static str {
    match mode {
        LayoutMode::AutoWidth => "poly-layout-mode-auto-width",
        LayoutMode::AutoPortrait => "poly-layout-mode-auto-portrait",
        LayoutMode::ForceDesktop => "poly-layout-mode-force-desktop",
        LayoutMode::ForceMobile => "poly-layout-mode-force-mobile",
    }
}

fn app_root_class(app_state: &AppState) -> String {
    let effective_mode = effective_layout_mode(app_state.layout_mode, false);
    let mut classes = vec!["poly-app", layout_mode_class(effective_mode)];
    if app_state.mirror_menu_layout {
        classes.push("poly-menu-mirrored");
    }
    if app_state.mirror_chat_messages {
        classes.push("poly-chat-mirrored");
    }
    classes.join(" ")
}

// ── App — startup registration helpers ──────────────────────────────────────

/// Register all native backend signup entries into `ClientManager`.
///
/// Called once at `App` mount (via `use_effect`).  Each compiled-in backend
/// registers itself here.  WASM plugins register via the plugin host at
/// load time (not yet implemented).
///
/// This mirrors the WIT `plugin-metadata` pattern: the host has zero
/// compile-time knowledge of specific backends — each plugin registers itself.
fn register_native_signup_entries(client_manager: &mut BatchedSignal<ClientManager>) {
    #[cfg(feature = "stoat")]
    client_manager.batch(|cm| cm.register_signup_entry(SignupEntry {
        slug: "stoat",
        icon: "🦦",
        name_key: "plugin-stoat-signup-name",
        desc_key: "plugin-stoat-signup-desc",
        render: poly_stoat::signup::signup_render_fn,
    }));

    // Register the Poly Server backend when compiled with the `server` feature.
    // The render fn lives in poly-server-client — core has no knowledge of the form.
    #[cfg(feature = "server")]
    client_manager.batch(|cm| cm.register_signup_entry(SignupEntry {
        slug: "poly",
        icon: "🔷",
        name_key: "plugin-poly-signup-name",
        desc_key: "plugin-poly-signup-desc",
        render: poly_server_client::signup::signup_render_fn,
    }));
}

/// Register all native backend plugin settings pages into `ClientManager`.
///
/// Called once at `App` mount (via `use_effect`), immediately after
/// [`register_native_signup_entries`]. Plugin settings pages are registered
/// **unconditionally** regardless of whether the backend is currently active.
/// This ensures the settings page is always reachable in the Plugin Settings
/// nav so users can enable/disable the plugin or adjust its options at any
/// time.
///
/// Registration is idempotent: if the activation path (e.g. [`demo::toggle_demo`])
/// calls [`ClientManager::register_plugin_settings`] a second time, the entry
/// is simply replaced in place.
fn register_native_plugin_settings(client_manager: &mut BatchedSignal<ClientManager>) {
    use crate::client_manager::PluginSettingsEntry;

    #[cfg(feature = "demo")]
    client_manager.batch(|cm| cm.register_plugin_settings(PluginSettingsEntry {
        slug: "demo",
        nav_label_key: "plugin-demo-title",
        nav_icon: "🧪",
        render: demo_settings_render_fn,
    }));

    #[cfg(feature = "stoat")]
    client_manager.batch(|cm| cm.register_plugin_settings(PluginSettingsEntry {
        slug: "stoat",
        nav_label_key: "plugin-stoat-title",
        nav_icon: "🦦",
        render: stoat_settings_render_fn,
    }));

    #[cfg(feature = "server")]
    client_manager.batch(|cm| cm.register_plugin_settings(PluginSettingsEntry {
        slug: "poly",
        nav_label_key: "plugin-poly-title",
        nav_icon: "🔷",
        render: poly_settings_render_fn,
    }));
}

/// Register test accounts from each compiled-in native plugin into `ClientManager`.
///
/// Gated per-plugin feature so production builds without `discord`/`teams`/etc.
/// compile out the entire block and ship zero test credentials.
/// Called once at `App` mount via `use_effect`, immediately after
/// [`register_native_plugin_settings`].
fn register_native_test_accounts(client_manager: &mut BatchedSignal<ClientManager>) {
    // Per-call dedupe lives in `ClientManager::register_test_account` (retain
    // by (base_url, username), then push). DO NOT add a `.clear()` here —
    // an unconditional write inside this use_effect callback causes a
    // re-render loop (downstream readers subscribe to test_account_entries,
    // re-render fires the effect, clear writes the signal, repeat). The
    // boot-hang watchdog catches the loop after 20s.
    #[cfg(feature = "discord")]
    {
        for entry in poly_discord::signup::get_test_accounts() {
            let e = *entry;
            client_manager.batch(move |cm| cm.register_test_account(e));
        }
    }
    #[cfg(feature = "teams")]
    {
        for entry in poly_teams::signup::get_test_accounts() {
            let e = *entry;
            client_manager.batch(move |cm| cm.register_test_account(e));
        }
    }
    #[cfg(feature = "matrix")]
    {
        for entry in poly_matrix::signup::get_test_accounts() {
            let e = *entry;
            client_manager.batch(move |cm| cm.register_test_account(e));
        }
    }
    #[cfg(feature = "stoat")]
    {
        for entry in poly_stoat::signup::get_test_accounts() {
            let e = *entry;
            client_manager.batch(move |cm| cm.register_test_account(e));
        }
    }
    #[cfg(feature = "lemmy")]
    {
        for entry in poly_lemmy::signup::get_test_accounts() {
            let e = *entry;
            client_manager.batch(move |cm| cm.register_test_account(e));
        }
    }
    #[cfg(feature = "github")]
    {
        for entry in poly_github::signup::get_test_accounts() {
            let e = *entry;
            client_manager.batch(move |cm| cm.register_test_account(e));
        }
    }
    #[cfg(feature = "forgejo")]
    {
        for entry in poly_forgejo::signup::get_test_accounts() {
            let e = *entry;
            client_manager.batch(move |cm| cm.register_test_account(e));
        }
    }
    let n = client_manager.read().test_account_entries.len();
    if n > 0 {
        tracing::info!("registered {n} test accounts");
    }
}

/// Debug-only — sign in every registered test account sequentially after the
/// test servers have started. Uses the same auth + on-complete pipeline as
/// the `/signup/test` quick-add buttons; just drives them programmatically.
///
/// Sequential not parallel — each account's session write triggers a
/// reactive cascade through the favorites bar / channel list / chat data,
/// and bunching ten of them into one tick used to overwhelm the WASM
/// scheduler before the RouteSyncedWrite gate landed. A 100 ms gap between
/// sign-ins gives Dioxus's render loop time to drain.
/// Synthesize an offline `Session` from a `TestAccountEntry` so that
/// accounts whose server is unreachable still appear in the sidebar as
/// disconnected entries (clickable to reauth). The `instance_id` is
/// derived from `base_url` with the scheme stripped so the account lands
/// under the right `:instance_id` URL segment.
#[cfg(debug_assertions)]
fn offline_session_from_entry(entry: &poly_client::TestAccountEntry) -> poly_client::Session {
    use poly_client::{BackendType, PresenceStatus, Session, User};
    let backend = BackendType::from(entry.backend_slug);
    let instance_id = entry
        .base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string();
    let user_id = format!("{}:{}", entry.backend_slug, entry.username);
    Session {
        id: user_id.clone(),
        user: User {
            id: user_id,
            display_name: entry.label.to_string(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        token: String::new(),
        backend,
        icon_emoji: Some(entry.icon.to_string()),
        instance_id,
        backend_url: Some(entry.base_url.to_string()),
    }
}

/// Set to `true` after `auto_signin_test_accounts` has finished its loop
/// (every entry attempted — whether the underlying authenticate succeeded or
/// fell through to `register_offline_session`). Read by
/// `route_targets_unknown_account` so deep-link navigation defers the
/// "redirect to Settings on unknown account" verdict until the startup
/// sign-in burst has had its chance.
#[cfg(debug_assertions)]
pub static AUTO_SIGNIN_DONE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(debug_assertions)]
fn auto_signin_test_accounts(
    client_manager: BatchedSignal<ClientManager>,
    chat_data: BatchedSignal<ChatData>,
) {
    let entries: Vec<poly_client::TestAccountEntry> =
        client_manager.read().test_account_entries.to_vec();
    if entries.is_empty() {
        AUTO_SIGNIN_DONE.store(true, std::sync::atomic::Ordering::SeqCst);
        return;
    }
    let mut client_manager_w = client_manager;
    let on_complete = crate::ui::signup::build_on_complete_no_nav(client_manager, chat_data);
    spawn(async move {
        for entry in entries {
            let auth_fn = entry.authenticate;
            let label = entry.label.to_string();
            match (auth_fn)(
                entry.base_url.to_string(),
                entry.username.to_string(),
                entry.password.to_string(),
            )
            .await
            {
                Ok(completed) => {
                    tracing::info!("auto-signed in test account: {label}");
                    on_complete.call(completed);
                }
                Err(e) => {
                    tracing::warn!("auto-signin failed for {label}: {e}");
                    // Still register an offline Session so the account shows up
                    // in the sidebar as a disconnected entry the user can click
                    // through to reauth / retry. Without this the server-offline
                    // accounts vanish from Bar 1 entirely.
                    let session = offline_session_from_entry(&entry);
                    let account_id = session.id.clone();
                    client_manager_w
                        .batch(move |cm| cm.register_offline_session(account_id, session));
                }
            }
            // Brief gap between sign-ins so the per-session reactive
            // cascade settles before the next one fires.
            #[cfg(target_arch = "wasm32")]
            {
                let _ = dioxus::document::eval(
                    "setTimeout(() => dioxus.send(true), 100);",
                )
                .recv::<bool>()
                .await;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
        // Loop done — flip the flag so route_targets_unknown_account stops
        // deferring on missing test accounts.
        AUTO_SIGNIN_DONE.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

// ── App — async helpers ──────────────────────────────────────────────────────

/// Restore all persisted poly-server accounts from the token store.
///
/// Called during `init_storage` when `setup_complete` is true.
/// For each stored `AccountToken` with `backend == "poly"` and a valid
/// `instance_id` (the base URL), this function:
/// 1. Reads the device identity key from storage.
/// 2. Re-authenticates with the poly server using that key (token-based sign-in).
/// 3. Commits the resulting session + backend to `ClientManager` and `ChatData`.
/// 4. Fetches servers and populates `chat_data.servers` + `server_account_map`.
///
/// Accounts that fail to reconnect (e.g. server offline) are silently skipped.
#[cfg(feature = "server")]
async fn restore_poly_accounts(
    storage: &crate::storage::Storage,
    client_manager: BatchedSignal<ClientManager>,
    chat_data: BatchedSignal<ChatData>,
) {
    use crate::client_manager::BackendHandle;
    use poly_client::ClientBackend as _;
    use std::collections::HashMap;
    use std::sync::Arc;

    let Ok(tokens) = storage.get_account_tokens().await else {
        return;
    };

    let poly_tokens: Vec<_> = tokens
        .into_iter()
        .filter(|t| t.backend == "poly" && t.instance_id.is_some())
        .collect();

    if poly_tokens.is_empty() {
        return;
    }

    // Load the device identity key once — shared across all poly accounts.
    // Only required when we actually have poly-server tokens to restore;
    // checking earlier would warn on every boot for users with no poly
    // accounts at all.
    let Ok(Some(key_bytes)) = storage.get_identity_key().await else {
        tracing::warn!(
            "restore_poly_accounts: no identity key found but {} poly token(s) to restore — skipping",
            poly_tokens.len()
        );
        return;
    };

    tracing::info!(
        "Restoring {} poly server account(s) from storage",
        poly_tokens.len()
    );

    for token in poly_tokens {
        let Some(ref base_url) = token.instance_id else {
            continue;
        };

        let mut backend = poly_server_client::PolyServerBackend::new(base_url, key_bytes);

        let credentials = poly_client::AuthCredentials::Token(token.token.clone());
        match backend.authenticate(credentials).await {
            Ok(session) => {
                let account_id = session.id.clone();
                let backend_handle: BackendHandle = Arc::new(tokio::sync::RwLock::new(Box::new(
                    backend,
                )
                    as Box<dyn poly_client::ClientBackend + Send + Sync>));

                // Build server→account map.
                let mut server_map = HashMap::new();
                let servers = {
                    let guard = backend_handle.read().await;
                    guard.get_servers().await.unwrap_or_default()
                };
                for srv in &servers {
                    server_map.insert(srv.id.clone(), account_id.clone());
                }

                // Commit synchronously.
                {
                    let aid = account_id.clone();
                    let sess = session.clone();
                    let bh = backend_handle.clone();
                    client_manager.batch(move |cm| {
                        cm.commit_poly_server(aid, sess, bh, server_map);
                    });
                }
                {
                    let aid = account_id.clone();
                    chat_data.batch(move |cd| {
                        cd.account_sessions.insert(aid, session);
                    });
                }

                // Populate servers in chat_data and update the offline server
                // metadata cache so they survive the next restart even when the
                // server is unreachable.
                {
                    // Build cache records before consuming `servers`.
                    let cache_records: Vec<crate::storage::OfflineServerRecord> = servers
                        .iter()
                        .map(|srv| crate::storage::OfflineServerRecord {
                            id: srv.id.clone(),
                            name: srv.name.clone(),
                            icon_url: srv.icon_url.clone(),
                            banner_url: srv.banner_url.clone(),
                            backend: "poly".to_string(),
                            account_id: account_id.clone(),
                            account_display_name: srv.account_display_name.clone(),
                        })
                        .collect();
                    let new_fav_ids: Vec<String> = servers.iter().map(|s| s.id.clone()).collect();

                    let all_fav_ids = chat_data.batch(|cd| {
                        for id in &new_fav_ids {
                            if !cd.favorited_server_ids.contains(id) {
                                cd.favorited_server_ids.push(id.clone());
                            }
                        }
                        // Avoid duplicates if servers list was already populated.
                        for srv in servers {
                            if !cd.servers.iter().any(|s| s.id == srv.id) {
                                cd.servers.push(srv);
                            }
                        }
                        cd.favorited_server_ids.clone()
                    });

                    // Persist cache + favourites without holding any Signal lock.
                    if let Err(e) = storage.upsert_offline_server_cache(&cache_records).await {
                        tracing::warn!("Failed to cache server metadata: {e}");
                    }
                    crate::ui::favorites_sidebar::persist_favorites(all_fav_ids).await;
                }

                // Fetch DMs and friends in background.
                {
                    let guard = backend_handle.read().await;
                    let dms = guard.get_dm_channels().await.ok();
                    let friends = guard.get_friends().await.ok();
                    let aid = account_id.clone();
                    chat_data.batch(move |cd| {
                        if let Some(dms) = dms {
                            cd.dm_channels.extend(dms);
                        }
                        if let Some(friends) = friends {
                            for friend in friends {
                                let already = cd.friends.get(&aid).is_some_and(|v| v.iter().any(|f| f.id == friend.id));
                                if !already {
                                    cd.friends.entry(aid.clone()).or_default().push(friend);
                                }
                            }
                        }
                    });
                }

                tracing::info!("Restored poly account: {account_id}");
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to restore poly account {} from {base_url}: {e}. Showing as offline.",
                    token.account_id
                );
                // Still show the account in the favorites bar as offline so the
                // user knows it exists and can see its disconnected state.
                let offline_session = poly_client::Session {
                    id: token.account_id.clone(),
                    user: poly_client::User {
                        id: token.account_id.clone(),
                        display_name: token.display_name.clone(),
                        avatar_url: None,
                        presence: poly_client::PresenceStatus::Offline,
                        backend: poly_client::BackendType::from("poly"),
                    },
                    token: token.token.clone(),
                    backend: poly_client::BackendType::from("poly"),
                    icon_emoji: None,
                    instance_id: base_url.to_string(),
                    backend_url: Some(base_url.to_string()),
                };
                {
                    let aid = token.account_id.clone();
                    let sess = offline_session.clone();
                    client_manager.batch(move |cm| cm.register_offline_session(aid, sess));
                }
                {
                    let aid_c = token.account_id.clone();
                    let sess_c = offline_session;
                    chat_data.batch(move |cd| {
                        cd.account_sessions.insert(aid_c, sess_c);
                    });
                }

                // Restore cached server metadata so Bar 1 can render server
                // icons (shown as offline/disconnected) without a network round-trip.
                let cached = storage.get_offline_server_cache().await.unwrap_or_default();
                let account_servers: Vec<poly_client::Server> = cached
                    .into_iter()
                    .filter(|r| r.account_id == token.account_id)
                    .map(|r| poly_client::Server {
                        id: r.id,
                        name: r.name,
                        icon_url: r.icon_url,
                        banner_url: r.banner_url,
                        categories: Vec::new(),
                        backend: poly_client::BackendType::from("poly"),
                        unread_count: 0,
                        mention_count: 0,
                        account_id: r.account_id,
                        account_display_name: r.account_display_name,
                        default_channel_id: None,
                        description: None,
                        star_count: None,
                        language: None,
                        forks_count: None,
                        open_issues_count: None,
                    })
                    .collect();
                if !account_servers.is_empty() {
                    chat_data.batch(move |cd| {
                        for srv in account_servers {
                            if !cd.servers.iter().any(|s| s.id == srv.id) {
                                cd.servers.push(srv);
                            }
                        }
                    });
                }
            }
        }
    }
}

/// Initialise storage, apply persisted theme + locale, and decide the initial view.
///
/// Called once via `use_future` on App mount. Always sets `storage_ready` to
/// `true` when done — failures fall back to in-memory-only mode.
// DECISION(DX-STORAGE-4): storage init in use_future ensures it runs after
// the component mounts but before the first meaningful render completes.
async fn init_storage(
    theme_config: BatchedSignal<crate::theme::ThemeConfig>,
    mut storage_ready: Signal<bool>,
    app_state: BatchedSignal<AppState>,
    mut locale_sig: Signal<String>,
    client_manager: BatchedSignal<ClientManager>,
    chat_data: BatchedSignal<ChatData>,
) {
    match crate::storage::Storage::init().await {
        Ok(storage) => {
            let _ = crate::STORAGE.set(storage.clone());
            if let Err(e) = storage.run_migrations().await {
                tracing::warn!("Storage migration error: {e}");
            }
            match storage.get_theme_config().await {
                Ok(config) => theme_config.batch(|v| *v = config),
                Err(e) => tracing::warn!("Failed to load saved theme config: {e}"),
            }
            match storage.get_app_settings().await {
                Ok(settings) if settings.setup_complete => {
                    tracing::info!("Storage: setup complete, going to main layout");
                    crate::i18n::set_locale(&settings.locale);
                    *locale_sig.write() = settings.locale.clone();
                    {
                        let disabled = settings.disabled_native_backends.clone();
                        client_manager.batch(move |cm| cm.set_disabled_native_backends(disabled));
                    }
                    let restored_layout_mode =
                        effective_layout_mode(settings.layout_mode, settings.force_mobile_layout);
                    // Collapse the 7-write cascade into ONE batch — see
                    // CLAUDE.md § Common WASM-hang causes #1.
                    let mirror_menu_layout = settings.mirror_menu_layout;
                    let mirror_chat_messages = settings.mirror_chat_messages;
                    let member_list_grouping = settings.member_list_grouping;
                    let member_list_sort_order = settings.member_list_sort_order;
                    let member_list_show_offline = settings.member_list_show_offline;
                    app_state.batch(|st| {
                        st.is_setup_complete = true;
                        st.layout_mode = restored_layout_mode;
                        st.mirror_menu_layout = mirror_menu_layout;
                        st.mirror_chat_messages = mirror_chat_messages;
                        st.member_list_grouping = member_list_grouping;
                        st.member_list_sort_order = member_list_sort_order;
                        st.member_list_show_offline = member_list_show_offline;
                    });
                    // nav.view is written by sync_route_to_app_state on the next nav.push
                    // Restore favorited servers so Bar 1 repopulates immediately
                    // on launch — before the server list is fetched from the network.
                    if !settings.favorited_server_ids.is_empty() {
                        let fav_ids = settings.favorited_server_ids.clone();
                        chat_data.batch(move |cd| {
                            cd.favorited_server_ids = fav_ids;
                        });
                        tracing::info!(
                            "Restored {} favorited server(s) from storage",
                            settings.favorited_server_ids.len()
                        );
                    }
                    // Restore the demo client if it was active when the app last closed.
                    // toggle_demo activates all demo data; the Router's Root component
                    // then redirects to /demo/demo/dms once it mounts.
                    if settings.demo_active {
                        demo::toggle_demo(client_manager, chat_data, app_state).await;
                    }
                    // Collapse the 4-write nav.* cascade into ONE batch. When
                    // mobile layout is active, both sidebar visibility bits are
                    // forced false regardless of persisted values.
                    let is_mobile = layout_mode_is_mobile(restored_layout_mode);
                    let server_list_open = settings.server_member_list_open && !is_mobile;
                    let dm_list_open = settings.dm_member_list_open && !is_mobile;
                    app_state.batch(|st| {
                        st.nav.right_sidebar_visible = server_list_open;
                        st.nav.dm_right_sidebar_visible = dm_list_open;
                    });
                    // Restore per-account last-visited routes so account-switching
                    // returns to the correct page even after a page reload.
                    match storage.get_account_last_routes().await {
                        Ok(stored_routes) if !stored_routes.is_empty() => {
                            app_state.batch(|st| st.nav.account_last_routes = stored_routes);
                            tracing::info!("Restored per-account last routes from storage");
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!("Failed to read account last routes: {e}"),
                    }
                    match storage.get_account_last_dm_routes().await {
                        Ok(stored_routes) if !stored_routes.is_empty() => {
                            app_state.batch(|st| st.nav.account_last_dm_routes = stored_routes);
                            tracing::info!("Restored per-account last DM routes from storage");
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!("Failed to read account last DM routes: {e}"),
                    }

                    // Restore poly server accounts from persisted tokens.
                    // This runs after demo restore so both can coexist.
                    #[cfg(feature = "server")]
                    restore_poly_accounts(&storage, client_manager, chat_data).await;
                }
                Ok(_) => tracing::info!("Storage: no setup found, showing wizard"),
                Err(e) => tracing::warn!("Storage: failed to read app_settings: {e}"),
            }
            storage_ready.set(true);
        }
        Err(e) => {
            tracing::error!("Storage init failed: {e}. Running without persistence.");
            storage_ready.set(true);
        }
    }
}

/// Persist a completed setup to storage: account ID, locale, and default theme.
async fn persist_setup_completion(account_id: String) {
    let Some(s) = crate::STORAGE.get() else {
        return;
    };
    let locale = crate::i18n::current_locale();
    let settings = crate::storage::AppSettings {
        setup_complete: true,
        account_id,
        locale,
        theme: "neutral-dark".to_string(),
        // demo_active defaults to true — new users get demo data to explore.
        demo_active: true,
        // New install has no favorites yet.
        favorited_server_ids: Vec::new(),
        server_icon_overrides: std::collections::HashMap::new(),
        server_banner_overrides: std::collections::HashMap::new(),
        server_member_list_open: true,
        dm_member_list_open: false,
        media: crate::storage::MediaProviderSettings::default(),
        // All native backends enabled by default; no WASM plugins yet.
        disabled_native_backends: Vec::new(),
        wasm_plugins: Vec::new(),
        // Use WebSocket for real-time events by default.
        poly_use_websocket: true,
        force_mobile_layout: false,
        layout_mode: LayoutMode::AutoWidth,
        mirror_menu_layout: false,
        mirror_chat_messages: false,
        member_list_grouping: crate::state::MemberListGrouping::default(),
        member_list_sort_order: crate::state::MemberListSortOrder::default(),
        member_list_show_offline: true,
        account_order: Vec::new(),
        account_server_order: std::collections::HashMap::new(),
    };
    if let Err(e) = s.set_app_settings(&settings).await {
        tracing::error!("Failed to persist app settings: {e}");
    } else {
        tracing::info!("App settings persisted ✓");
    }
    if let Err(e) = s
        .set_theme_config(&crate::theme::ThemeConfig::default())
        .await
    {
        tracing::error!("Failed to persist default theme config: {e}");
    }
}

fn router_config(
    app_state: BatchedSignal<AppState>,
    client_manager: BatchedSignal<ClientManager>,
) -> dioxus_router::RouterConfig<Route> {
    dioxus_router::RouterConfig::default().on_update(
        move |state: dioxus_router::GenericRouterContext<Route>| {
            let route = state.current();
            sync_route_to_app_state(&route, app_state);
            preserve_layout_override_query_in_url();

            if route_targets_unknown_account(&route, &client_manager.read()) {
                app_state.batch(|st| st.settings_section = SettingsSection::Accounts);
                return Some(NavigationTarget::Internal(Route::SettingsRoute));
            }

            if matches!(route, Route::PageNotFound { .. } | Route::Root) {
                let cm = client_manager.read();
                if cm.demo_active {
                    let last_route = app_state
                        .read()
                        .nav
                        .account_last_routes
                        .values()
                        .find_map(|url| url.parse::<Route>().ok());
                    if let Some(stored_route) = last_route {
                        return Some(NavigationTarget::Internal(stored_route));
                    }
                    return Some(NavigationTarget::Internal(Route::DmsHome {
                        backend: "demo".to_string(),
                        instance_id: "demo".to_string(),
                        account_id: "demo-cat".to_string(),
                    }));
                }
                drop(cm); // poly-lint: allow long-read-guard — explicit drop(cm) before batch, audit M1
                app_state.batch(|st| st.settings_section = SettingsSection::Accounts);
                return Some(NavigationTarget::Internal(Route::SettingsRoute));
            }

            None
        },
    )
}

#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn AppBody(storage_ready: bool, setup_complete: bool, app_state: BatchedSignal<AppState>) -> Element {
    // Pull context signals so we can activate demo after setup completes.
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    rsx! {
        if !storage_ready {
            div { class: "storage-loading" }
        } else if !setup_complete {
            SetupWizard {
                on_complete: move |account_id: String| {
                    // Keep showing the wizard (storage-loading overlay) until
                    // toggle_demo completes so the router only mounts when demo
                    // data is already populated. That way the router's on_update
                    // initial redirect correctly lands on DmsHome instead of the
                    // empty Accounts settings page.
                    spawn(async move {
                        persist_setup_completion(account_id).await;
                        // Activate demo immediately so new users see demo data
                        // right away without needing an app restart.
                        // demo_active is true in persist_setup_completion so it
                        // will also be restored correctly on subsequent launches.
                        demo::toggle_demo(client_manager, chat_data, app_state).await;
                        // Only now flip is_setup_complete — this mounts the Router
                        // with demo already active, so on_update's initial redirect
                        // lands on DmsHome.
                        app_state.batch(|st| st.is_setup_complete = true);
                    });
                },
            }
        } else {
            Router::<Route> { config: move || router_config(app_state, use_context()) }
        }
    }
}

#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn StartupOverlay(state: StartupOverlayState) -> Element {
    if !state.enabled || !state.visible {
        return rsx! {};
    }

    let root_class = if state.compact {
        "poly-startup-overlay poly-startup-overlay-compact"
    } else {
        "poly-startup-overlay"
    };

    rsx! {
        div {
            class: "{root_class}",
            div { class: "poly-startup-backdrop" }
            div { class: "poly-startup-shell",
                div { class: "poly-startup-window",
                    div { class: "poly-startup-header",
                        span { class: "poly-startup-kicker", "Poly boot" }
                        h1 { class: "poly-startup-title", "{state.headline}" }
                        p { class: "poly-startup-subline", "{state.subline}" }
                    }
                    div { class: "poly-startup-accounts",
                        if state.accounts.is_empty() {
                            div { class: "poly-startup-account poly-startup-account-placeholder",
                                span { class: "poly-startup-account-avatar poly-startup-account-avatar-placeholder", "P" }
                                div { class: "poly-startup-account-copy",
                                    span { class: "poly-startup-account-name", "Preparing workspace" }
                                    span { class: "poly-startup-account-status idle", "waiting" }
                                }
                            }
                        } else {
                            for account in state.accounts {
                                div { class: "poly-startup-account", key: "{account.id}",
                                    div { class: "poly-startup-account-avatar-wrap",
                                        if let Some(url) = account.avatar_url.clone() {
                                            img {
                                                class: "poly-startup-account-avatar",
                                                src: "{url}",
                                                alt: "{account.label}",
                                            }
                                        } else {
                                            span { class: "poly-startup-account-avatar poly-startup-account-avatar-placeholder", "{account.label.chars().next().unwrap_or('?')}" }
                                        }
                                        span { class: "poly-startup-account-indicator {account.status_class}",
                                            span { class: "poly-startup-indicator-symbol", "{account.status_symbol}" }
                                        }
                                    }
                                    div { class: "poly-startup-account-copy",
                                        span { class: "poly-startup-account-name", "{account.label}" }
                                        span { class: "poly-startup-account-status {account.status_class}", "{account.status_class}" }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "poly-startup-log-window",
                        div { class: "poly-startup-log-header",
                            span { class: "poly-startup-log-title", "boot log" }
                            span { class: "poly-startup-log-badge", "live" }
                        }
                        div { class: "poly-startup-log-body",
                            for (index, line) in state.logs.iter().enumerate() {
                                {
                                    let line_number = format!("{:02}", index + 1);
                                    rsx! {
                                        div { class: "poly-startup-log-line", key: "boot-log-{index}",
                                            span { class: "poly-startup-log-gutter", "{line_number}" }
                                            span { class: "poly-startup-log-text", "{line}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── App component ─────────────────────────────────────────────────────────────

/// Root application component.
///
/// Shows a blank loading screen while storage initialises (<50 ms), then
/// routes to the setup wizard or the main layout based on saved settings.
///
/// ## Context provided to children
/// - `Signal<String>` — current locale (from [`crate::i18n::provide_locale_context`])
/// - `Signal<crate::theme::ThemeConfig>` — active theme (from [`provide_context`])
/// - `BatchedSignal<ClientManager>` — client manager for active backends
/// - `BatchedSignal<ChatData>` — reactive chat data store
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn App() -> Element {
    let app_state: BatchedSignal<AppState> =
        BatchedSignal::from_signal(use_signal(AppState::default));
    let storage_ready = use_signal(|| false);
    let startup_overlay_config = startup_overlay_config_from_query();
    let startup_overlay_enabled = startup_overlay_config.enabled;
    // Show the startup overlay immediately on boot when enabled so the boot
    // log is visible on fast web launches instead of only appearing after the
    // first post-mount effect pass.
    let mut startup_overlay_visible = use_signal(|| startup_overlay_enabled);
    let mut startup_overlay_started = use_signal(startup_now_ms);
    let mut startup_overlay_finished = use_signal(|| false);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (startup_overlay_started, startup_overlay_finished);
    // DECISION(DX-I18N-1): Signal<String> context; use_locale() in children subscribes.
    crate::i18n::provide_locale_context();
    let locale_sig = crate::i18n::use_locale();
    // DECISION(DX-THEME-1): BatchedSignal<ThemeConfig> context + <style> injection.
    let theme_config: BatchedSignal<crate::theme::ThemeConfig> =
        BatchedSignal::from_signal(use_signal(crate::theme::ThemeConfig::default));
    provide_context(theme_config);

    // DECISION(DX-2.5.1): ClientManager + ChatData as context BatchedSignals.
    let mut client_manager: BatchedSignal<ClientManager> =
        BatchedSignal::from_signal(use_signal(ClientManager::new));
    provide_context(client_manager);

    // ChatData is declared here (before the startup use_effect) so that the
    // auto-connect helper can capture it by copy in the same effect.
    let chat_data: BatchedSignal<ChatData> =
        BatchedSignal::from_signal(use_signal(ChatData::default));
    provide_context(chat_data);

    // Register all native backend signup entries.  This mirrors the WIT
    // plugin-metadata pattern: the host has no compile-time knowledge of
    // which backends exist — each plugin registers itself once at startup.
    // DECISION(DX-SIGNUP-1): Signup entries are registered at App mount
    // so they are available before the first SignupPickerPage render.
    //
    // Plugin settings pages are also registered here unconditionally so the
    // Demo Settings and Poly Server settings pages are always reachable in
    // the Plugin Settings nav, even before the user has activated the plugin.
    //
    // Under dev-plugins (discord + teams features), test accounts are also
    // registered and auto-connected so the app boots pre-authenticated.
    // One-shot registration: `use_hook` runs once per mount and does NOT
    // subscribe to the signals it writes to. `use_effect` would re-fire every
    // time the async auto-signin wrote a new session, spawning a fresh loop
    // per successful sign-in and producing N² "session already exists"
    // warnings in the log.
    use_hook(|| {
        register_native_signup_entries(&mut client_manager);
        register_native_plugin_settings(&mut client_manager);
        register_native_test_accounts(&mut client_manager);
        #[cfg(debug_assertions)]
        {
            auto_signin_test_accounts(client_manager, chat_data);
        }
        let _ = chat_data;
    });

    // Provide app_state as context so child components subscribe independently
    // via use_context() instead of receiving it as a prop (which enables Dioxus
    // prop-comparison skip optimization that can suppress signal-triggered re-renders).
    provide_context(app_state);

    // Pack B wiring — global toast queue + sidebar refresh counter so
    // `ActionOutcome::Toast`, `Pending`, and `RefreshSidebar` cross the last
    // mile into user-visible UX. See `ui::client_ui::action_outcome` +
    // `ui::client_ui::toast` for details.
    let toast_queue: Signal<Vec<crate::ui::client_ui::ToastMessage>> =
        use_signal(Vec::new);
    provide_context(toast_queue);
    let sidebar_refresh: Signal<u32> = use_signal(|| 0u32);
    provide_context(sidebar_refresh);

    use_future(move || async move {
        init_storage(
            theme_config,
            storage_ready,
            app_state,
            locale_sig,
            client_manager,
            chat_data,
        )
        .await;
    });
    let theme_css = crate::theme::generate_css(&theme_config.read());
    let storage_ready_now = *storage_ready.read();
    let app_state_snapshot = app_state.read().clone();
    let setup_complete = app_state_snapshot.is_setup_complete;
    let root_class = app_root_class(&app_state_snapshot);
    let client_manager_snapshot = client_manager.read().clone();
    let chat_data_snapshot = chat_data.read().clone();

    use_effect(move || {
        if !startup_overlay_enabled {
            startup_overlay_visible.set(false);
            startup_overlay_finished.set(false);
            return;
        }
        if !*startup_overlay_visible.read() && !*startup_overlay_finished.read() {
            startup_overlay_started.set(startup_now_ms());
            startup_overlay_visible.set(true);
            return;
        }
        if *startup_overlay_finished.read() {
            return;
        }

        startup_overlay_finished.set(true);

        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            {
                let elapsed_ms = js_sys::Date::now() - *startup_overlay_started.read();
                let remaining_ms = startup_overlay_config
                    .min_visible_ms
                    .saturating_sub(elapsed_ms.max(0.0) as u32);
                let _ = document::eval(&format!(
                    "setTimeout(() => requestAnimationFrame(() => requestAnimationFrame(() => dioxus.send(true))), {});",
                    remaining_ms
                ))
                .recv::<bool>()
                .await;

                // Wait for auto_signin_test_accounts loop to finish so the
                // overlay doesn't dismiss while accounts are still popping in
                // (visible as icons appearing one-by-one in Bar 1). Cap the
                // wait at 8 s so a stalled sign-in doesn't trap the user
                // behind the overlay forever — slow accounts will pop in
                // after the overlay vanishes, which is acceptable degradation.
                #[cfg(debug_assertions)]
                {
                    use std::sync::atomic::Ordering;
                    let deadline_ms = js_sys::Date::now() + 8000.0;
                    while !AUTO_SIGNIN_DONE.load(Ordering::SeqCst)
                        && js_sys::Date::now() < deadline_ms
                    {
                        let _ = document::eval(
                            "setTimeout(() => dioxus.send(true), 100);",
                        )
                        .recv::<bool>()
                        .await;
                    }
                }
            }
            startup_overlay_visible.set(false);
            startup_overlay_finished.set(true);
        });
    });

    #[cfg(target_arch = "wasm32")]
    {
        let startup_state = startup_overlay_state(
            StartupOverlayParams {
                enabled: startup_overlay_enabled,
                visible: *startup_overlay_visible.read(),
                storage_ready: storage_ready_now,
                setup_complete,
            },
            &app_state_snapshot,
            &client_manager_snapshot,
            &chat_data_snapshot,
        );
        let script = format!(
            "window.__polyStartupState = {{ enabled: {}, visible: {}, phase: '{}' }}; document.documentElement.setAttribute('data-poly-startup-phase', '{}');",
            startup_state.enabled,
            startup_state.visible,
            if startup_state.visible { "booting" } else { "revealed" },
            if startup_state.visible { "booting" } else { "revealed" },
        );
        let _ = document::eval(&script);
    }

    let startup_state = startup_overlay_state(
        StartupOverlayParams {
            enabled: startup_overlay_enabled,
            visible: *startup_overlay_visible.read(),
            storage_ready: storage_ready_now,
            setup_complete,
        },
        &app_state_snapshot,
        &client_manager_snapshot,
        &chat_data_snapshot,
    );

    rsx! {
        for asset in CSS_SLICES.iter() {
            document::Link { rel: "stylesheet", href: *asset }
        }
        style { id: "poly-theme", "{theme_css}" }
        div { class: root_class,
            if startup_state.visible {
                StartupOverlay { state: startup_state.clone() }
            }
            ElectronTitleBar {}
            div { class: if startup_state.visible { "poly-app-stage poly-app-stage-hidden" } else { "poly-app-stage" },
                AppBody {
                    storage_ready: storage_ready_now,
                    setup_complete,
                    app_state,
                }
            }
        }
    }
}
