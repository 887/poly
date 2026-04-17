//! Plugin manager settings page.
//!
//! Plugins in Poly are **messenger backends** — each backend type (Demo,
//! Discord, Matrix, Stoat, Teams, Poly Server) is a plugin. Native backends
//! are compiled-in by feature flag; WASM plugins can be loaded from URLs.
//!
//! This page lets the user:
//! - Toggle native backends on / off with checkboxes
//! - Add WASM plugins from URLs (the app appends `?wit=<version>`)
//! - Toggle WASM plugins on / off
//! - Remove WASM plugins
//!
//! Accounts are *sessions created by a logged-in plugin* — they live in the
//! Accounts settings page, not here.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.

use crate::i18n::t;
use crate::storage::WasmPluginEntry;
use dioxus::prelude::*;
use poly_client::{
    BackendCapabilities, ContainerLabelForm, capabilities_for_slug, container_label_key,
};
use poly_ui_macros::context_menu;

/// WIT version string appended to WASM plugin fetch URLs.
const WIT_VERSION: &str = "0.1.0";

/// Native backend types compiled into the **shipping** build.
///
/// Lists every "built-in" backend; `available` is `false` when the backend
/// was not compiled in (feature flag absent). An unavailable backend shows
/// greyed-out so the user knows it exists but is not in this build.
///
/// Discord and Microsoft Teams deliberately do **not** appear here even
/// when their cargo features are enabled — they are surfaced in
/// [`DEV_INJECTED_BACKENDS`] and rendered inside the WASM-plugins section
/// to make it explicit they are not first-party Poly backends. They are
/// only compiled in dev builds (`apps/web` `dev-plugins` feature) and are
/// stripped from production binaries entirely.
const NATIVE_BACKENDS: &[NativeBackend] = &[
    NativeBackend {
        slug: "demo",
        icon: "🧪",
        name: "Messenger Demo",
        description: "Built-in mock data for exploring Poly without real accounts.",
        available: true,
    },
    NativeBackend {
        slug: "demo_forum",
        icon: "🧪",
        name: "Forum Demo",
        description: "Built-in mock forum data for exploring the Lemmy-style forum UI.",
        available: true,
    },
    NativeBackend {
        slug: "stoat",
        icon: "🦦",
        name: "Stoat (Revolt)",
        description: "Open-source alternative to Discord. Self-hosted or revolt.chat.",
        available: cfg!(feature = "stoat"),
    },
    NativeBackend {
        slug: "matrix",
        icon: "🟩",
        name: "Matrix",
        description: "Federated, end-to-end encrypted messaging protocol.",
        available: cfg!(feature = "matrix"),
    },
    NativeBackend {
        slug: "poly",
        icon: "🔷",
        name: "Poly Server",
        description: "Self-hosted Poly backup / sync server with E2E encryption.",
        available: cfg!(feature = "server"),
    },
    NativeBackend {
        slug: "hackernews",
        icon: "🔶",
        name: "Hacker News",
        description: "Read Hacker News stories and threaded discussions.",
        available: cfg!(feature = "hackernews"),
    },
    NativeBackend {
        slug: "lemmy",
        icon: "🐾",
        name: "Lemmy",
        description: "Federated link aggregator. Connect to any Lemmy instance.",
        available: cfg!(feature = "lemmy"),
    },
    NativeBackend {
        slug: "github",
        icon: "🐙",
        name: "GitHub",
        description: "Browse GitHub / GHE repos via your local gh CLI. Issues, PRs, and code explorer.",
        available: cfg!(feature = "github"),
    },
    NativeBackend {
        slug: "forgejo",
        icon: "🦊",
        name: "Forgejo",
        description: "Browse repos on any Forgejo, Gitea, or Codeberg instance. Issues, PRs, and code explorer.",
        available: cfg!(feature = "forgejo"),
    },
];

/// Dev-only backends that are technically compiled into this binary but are
/// surfaced in the WASM-plugins section instead of "Built-in", because they
/// are not part of any shipping build.
///
/// Discord and Microsoft Teams live here. Their crates only get linked in
/// when `apps/web` is built with the `dev-plugins` feature (default in dev,
/// off in `--features production`). They show up in the UI as if a user
/// loaded them as external WASM plugins so it stays visually obvious they
/// are not first-party Poly backends — and so the production build has
/// nothing Discord/Teams-shaped to point at during app-store review.
///
/// At app startup, [`ensure_dev_plugins_in`] copies these into the persisted
/// [`crate::storage::AppSettings::wasm_plugins`] list using a sentinel URL
/// (`internal://dev-plugin/<slug>`). The rendering code in `PluginsSettings`
/// detects that URL prefix and routes the toggle through the same
/// disabled-native-backends path that the built-in toggles use, so the
/// `(dev)` rows behave exactly like first-party toggles even though they
/// look like user-loaded WASM plugins.
pub(crate) const DEV_INJECTED_BACKENDS: &[NativeBackend] = &[
    #[cfg(feature = "discord")]
    NativeBackend {
        slug: "discord",
        icon: "🟣",
        name: "Discord (dev)",
        description: "Popular gaming and community chat platform. Dev-only — not shipped in release builds.",
        available: true,
    },
    #[cfg(feature = "teams")]
    NativeBackend {
        slug: "teams",
        icon: "🟦",
        name: "Microsoft Teams (dev)",
        description: "Enterprise communication platform by Microsoft. Dev-only — not shipped in release builds.",
        available: true,
    },
];

/// Compile-time backend descriptor (only const-compatible types).
pub(crate) struct NativeBackend {
    pub(crate) slug: &'static str,
    pub(crate) icon: &'static str,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    /// Whether this backend was compiled in (feature flag check).
    pub(crate) available: bool,
}

/// URL scheme used to mark dev-injected entries inside the persisted
/// [`crate::storage::AppSettings::wasm_plugins`] list. Anything with this
/// prefix is rendered as a dev backend (Discord/Teams) instead of a real
/// user-loaded WASM plugin.
const DEV_PLUGIN_URL_PREFIX: &str = "internal://dev-plugin/";

/// Build the sentinel URL for a dev-injected backend.
pub(crate) fn dev_injected_url(slug: &str) -> String {
    format!("{DEV_PLUGIN_URL_PREFIX}{slug}")
}

/// Extract the backend slug from a dev-injected URL, or `None` if the URL
/// is a normal user-loaded plugin.
pub(crate) fn parse_dev_injected_slug(url: &str) -> Option<&str> {
    url.strip_prefix(DEV_PLUGIN_URL_PREFIX)
}

/// Look up a dev backend descriptor by slug.
pub(crate) fn lookup_dev_backend(slug: &str) -> Option<&'static NativeBackend> {
    DEV_INJECTED_BACKENDS.iter().find(|b| b.slug == slug)
}

/// Inject any missing dev-only backends (Discord/Teams) into the persisted
/// `wasm_plugins` list. Returns `true` if `settings` was modified, so the
/// caller knows whether to write back to storage.
///
/// Cleans up stale dev entries whose feature has been turned off (e.g. a
/// production build still seeing a leftover `discord` row from a previous
/// dev session) so the UI never shows a backend whose code is not linked
/// in.
pub fn ensure_dev_plugins_in(settings: &mut crate::storage::AppSettings) -> bool {
    let live_dev_slugs: std::collections::HashSet<&'static str> =
        DEV_INJECTED_BACKENDS.iter().map(|b| b.slug).collect();
    let before = settings.wasm_plugins.len();

    settings.wasm_plugins.retain(|entry| {
        if let Some(slug) = parse_dev_injected_slug(&entry.url) {
            live_dev_slugs.contains(slug)
        } else {
            true
        }
    });

    let mut changed = settings.wasm_plugins.len() != before;

    for backend in DEV_INJECTED_BACKENDS {
        let url = dev_injected_url(backend.slug);
        if !settings.wasm_plugins.iter().any(|e| e.url == url) {
            let enabled = !settings.disabled_native_backends.iter().any(|s| s == backend.slug);
            settings.wasm_plugins.push(WasmPluginEntry {
                url,
                name: Some(backend.name.to_string()),
                enabled,
            });
            changed = true;
        }
    }
    changed
}

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

/// Toggle a non-demo native backend on/off.
///
/// Extracted from `PluginsSettings` so the dev-injected discord/teams rows
/// in the WASM section can reuse the exact same disconnect-and-persist
/// logic without duplicating the giant inline closure.
fn toggle_native_backend(
    toggled: String,
    mut client_manager: Signal<crate::client_manager::ClientManager>,
    mut chat_data: Signal<crate::state::ChatData>,
    mut disabled: Signal<Vec<String>>,
    wasm_plugins: Signal<Vec<WasmPluginEntry>>,
) {
    let is_enabled = !disabled.read().contains(&toggled);
    if is_enabled {
        let bt = poly_client::BackendType::from_slug(&toggled);
        let (removed_ids, handles) =
            client_manager.write().take_accounts_by_backend(bt.clone());
        let backend_slug = bt.slug().to_string();
        disabled.write().push(toggled.clone());
        let new_disabled = disabled.read().clone();
        client_manager
            .write()
            .set_disabled_native_backends(new_disabled.clone());
        let wasm = wasm_plugins.read().clone();
        spawn(async move {
            for h in handles {
                let mut g = h.write().await;
                let _ = g.logout().await;
            }
            if !removed_ids.is_empty() {
                let mut cd = chat_data.write();
                cd.servers
                    .retain(|s| s.backend != bt || !removed_ids.contains(&s.account_id));
                cd.dm_channels
                    .retain(|d| d.backend != bt || !removed_ids.contains(&d.account_id));
                cd.groups
                    .retain(|g| g.backend != bt || !removed_ids.contains(&g.account_id));
                cd.notifications
                    .retain(|n| n.backend != bt || !removed_ids.contains(&n.account_id));
                for id in &removed_ids {
                    cd.friends.remove(id.as_str());
                }
                for id in &removed_ids {
                    cd.account_sessions.remove(id.as_str());
                }
                let live_server_ids: Vec<String> =
                    cd.servers.iter().map(|s| s.id.clone()).collect();
                cd.favorited_server_ids
                    .retain(|fid| live_server_ids.contains(fid));
            }
            let is_self_init =
                backend_slug == "demo_forum" || backend_slug == "hackernews";
            if !is_self_init
                && let Some(storage) = crate::STORAGE.get() {
                    for id in &removed_ids {
                        let _ = storage.remove_account_token(&backend_slug, id).await;
                    }
                }
            let mut settings = load_settings().await;
            settings.disabled_native_backends = new_disabled;
            settings.wasm_plugins = wasm;
            save_settings(&settings).await;
        });
    } else {
        disabled.write().retain(|s| s != &toggled);
        let new_disabled = disabled.read().clone();
        client_manager
            .write()
            .set_disabled_native_backends(new_disabled.clone());
        let wasm = wasm_plugins.read().clone();
        let toggled_re = toggled.clone();
        spawn(async move {
            let mut s = load_settings().await;
            s.disabled_native_backends = new_disabled;
            s.wasm_plugins = wasm;
            save_settings(&s).await;
        });
        if toggled_re == "demo_forum" {
            spawn(async move {
                crate::ui::demo::toggle_demo_forum_on(client_manager, chat_data).await;
            });
        }
        #[cfg(feature = "hackernews")]
        if toggled_re == "hackernews" {
            if let Some(storage) = crate::STORAGE.get() {
                spawn(async move {
                    crate::ui::restore_hackernews_accounts(storage, client_manager, chat_data)
                        .await;
                });
            }
        }
        #[cfg(feature = "github")]
        if toggled_re == "github" {
            if let Some(storage) = crate::STORAGE.get() {
                spawn(async move {
                    crate::ui::restore_github_accounts(storage, client_manager, chat_data).await;
                });
            }
        }
        #[cfg(feature = "forgejo")]
        if toggled_re == "forgejo" {
            if let Some(storage) = crate::STORAGE.get() {
                spawn(async move {
                    crate::ui::restore_forgejo_accounts(storage, client_manager, chat_data).await;
                });
            }
        }
    }
}

/// A single native backend row with toggle checkbox and a click-to-expand
/// capability details panel.
///
/// `badge_class` is "native" for compiled-in backends and "wasm" for
/// dev-injected backends (Discord/Teams) so they render with the correct
/// badge while sharing the same row layout and capability panel.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn NativePluginRow(
    slug: String,
    icon: String,
    name: String,
    description: String,
    available: bool,
    enabled: bool,
    account_count: usize,
    badge_class: String,
    badge_label_key: String,
    on_toggle: EventHandler<String>,
) -> Element {
    let slug_for_toggle = slug.clone();
    let slug_for_panel = slug.clone();
    let mut expanded = use_signal(|| false);
    let is_open = *expanded.read();
    let disclosure_label = if is_open {
        t("plugins-capabilities-hide")
    } else {
        t("plugins-capabilities-show")
    };
    rsx! {
        div {
            class: if available { "plugin-row-wrap" } else { "plugin-row-wrap plugin-row-unavailable" },
            div {
                class: "plugin-row",
                onclick: move |_| {
                    let current = *expanded.read();
                    expanded.set(!current);
                },
                label {
                    class: "plugin-row-toggle",
                    onclick: move |e| { e.stop_propagation(); },
                    input {
                        r#type: "checkbox",
                        checked: enabled,
                        onchange: move |_| on_toggle.call(slug_for_toggle.clone()),
                    }
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
                    span { class: "plugin-type-badge {badge_class}", "{t(badge_label_key.as_str())}" }
                    button {
                        class: "plugin-disclosure-btn",
                        r#type: "button",
                        "aria-expanded": "{is_open}",
                        onclick: move |e| {
                            e.stop_propagation();
                            let current = *expanded.read();
                            expanded.set(!current);
                        },
                        "{disclosure_label}"
                    }
                }
            }
            if is_open {
                PluginCapabilityPanel { slug: slug_for_panel }
            }
        }
    }
}

/// Expandable panel that renders the full capability shape for a plugin slug.
///
/// Consumes [`capabilities_for_slug`] and [`container_label_key`] so it stays
/// in sync with the capability matrix used by route gating, nav buttons, and
/// MCP tool advertisement. Everything here is read-only inspection — no
/// state, no toggles.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn PluginCapabilityPanel(slug: String) -> Element {
    let caps: BackendCapabilities = capabilities_for_slug(&slug);
    let shape_rows = caps.shape_rows();
    let flag_rows = caps.feature_flags();
    let container_singular = t(container_label_key(&slug, ContainerLabelForm::Singular));
    let container_plural = t(container_label_key(&slug, ContainerLabelForm::Plural));
    let layout_key = if caps.is_forum_layout() {
        "plugins-capabilities-layout-forum"
    } else {
        "plugins-capabilities-layout-chat"
    };
    rsx! {
        div { class: "plugin-capability-panel",
            "data-testid": "plugin-capability-panel-{slug}",
            h5 { class: "plugin-capability-heading",
                "{t(\"plugins-capabilities-shape\")}"
            }
            dl { class: "plugin-capability-list",
                for row in shape_rows.iter() {
                    {
                        let label_key = row.label_key;
                        let value_key = row.value_key;
                        rsx! {
                            dt { "{t(label_key)}" }
                            dd { "{t(value_key)}" }
                        }
                    }
                }
                dt { "{t(\"plugins-capabilities-layout\")}" }
                dd { "{t(layout_key)}" }
            }
            h5 { class: "plugin-capability-heading",
                "{t(\"plugins-capabilities-flags\")}"
            }
            ul { class: "plugin-capability-flags",
                for (label_key, supported) in flag_rows.iter() {
                    {
                        let cls = if *supported { "flag on" } else { "flag off" };
                        let state_key = if *supported {
                            "plugins-flag-supported"
                        } else {
                            "plugins-flag-unsupported"
                        };
                        let key = *label_key;
                        rsx! {
                            li { class: "{cls}",
                                span { class: "flag-label", "{t(key)}" }
                                span { class: "flag-state", "{t(state_key)}" }
                            }
                        }
                    }
                }
            }
            h5 { class: "plugin-capability-heading",
                "{t(\"plugins-capabilities-terminology\")}"
            }
            dl { class: "plugin-capability-list",
                dt { "{t(\"plugins-capabilities-container\")}" }
                dd { "{container_singular} / {container_plural}" }
            }
        }
    }
}

/// A single WASM plugin row with toggle and remove buttons.
#[context_menu(inherit)]
#[rustfmt::skip]
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
    rsx! {
        div { class: "plugin-row",
            label { class: "plugin-row-toggle",
                input {
                    r#type: "checkbox",
                    checked: entry.enabled,
                    onchange: move |_| on_toggle.call(idx_toggle),
                }
            }
            div { class: "plugin-row-icon", "🔌" }
            div { class: "plugin-row-info",
                div { class: "plugin-row-name", "{display_name}" }
                div { class: "plugin-row-description", "{entry.url}" }
            }
            div { class: "plugin-row-meta",
                span { class: "plugin-type-badge wasm", "{t(\"plugins-type-wasm\")}" }
                button {
                    class: "btn btn-small btn-danger",
                    onclick: move |_| on_remove.call(idx_remove),
                    "{t(\"plugins-remove\")}"
                }
            }
        }
    }
}

/// URL input form to add a new WASM plugin, or install from a local file.
///
/// Two install modes: from URL (with WIT version appended) or from a local .wasm file.
/// Display name is inferred from the URL hostname or file name — no manual entry needed.
#[context_menu(inherit)]
#[rustfmt::skip]
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
/// Shows all messenger backend plugins (native + WASM) with toggle checkboxes.
/// Native backends are compiled-in; WASM plugins are loaded from user-provided URLs.
///
/// **Accounts** ("Cat (demo)", "Dog (demo)") are sessions created when a plugin
/// authenticates a user — they appear in the Accounts settings page. Here we
/// manage *which plugins are available and enabled*.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn PluginsSettings() -> Element {
    let client_manager: Signal<crate::client_manager::ClientManager> = use_context();
    let chat_data: Signal<crate::state::ChatData> = use_context();
    let app_state: Signal<crate::state::AppState> = use_context();

    // Local reactive copies of the persisted list — updated on every toggle/add/remove.
    let mut disabled: Signal<Vec<String>> = use_signal(Vec::new);
    let mut wasm_plugins: Signal<Vec<WasmPluginEntry>> = use_signal(Vec::new);

    // Load settings from storage once on mount.
    use_future(move || async move {
        let s = load_settings().await;
        disabled.set(s.disabled_native_backends.clone());
        wasm_plugins.set(s.wasm_plugins.clone());
    });

    let disabled_snap = disabled.read().clone();
    let wasm_snap = wasm_plugins.read().clone();

    rsx! {
        div { class: "settings-section",
            h2 { class: "settings-section-title", "{t(\"settings-plugins\")}" }
            p { class: "settings-section-description",
                "{t(\"settings-plugins-description\")}"
            }

            // ── Native backends ────────────────────────────────────────────
            h3 { class: "settings-subsection-title",
                "{t(\"plugins-native-title\")}"
            }
            p { class: "settings-description",
                "{t(\"plugins-native-description\")}"
            }
            div { class: "plugin-list",
                for backend in NATIVE_BACKENDS {
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
                            NativePluginRow {
                                key: "{slug_key}",
                                slug: slug.clone(),
                                icon: backend.icon.to_string(),
                                name: backend.name.to_string(),
                                description: backend.description.to_string(),
                                available: backend.available,
                                enabled,
                                account_count,
                                badge_class: "native".to_string(),
                                badge_label_key: "plugins-type-native".to_string(),
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
                                        toggle_native_backend(
                                            toggled,
                                            client_manager,
                                            chat_data,
                                            disabled,
                                            wasm_plugins,
                                        );
                                    }
                                },
                            }
                        }
                    }
                }
            }

            // ── WASM plugins ───────────────────────────────────────────────
            h3 { class: "settings-subsection-title",
                "{t(\"plugins-wasm-title\")}"
            }
            p { class: "settings-description",
                "{t(\"plugins-wasm-description\")}"
            }
            div { class: "plugin-list",
                if wasm_snap.is_empty() {
                    div { class: "plugin-empty",
                        "{t(\"plugins-none-loaded\")}"
                    }
                }
                for (idx, entry) in wasm_snap.iter().enumerate() {
                    {
                        if let Some(slug) = parse_dev_injected_slug(&entry.url).map(|s| s.to_string()) {
                            let backend = lookup_dev_backend(&slug);
                            let icon = backend.map(|b| b.icon).unwrap_or("🔌").to_string();
                            let name = backend
                                .map(|b| b.name.to_string())
                                .or_else(|| entry.name.clone())
                                .unwrap_or_else(|| slug.clone());
                            let description = backend
                                .map(|b| b.description.to_string())
                                .unwrap_or_default();
                            let enabled = !disabled_snap.contains(&slug);
                            let account_count = client_manager
                                .read()
                                .sessions
                                .values()
                                .filter(|s| s.backend.slug() == slug.as_str())
                                .count();
                            let slug_key = slug.clone();
                            rsx! {
                                NativePluginRow {
                                    key: "{slug_key}",
                                    slug: slug.clone(),
                                    icon,
                                    name,
                                    description,
                                    available: true,
                                    enabled,
                                    account_count,
                                    badge_class: "wasm".to_string(),
                                    badge_label_key: "plugins-type-wasm".to_string(),
                                    on_toggle: move |toggled: String| {
                                        toggle_native_backend(
                                            toggled,
                                            client_manager,
                                            chat_data,
                                            disabled,
                                            wasm_plugins,
                                        );
                                    },
                                }
                            }
                        } else {
                            let entry_clone = entry.clone();
                            rsx! {
                                WasmPluginRow {
                                    key: "{idx}",
                                    index: idx,
                                    entry: entry_clone,
                                    on_toggle: move |i: usize| {
                                        let mut wasm = wasm_plugins.write();
                                        if let Some(p) = wasm.get_mut(i) {
                                            p.enabled = !p.enabled;
                                        }
                                        let new_wasm = wasm.clone();
                                        drop(wasm);
                                        let dis = disabled.read().clone();
                                        spawn(async move {
                                            let mut s = load_settings().await;
                                            s.disabled_native_backends = dis;
                                            s.wasm_plugins = new_wasm;
                                            save_settings(&s).await;
                                        });
                                    },
                                    on_remove: move |i: usize| {
                                        let mut wasm = wasm_plugins.write();
                                        if i < wasm.len() {
                                            wasm.remove(i);
                                        }
                                        let new_wasm = wasm.clone();
                                        drop(wasm);
                                        let dis = disabled.read().clone();
                                        spawn(async move {
                                            let mut s = load_settings().await;
                                            s.disabled_native_backends = dis;
                                            s.wasm_plugins = new_wasm;
                                            save_settings(&s).await;
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }

            AddWasmPlugin {
                on_add: move |entry: WasmPluginEntry| {
                    wasm_plugins.write().push(entry);
                    let new_wasm = wasm_plugins.read().clone();
                    let dis = disabled.read().clone();
                    spawn(async move {
                        let mut s = load_settings().await;
                        s.disabled_native_backends = dis;
                        s.wasm_plugins = new_wasm;
                        save_settings(&s).await;
                    });
                },
            }
        }
    }
}
