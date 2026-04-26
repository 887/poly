//! Plugin manager settings page.
//!
//! Every Poly messenger backend is a WASM-style plugin. They split into two
//! groups based on **how they reach the user's machine**:
//!
//! - **Built-in WASM plugins** — bundled with the Poly binary at compile
//!   time and updated when the user updates the app. The registry lives in
//!   [`crate::client_manager::builtin_backend_descriptors`] and is the
//!   single source of truth shared with the signup picker / `ClientManager`.
//! - **Sideloaded WASM plugins** — added by the user at runtime via the
//!   "Add Plugin" form below (URL or local `.wasm` file).
//!
//! This page lets the user:
//! - Toggle built-in plugins on / off with checkboxes
//! - Add sideloaded WASM plugins from URLs (the app appends `?wit=<version>`)
//!   or local files
//! - Toggle sideloaded WASM plugins on / off
//! - Remove sideloaded WASM plugins
//!
//! Accounts are *sessions created by a logged-in plugin* — they live in the
//! Accounts settings page, not here.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.

use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::storage::WasmPluginEntry;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the plugins settings section.
pub enum PluginsSettingsAction {
    /// Toggle a native backend plugin on or off.
    ToggleNativeBackend(String),
    /// Toggle a WASM plugin on or off by index.
    ToggleWasmPlugin(usize),
    /// Remove a WASM plugin by index.
    RemoveWasmPlugin(usize),
    /// Add a new WASM plugin.
    AddWasmPlugin(WasmPluginEntry),
}

impl UiAction for PluginsSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::ToggleNativeBackend(_slug) => todo!("phase-E: toggle native backend"),
            Self::ToggleWasmPlugin(_index) => todo!("phase-E: toggle WASM plugin"),
            Self::RemoveWasmPlugin(_index) => todo!("phase-E: remove WASM plugin"),
            Self::AddWasmPlugin(_entry) => todo!("phase-E: add WASM plugin"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::state::AppState;

    /// Structural test: all variants construct and the type implements UiAction.
    #[test]
    fn plugins_settings_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<PluginsSettingsAction>();
        let _ = PluginsSettingsAction::ToggleNativeBackend("demo".into());
        let _ = PluginsSettingsAction::ToggleWasmPlugin(0);
        let _ = PluginsSettingsAction::RemoveWasmPlugin(0);
        let _ = PluginsSettingsAction::AddWasmPlugin(WasmPluginEntry {
            url: "https://example.com/plugin.wasm".into(),
            name: None,
            enabled: true,
            bundled: false,
        });
    }
}

/// WIT version string appended to WASM plugin fetch URLs.
const WIT_VERSION: &str = "0.1.0";

// The list of built-in WASM plugins lives in
// [`crate::client_manager::builtin_backend_descriptors`] — a single
// registry shared with the signup picker and `ClientManager` so adding /
// removing a built-in is a one-line edit in one place. See the policy
// comment there for why Discord and Teams are excluded.
use crate::client_manager::builtin_backend_descriptors;

/// Load current app settings from storage (or return default).
async fn load_settings() -> crate::storage::AppSettings {
    if let Some(storage) = crate::STORAGE.get() {
        storage.get_app_settings().await.unwrap_or_default()
    } else {
        crate::storage::AppSettings::default()
    }
}

/// Save updated settings to storage.
async fn save_settings(settings: &crate::storage::AppSettings) {
    if let Some(storage) = crate::STORAGE.get()
        && let Err(e) = storage.set_app_settings(settings).await
    {
        tracing::warn!("Failed to save plugin settings: {e}");
    }
}

/// A single built-in plugin row with toggle checkbox.
///
/// Despite the historical "native" name in code, every Poly backend is
/// conceptually a WASM plugin — `BuiltinPluginRow` renders the entries that
/// are bundled with the binary, distinguishing them from sideloaded plugins
/// added at runtime.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn BuiltinPluginRow(
    slug: String,
    icon: String,
    name: String,
    description: String,
    available: bool,
    enabled: bool,
    account_count: usize,
    on_toggle: EventHandler<String>,
) -> Element {
    let slug_for_toggle = slug.clone();
    rsx! {
        div {
            class: if available { "plugin-row" } else { "plugin-row plugin-row-unavailable" },
            label { class: "plugin-row-toggle toggle-switch",
                input {
                    r#type: "checkbox",
                    role: "switch",
                    checked: enabled,
                    "aria-checked": if enabled { "true" } else { "false" },
                    onchange: move |_| on_toggle.call(slug_for_toggle.clone()),
                }
                span { class: "toggle-slider" }
            }
            div { class: "plugin-row-icon", "{icon}" }
            div { class: "plugin-row-info",
                div { class: "plugin-row-name",
                    "{name}"
                    if !available {
                        span { class: "plugin-not-compiled",
                            " ({t(\"plugins-not-compiled\")})"
                        }
                    }
                }
                div { class: "plugin-row-description", "{description}" }
                if account_count > 0 {
                    div { class: "plugin-row-accounts",
                        "{t(\"plugins-active-accounts\")}: {account_count}"
                    }
                }
            }
            div { class: "plugin-row-meta",
                span { class: "plugin-type-badge native", "{t(\"plugins-type-builtin\")}" }
            }
        }
    }
}

/// A single WASM plugin row with toggle and remove buttons.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn WasmPluginRow(
    index: usize,
    entry: WasmPluginEntry,
    on_toggle: EventHandler<usize>,
    on_remove: EventHandler<usize>,
) -> Element {
    let display_name = entry
        .name
        .as_deref()
        .unwrap_or(entry.url.as_str())
        .to_string();
    let idx_toggle = index;
    let idx_remove = index;
    let is_bundled = entry.bundled;
    // In-DOM remove confirmation. Avoids window.confirm() (per existing
    // pattern in `LeaveServerConfirm`) — Remove is destructive so the
    // first click only arms it, the second click commits.
    let mut confirm_remove = use_signal(|| false);
    // Bundled plugins get a recognisable icon per known slug; user-added
    // plugins fall back to the generic plug emoji.
    let icon = if is_bundled {
        match crate::bundled_plugins::slug_from_url(&entry.url) {
            Some("discord") => "🎮",
            Some("teams") => "🟦",
            _ => "📦",
        }
    } else {
        "🔌"
    };
    rsx! {
        div { class: "plugin-row",
            label { class: "plugin-row-toggle toggle-switch",
                input {
                    r#type: "checkbox",
                    role: "switch",
                    checked: entry.enabled,
                    "aria-checked": if entry.enabled { "true" } else { "false" },
                    onchange: move |_| on_toggle.call(idx_toggle),
                }
                span { class: "toggle-slider" }
            }
            div { class: "plugin-row-icon", "{icon}" }
            div { class: "plugin-row-info",
                div { class: "plugin-row-name", "{display_name}" }
                div { class: "plugin-row-description", "{entry.url}" }
            }
            div { class: "plugin-row-meta",
                if is_bundled {
                    span { class: "plugin-type-badge wasm", "{t(\"plugins-type-bundled\")}" }
                } else {
                    span { class: "plugin-type-badge wasm", "{t(\"plugins-type-sideloaded\")}" }
                }
                if confirm_remove() {
                    span { class: "plugin-row-confirm-prompt", "{t(\"plugins-remove-confirm\")}" }
                    button {
                        class: "btn btn-small",
                        onclick: move |_| confirm_remove.set(false),
                        "{t(\"plugins-remove-cancel\")}"
                    }
                    button {
                        class: "btn btn-small btn-danger",
                        onclick: move |_| {
                            confirm_remove.set(false);
                            on_remove.call(idx_remove);
                        },
                        "{t(\"plugins-remove-yes\")}"
                    }
                } else {
                    button {
                        class: "btn btn-small btn-danger",
                        onclick: move |_| confirm_remove.set(true),
                        "{t(\"plugins-remove\")}"
                    }
                }
            }
        }
    }
}

/// URL input form to add a new WASM plugin, or install from a local file.
///
/// Two install modes: from URL (with WIT version appended) or from a local .wasm file.
/// Display name is inferred from the URL hostname or file name — no manual entry needed.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(allow_default)]
#[component]
fn AddWasmPlugin(on_add: EventHandler<WasmPluginEntry>) -> Element {
    let mut url = use_signal(String::new);
    let mut error = use_signal(String::new);
    // "url" | "file"
    let mut install_mode = use_signal(|| "url".to_string());
    rsx! {
        div { class: "plugin-add-form",
            h4 { "{t(\"plugins-add-wasm-title\")}" }
            // Mode tabs
            div { class: "plugin-install-tabs",
                button {
                    class: if *install_mode.read() == "url" { "plugin-install-tab active" } else { "plugin-install-tab" },
                    onclick: move |_| install_mode.set("url".to_string()),
                    "{t(\"plugins-install-from-url\")}"
                }
                button {
                    class: if *install_mode.read() == "file" { "plugin-install-tab active" } else { "plugin-install-tab" },
                    onclick: move |_| install_mode.set("file".to_string()),
                    "{t(\"plugins-install-from-file\")}"
                }
            }

            if *install_mode.read() == "url" {
                // URL install
                p { class: "settings-description",
                    "{t(\"plugins-add-wasm-description\")}"
                }
                div { class: "plugin-add-row",
                    input {
                        r#type: "text",
                        class: "plugin-url-input",
                        placeholder: "{t(\"plugins-url-placeholder\")}",
                        value: "{url.read()}",
                        oninput: move |e| {
                            url.set(e.value());
                            error.set(String::new());
                        },
                    }
                    button {
                        class: "btn btn-primary",
                        disabled: url.read().trim().is_empty(),
                        onclick: move |_| {
                            let u = url.read().trim().to_string();
                            if u.is_empty() {
                                error.set(t("plugins-url-required"));
                                return;
                            }
                            on_add.call(WasmPluginEntry {
                                url: u,
                                name: None,
                                enabled: true,
                                bundled: false,
                            });
                            url.set(String::new());
                        },
                        "{t(\"plugins-add-btn\")}"
                    }
                }
                p { class: "plugin-add-hint",
                    "{t(\"plugins-wit-hint\")}: {WIT_VERSION}"
                }
            } else {
                // File install
                p { class: "settings-description",
                    "{t(\"plugins-add-file-description\")}"
                }
                div { class: "plugin-add-row",
                    input {
                        r#type: "file",
                        class: "plugin-file-input",
                        accept: ".wasm",
                        onchange: move |e| {
                            // Extract file name from the event value (browser returns path or name)
                            let raw = e.value();
                            let file_name = raw
                                .split(['/', '\\'])
                                .next_back()
                                .unwrap_or(raw.as_str())
                                .to_string();
                            if !file_name.is_empty() && file_name != "null" {
                                url.set(file_name.clone());
                                on_add.call(WasmPluginEntry {
                                    // Store as file:// reference; actual loading handled by plugin host
                                    url: format!("file://{file_name}"),
                                    name: Some(file_name),
                                    enabled: true,
                                    bundled: false,
                                });
                                url.set(String::new());
                            }
                        },
                    }
                }
                p { class: "plugin-add-hint",
                    "{t(\"plugins-file-hint\")}"
                }
            }

            if !error.read().is_empty() {
                p { class: "plugin-add-error", "{error.read()}" }
            }
        }
    }
}

/// Plugin manager settings page.
///
/// Two sections, both showing WASM-style messenger backend plugins:
/// - **Built-in WASM plugins** — bundled with this Poly binary, iterated
///   from [`crate::client_manager::builtin_backend_descriptors`].
/// - **Sideloaded WASM plugins** — added by the user via URL or local file.
///
/// **Accounts** ("Cat (demo)", "Dog (demo)") are sessions created when a plugin
/// authenticates a user — they appear in the Accounts settings page. Here we
/// manage *which plugins are available and enabled*.
#[rustfmt::skip]
#[ui_action(PluginsSettingsAction)]
#[context_menu(none)]
#[component]
pub fn PluginsSettings() -> Element {
    let client_manager: BatchedSignal<crate::client_manager::ClientManager> = use_context();
    let chat_data: BatchedSignal<crate::state::ChatData> = use_context();
    let app_state: crate::state::BatchedSignal<crate::state::AppState> = use_context();

    // Local reactive copies of the persisted list — updated on every toggle/add/remove.
    let mut disabled: Signal<Vec<String>> = use_signal(Vec::new);
    let mut wasm_plugins: Signal<Vec<WasmPluginEntry>> = use_signal(Vec::new);
    // Tracks bundled plugin slugs the user has explicitly removed so that
    // `ensure_bundled_plugins` doesn't re-inject them on the next launch.
    let mut removed_bundled: Signal<Vec<String>> = use_signal(Vec::new);

    // Load settings from storage once on mount.
    use_future(move || async move {
        let s = load_settings().await;
        disabled.set(s.disabled_native_backends.clone());
        wasm_plugins.set(s.wasm_plugins.clone());
        removed_bundled.set(s.removed_bundled_plugins.clone());
    });

    let disabled_snap = disabled.read().clone();
    let wasm_snap = wasm_plugins.read().clone();

    rsx! {
        div { class: "settings-section",
            h2 { class: "settings-section-title", "{t(\"settings-plugins\")}" }
            p { class: "settings-section-description",
                "{t(\"settings-plugins-description\")}"
            }

            // ── Built-in WASM plugins ──────────────────────────────────────
            h3 { class: "settings-subsection-title",
                "{t(\"plugins-builtin-title\")}"
            }
            p { class: "settings-description",
                "{t(\"plugins-builtin-description\")}"
            }
            div { class: "plugin-list",
                for backend in builtin_backend_descriptors() {
                    {
                        let slug = backend.slug.to_string();
                        let slug_key = slug.clone();
                        // Demo enabled state is driven by demo_active (not disabled_native_backends).
                        let enabled = if backend.slug == "demo" {
                            client_manager.read().demo_active
                        } else {
                            !disabled_snap.contains(&slug)
                        };
                        let account_count = client_manager
                            .read()
                            .sessions
                            .values()
                            .filter(|s| s.backend.slug() == backend.slug)
                            .count();
                        rsx! {
                            BuiltinPluginRow {
                                key: "{slug_key}",
                                slug: slug.clone(),
                                icon: backend.icon.to_string(),
                                name: backend.name.to_string(),
                                description: backend.description.to_string(),
                                available: backend.available,
                                enabled,
                                account_count,
                                on_toggle: move |toggled: String| {
                                    if toggled == "demo" {
                                        // Use toggle_demo to keep demo_active and nav
                                        // visibility in sync across the whole app.
                                        spawn(async move {
                                            crate::ui::demo::toggle_demo(
                                                client_manager, chat_data, app_state,
                                            ).await;
                                        });
                                    } else {
                                        // For any other native backend:
                                        // - If currently enabled (toggling OFF): disconnect
                                        //   all active accounts of that backend type.
                                        // - If currently disabled (toggling ON): re-enable
                                        //   the option (user must add accounts via /signup).
                                        let is_enabled = !disabled.read().contains(&toggled);
                                        if is_enabled {
                                            // Toggling OFF — actually disconnect all sessions.
                                            let backend_type =
                                                poly_client::BackendType::from_slug(&toggled);
                                            {
                                                let bt = backend_type;
                                                // Phase 1 (sync): take handles + clear
                                                // ClientManager state. No await held.
                                                let bt_c = bt.clone();
                                                let (removed_ids, handles) = client_manager
                                                    .batch(move |cm| cm.take_accounts_by_backend(bt_c));
                                                let backend_slug = bt.slug().to_string();
                                                // Update local disabled signal immediately for
                                                // instant UI feedback.
                                                disabled.write().push(toggled.clone());
                                                let new_disabled = disabled.read().clone();
                                                let nd = new_disabled.clone();
                                                client_manager
                                                    .batch(move |cm| cm.set_disabled_native_backends(nd));
                                                let wasm = wasm_plugins.read().clone();
                                                spawn(async move {
                                                    // Phase 2: async logout (no signal lock).
                                                    for h in handles {
                                                        let mut g = h.write().await;
                                                        let _ = g.logout().await;
                                                    }
                                                    // Phase 3: clean up ChatData.
                                                    if !removed_ids.is_empty() {
                                                        chat_data.batch(|cd| {
                                                            cd.servers.retain(|s| {
                                                                s.backend != bt
                                                                    || !removed_ids
                                                                        .contains(&s.account_id)
                                                            });
                                                            cd.dm_channels.retain(|d| {
                                                                d.backend != bt
                                                                    || !removed_ids
                                                                        .contains(&d.account_id)
                                                            });
                                                            cd.groups.retain(|g| {
                                                                g.backend != bt
                                                                    || !removed_ids
                                                                        .contains(&g.account_id)
                                                            });
                                                            cd.notifications.retain(|n| {
                                                                n.backend != bt
                                                                    || !removed_ids
                                                                        .contains(&n.account_id)
                                                            });
                                                            for id in &removed_ids {
                                                                cd.friends.remove(id.as_str());
                                                            }
                                                            for id in &removed_ids {
                                                                cd.account_sessions
                                                                    .remove(id.as_str());
                                                            }
                                                            // Retain only favorites that still have a matching server.
                                                            // Collect the server IDs first to avoid concurrent borrow.
                                                            let live_server_ids: Vec<String> = cd
                                                                .servers
                                                                .iter()
                                                                .map(|s| s.id.clone())
                                                                .collect();
                                                            cd.favorited_server_ids
                                                                .retain(|fid| live_server_ids.contains(fid));
                                                        });
                                                    }
                                                    // Phase 4: remove stored tokens.
                                                    if let Some(storage) = crate::STORAGE.get() {
                                                        for id in &removed_ids {
                                                            let _ = storage
                                                                .remove_account_token(
                                                                    &backend_slug,
                                                                    id,
                                                                )
                                                                .await;
                                                        }
                                                    }
                                                    // Phase 5: persist the disabled state.
                                                    let mut settings =
                                                        load_settings().await;
                                                    settings.disabled_native_backends =
                                                        new_disabled;
                                                    settings.wasm_plugins = wasm;
                                                    // removed_bundled_plugins is unchanged here;
                                                    // load_settings + save round-trip preserves it.
                                                    save_settings(&settings).await;
                                                });
                                            }
                                        } else {
                                            // Toggling ON — re-enable the backend option.
                                            disabled.write().retain(|s| s != &toggled);
                                            let new_disabled = disabled.read().clone();
                                            let nd = new_disabled.clone();
                                            client_manager
                                                .batch(move |cm| cm.set_disabled_native_backends(nd));
                                            let wasm = wasm_plugins.read().clone();
                                            spawn(async move {
                                                let mut s = load_settings().await;
                                                s.disabled_native_backends = new_disabled;
                                                s.wasm_plugins = wasm;
                                                save_settings(&s).await;
                                            });
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }

            // ── Sideloaded WASM plugins ────────────────────────────────────
            h3 { class: "settings-subsection-title",
                "{t(\"plugins-sideloaded-title\")}"
            }
            p { class: "settings-description",
                "{t(\"plugins-sideloaded-description\")}"
            }
            div { class: "plugin-list",
                if wasm_snap.is_empty() {
                    div { class: "plugin-empty",
                        "{t(\"plugins-none-loaded\")}"
                    }
                }
                for (idx, entry) in wasm_snap.iter().enumerate() {
                    WasmPluginRow {
                        key: "{idx}",
                        index: idx,
                        entry: entry.clone(),
                        on_toggle: move |i: usize| {
                            // Mirror into the local signal first; persist
                            // through the canonical
                            // `plugin_admin::set_plugin_enabled_with_storage`
                            // helper (same code path the MCP
                            // `/host/plugins/set-enabled` endpoint uses).
                            let mut wasm = wasm_plugins.write();
                            let target: Option<(String, bool)> =
                                wasm.get_mut(i).map(|p| {
                                    p.enabled = !p.enabled;
                                    (p.url.clone(), p.enabled)
                                });
                            let new_wasm = wasm.clone();
                            drop(wasm);
                            if let Some((url, enabled)) = target {
                                spawn(async move {
                                    if let Some(storage) = crate::STORAGE.get() {
                                        if let Err(e) =
                                            crate::plugin_admin::set_plugin_enabled_with_storage(
                                                storage, &url, enabled,
                                            )
                                            .await
                                        {
                                            tracing::warn!("Failed to toggle plugin: {e}");
                                        }
                                    }
                                });
                            }
                            let dis = disabled.read().clone();
                            // Reconcile signup entries IMMEDIATELY against
                            // the new wasm_plugins state so the favorites
                            // "+" picker reflects the toggle without a
                            // restart.
                            let new_wasm_for_sync = new_wasm.clone();
                            client_manager.batch(move |cm| {
                                let s = crate::storage::AppSettings {
                                    wasm_plugins: new_wasm_for_sync,
                                    ..crate::storage::AppSettings::default()
                                };
                                crate::bundled_plugins::sync_bundled_signup_entries(cm, &s);
                            });
                            spawn(async move {
                                let mut s = load_settings().await;
                                s.disabled_native_backends = dis;
                                s.wasm_plugins = new_wasm;
                                save_settings(&s).await;
                            });
                        },
                        on_remove: move |i: usize| {
                            // Mirror into the local signal first (drop the
                            // entry + record the bundled tombstone for an
                            // immediate UI update), then delegate persistence
                            // to `plugin_admin::remove_wasm_plugin_with_storage`
                            // — same code path the MCP `/host/plugins/remove`
                            // endpoint uses, so the tombstone semantics stay
                            // identical across both surfaces.
                            let mut wasm = wasm_plugins.write();
                            let removed_url: Option<String> = wasm.get(i).map(|e| e.url.clone());
                            let removed_slug: Option<String> = wasm
                                .get(i)
                                .filter(|e| e.bundled)
                                .and_then(|e| {
                                    crate::bundled_plugins::slug_from_url(&e.url)
                                        .map(str::to_string)
                                });
                            if i < wasm.len() {
                                wasm.remove(i);
                            }
                            let new_wasm = wasm.clone();
                            drop(wasm);
                            if let Some(slug) = removed_slug {
                                let mut rb = removed_bundled.write();
                                if !rb.contains(&slug) {
                                    rb.push(slug);
                                }
                            }
                            // Delegate canonical persistence (wasm_plugins +
                            // bundled tombstone) to the same helper the MCP
                            // `/host/plugins/remove` endpoint uses.
                            if let Some(url) = removed_url {
                                spawn(async move {
                                    if let Some(storage) = crate::STORAGE.get() {
                                        if let Err(e) =
                                            crate::plugin_admin::remove_wasm_plugin_with_storage(
                                                storage, &url,
                                            )
                                            .await
                                        {
                                            tracing::warn!("Failed to remove plugin: {e}");
                                        }
                                    }
                                });
                            }
                            // Reconcile signup entries IMMEDIATELY against the
                            // new wasm_plugins state so the favorites "+"
                            // picker reflects the removal without a restart.
                            let new_wasm_for_sync = new_wasm.clone();
                            client_manager.batch(move |cm| {
                                let s = crate::storage::AppSettings {
                                    wasm_plugins: new_wasm_for_sync,
                                    ..crate::storage::AppSettings::default()
                                };
                                crate::bundled_plugins::sync_bundled_signup_entries(cm, &s);
                            });
                        },
                    }
                }
            }

            AddWasmPlugin {
                on_add: move |entry: WasmPluginEntry| {
                    // Mirror into the local signal first so the UI updates
                    // synchronously, then delegate persistence to the
                    // canonical `plugin_admin::add_wasm_plugin_with_storage`
                    // helper (same code path the MCP `/host/plugins/add`
                    // endpoint uses — single source of truth).
                    wasm_plugins.write().push(entry.clone());
                    spawn(async move {
                        if let Some(storage) = crate::STORAGE.get() {
                            if let Err(e) = crate::plugin_admin::add_wasm_plugin_with_storage(
                                storage,
                                &entry.url,
                                entry.name.clone(),
                            )
                            .await
                            {
                                tracing::warn!("Failed to add plugin: {e}");
                            }
                        }
                    });
                },
            }
        }
    }
}
