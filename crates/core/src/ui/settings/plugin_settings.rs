//! Plugin-provided settings pages.
//!
//! Each compiled-in backend **registers** its settings page unconditionally at
//! app startup via
//! [`crate::client_manager::ClientManager::register_plugin_settings`]. The
//! settings host has no compile-time knowledge of any specific plugin.
//!
//! ## What lives here
//! Dioxus component implementations for **native** (non-WASM) built-in backends.
//! WASM plugins render their settings through schema-driven widgets hosted by
//! the plugin-host crate.
//!
//! ## Translation convention
//! Plugin strings come from the plugin's own FTL bundle (loaded at startup via
//! [`crate::i18n::init`]). Keys use the `plugin-<id>-*` prefix.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// Settings content for the Demo backend.
///
/// Registered unconditionally at app startup so the page is always visible
/// in the Plugin Settings nav, even when demo data is disabled. Toggling the
/// checkbox enables or disables demo accounts/servers without unmounting this
/// component or removing it from the nav.
///
/// Strings come from the plugin's own FTL bundle (prefixed `plugin-demo-`),
/// registered by [`crate::i18n::init`] at startup.
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn DemoPluginSettings() -> Element {
    let mut app_state: Signal<crate::state::AppState> = use_context();
    let client_manager: Signal<crate::client_manager::ClientManager> = use_context();
    let chat_data: Signal<crate::state::ChatData> = use_context();
    let demo_active = client_manager.read().demo_active;

    rsx! {
        div { class: "settings-section plugin-section",
            // Plugin-sourced heading — uses the plugin's own FTL key
            div { class: "plugin-section-header",
                span { class: "plugin-section-icon", "🧪" }
                h2 { class: "plugin-section-title",
                    "{t(\"plugin-demo-title\")}"
                }
                span { class: "plugin-section-badge", "{t(\"settings-plugins-badge\")}" }
            }
            p { class: "settings-section-description",
                "{t(\"plugin-demo-description\")}"
            }
            div { class: "settings-toggle-row",
                div { class: "settings-toggle-label-group",
                    label { class: "settings-toggle-label",
                        "{t(\"plugin-demo-setting-enabled-label\")}"
                    }
                    p { class: "settings-toggle-desc",
                        "{t(\"plugin-demo-setting-enabled-desc\")}"
                    }
                }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: demo_active,
                        onchange: move |_| {
                            // `spawn_forever` runs in `ScopeId::ROOT` so it survives
                            // any component re-renders triggered by toggle_demo. A plain
                            // `spawn` would tie the task to this component's scope; if
                            // Dioxus ever reorders scopes during the demo transition,
                            // the "RefCell already borrowed" panic could reappear.
                            let was_active = client_manager.read().demo_active;
                            dioxus::core::spawn_forever(async move {
                                crate::ui::demo::toggle_demo(
                                    client_manager, chat_data, app_state,
                                ).await;
                                if !was_active {
                                    app_state.write().is_setup_complete = true;
                                }
                            });
                        },
                    }
                    span { class: "toggle-slider" }
                }
            }
        }
    }
}

/// Plain `fn() -> Element` wrapper for [`DemoPluginSettings`].
///
/// Stored as the `render` field of a
/// [`crate::client_manager::PluginSettingsEntry`] by
/// [`crate::ui::mod::register_native_plugin_settings`] at app startup.
/// Using an explicit wrapper guarantees the stored value is always
/// `fn() -> Element`, independent of how `#[component]` transforms the
/// component's inner signature.
pub fn demo_settings_render_fn() -> Element {
    rsx! {
        DemoPluginSettings {}
    }
}

// ── Stoat plugin settings ────────────────────────────────────────────────────

/// Settings content for the Stoat backend.
#[cfg(feature = "stoat")]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn StoatPluginSettings() -> Element {
    rsx! {
        div { class: "settings-section plugin-section",
            div { class: "plugin-section-header",
                span { class: "plugin-section-icon", "🦦" }
                h2 { class: "plugin-section-title", "{t(\"plugin-stoat-title\")}" }
                span { class: "plugin-section-badge", "{t(\"settings-plugins-badge\")}" }
            }
            p { class: "settings-section-description",
                "{t(\"plugin-stoat-settings-description\")}"
            }
        }
    }
}

/// Plain `fn() -> Element` wrapper for [`StoatPluginSettings`].
#[cfg(feature = "stoat")]
pub fn stoat_settings_render_fn() -> Element {
    rsx! {
        StoatPluginSettings {}
    }
}

// ── Poly Server plugin settings ───────────────────────────────────────────────

/// Settings content for the Poly Server backend.
///
/// Registered unconditionally at app startup (when compiled with `feature =
/// "server"`) so the page is always reachable in the Plugin Settings nav.
///
/// Strings come from the `server-client` plugin's own FTL bundle
/// (prefixed `plugin-poly-`), registered by [`crate::i18n::init`] at startup.
#[cfg(feature = "server")]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn PolyServerPluginSettings() -> Element {
    // Read the stored setting. If storage is not yet initialised, default to true.
    let use_ws = use_signal(|| {
        if let Some(s) = crate::STORAGE.get() {
            // Storage reads are async; we prime from the known default and let
            // the user's saved value be applied on next app launch. For the
            // settings toggle, read from the signal only — no blocking call.
            let _ = s; // force use so cfg(test) hygiene is satisfied
        }
        true // default: WebSocket on
    });
    // Load the persisted value asynchronously and update the signal once ready.
    let mut use_ws_sig = use_ws;
    use_future(move || async move {
        if let Some(s) = crate::STORAGE.get()
            && let Ok(settings) = s.get_app_settings().await
        {
            use_ws_sig.set(settings.poly_use_websocket);
        }
    });

    let ws_checked = *use_ws.read();

    rsx! {
        div { class: "settings-section plugin-section",
            div { class: "plugin-section-header",
                span { class: "plugin-section-icon", "🔷" }
                h2 { class: "plugin-section-title",
                    "{t(\"plugin-poly-title\")}"
                }
                span { class: "plugin-section-badge", "{t(\"settings-plugins-badge\")}" }
            }
            p { class: "settings-section-description",
                "{t(\"plugin-poly-settings-description\")}"
            }
            div { class: "settings-toggle-row",
                div { class: "settings-toggle-label-group",
                    label { class: "settings-toggle-label",
                        "{t(\"plugin-poly-setting-websocket-label\")}"
                    }
                    p { class: "settings-toggle-desc",
                        "{t(\"plugin-poly-setting-websocket-desc\")}"
                    }
                }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: ws_checked,
                        onchange: move |_| {
                            let new_val = !*use_ws_sig.read();
                            use_ws_sig.set(new_val);
                            dioxus::core::spawn_forever(async move {
                                if let Some(s) = crate::STORAGE.get() {
                                    match s.get_app_settings().await {
                                        Ok(mut settings) => {
                                            settings.poly_use_websocket = new_val;
                                            if let Err(e) = s.set_app_settings(&settings).await {
                                                tracing::warn!(
                                                    "Failed to persist poly_use_websocket: {e}"
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "Failed to read app settings for poly ws toggle: {e}"
                                            );
                                        }
                                    }
                                }
                            });
                        },
                    }
                    span { class: "toggle-slider" }
                }
            }
        }
    }
}

/// Plain `fn() -> Element` wrapper for [`PolyServerPluginSettings`].
///
/// Stored as the `render` field of a
/// [`crate::client_manager::PluginSettingsEntry`] at app startup.
#[cfg(feature = "server")]
pub fn poly_settings_render_fn() -> Element {
    rsx! {
        PolyServerPluginSettings {}
    }
}

// ── Hacker News plugin settings ───────────────────────────────────────────────

#[context_menu(None)]
#[cfg(feature = "hackernews")]
#[component]
pub fn HackerNewsPluginSettings() -> Element {
    rsx! {
        div { class: "plugin-settings-section",
            h3 { class: "plugin-settings-title", "Hacker News" }
            p { class: "plugin-settings-desc",
                "Read-only Hacker News client. Browse top stories, Ask HN, Show HN, and job posts. No write access."
            }
        }
    }
}

#[cfg(feature = "hackernews")]
pub fn hackernews_settings_render_fn() -> Element {
    rsx! {
        HackerNewsPluginSettings {}
    }
}

// ── Lemmy plugin settings ─────────────────────────────────────────────────────

#[context_menu(None)]
#[cfg(feature = "lemmy")]
#[component]
pub fn LemmyPluginSettings() -> Element {
    rsx! {
        div { class: "plugin-settings-section",
            h3 { class: "plugin-settings-title", "Lemmy" }
            p { class: "plugin-settings-desc",
                "Federated link aggregator. Connect to any Lemmy instance with your credentials."
            }
        }
    }
}

#[cfg(feature = "lemmy")]
pub fn lemmy_settings_render_fn() -> Element {
    rsx! {
        LemmyPluginSettings {}
    }
}

// ── Discord plugin settings ───────────────────────────────────────────────────

/// Settings content for the Discord backend.
///
/// Discord is a dev-only plugin: compiled into the repo but not shipped in
/// release builds. Shown in the settings nav so developers can see the
/// declared manifest alongside the other native backends.
#[cfg(feature = "discord")]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn DiscordPluginSettings() -> Element {
    use poly_client::ClientBackend as _;
    let client = poly_discord::DiscordClient::new();
    let manifest = client.plugin_manifest();
    rsx! {
        div { class: "settings-section plugin-section",
            div { class: "plugin-section-header",
                span { class: "plugin-section-icon", "🟣" }
                h2 { class: "plugin-section-title", "Discord (dev)" }
                span { class: "plugin-section-badge", "{t(\"settings-plugins-badge\")}" }
            }
            p { class: "settings-section-description",
                "Popular gaming and community chat platform. Dev-only — not shipped in release builds."
            }
            PluginManifestPanel { manifest }
        }
    }
}

#[cfg(feature = "discord")]
pub fn discord_settings_render_fn() -> Element {
    rsx! {
        DiscordPluginSettings {}
    }
}

// ── Teams plugin settings ─────────────────────────────────────────────────────

/// Settings content for the Microsoft Teams backend.
#[cfg(feature = "teams")]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn TeamsPluginSettings() -> Element {
    use poly_client::ClientBackend as _;
    let client = poly_teams::TeamsClient::new();
    let manifest = client.plugin_manifest();
    rsx! {
        div { class: "settings-section plugin-section",
            div { class: "plugin-section-header",
                span { class: "plugin-section-icon", "🟦" }
                h2 { class: "plugin-section-title", "Microsoft Teams (dev)" }
                span { class: "plugin-section-badge", "{t(\"settings-plugins-badge\")}" }
            }
            p { class: "settings-section-description",
                "Enterprise communication platform by Microsoft. Dev-only — not shipped in release builds."
            }
            PluginManifestPanel { manifest }
        }
    }
}

#[cfg(feature = "teams")]
pub fn teams_settings_render_fn() -> Element {
    rsx! {
        TeamsPluginSettings {}
    }
}

// ── GitHub plugin settings ────────────────────────────────────────────────────

#[context_menu(None)]
#[cfg(feature = "github")]
#[component]
pub fn GitHubPluginSettings() -> Element {
    use poly_client::ClientBackend as _;
    let client = poly_github::GitHubClient::dotcom();
    let manifest = client.plugin_manifest();
    rsx! {
        div { class: "settings-section plugin-section",
            div { class: "plugin-section-header",
                span { class: "plugin-section-icon", "🐙" }
                h2 { class: "plugin-section-title", "{t(\"plugin-github-title\")}" }
                span { class: "plugin-section-badge", "{t(\"settings-plugins-badge\")}" }
            }
            p { class: "settings-section-description",
                "Read-only GitHub / GHE client. Browses repos, issues, pull requests, and source code through your local gh CLI."
            }
            PluginManifestPanel { manifest }
        }
    }
}

#[cfg(feature = "github")]
pub fn github_settings_render_fn() -> Element {
    rsx! {
        GitHubPluginSettings {}
    }
}

// ── Forgejo plugin settings ───────────────────────────────────────────────────

#[context_menu(None)]
#[cfg(feature = "forgejo")]
#[component]
pub fn ForgejoPluginSettings() -> Element {
    use poly_client::ClientBackend as _;
    let client = poly_forgejo::ForgejoClient::codeberg();
    let manifest = client.plugin_manifest();
    let t = crate::i18n::t;
    rsx! {
        div { class: "settings-section plugin-section",
            div { class: "plugin-section-header",
                span { class: "plugin-section-icon", "🦊" }
                h2 { class: "plugin-section-title", "{t(\"plugin-forgejo-title\")}" }
                span { class: "plugin-section-badge", "{t(\"settings-plugins-badge\")}" }
            }
            p { class: "settings-section-description",
                "Forge backend for Forgejo, Gitea, and Codeberg instances. Browse repos, issues, pull requests, and source code via the Forgejo REST API."
            }
            PluginManifestPanel { manifest }
        }
    }
}

#[cfg(feature = "forgejo")]
pub fn forgejo_settings_render_fn() -> Element {
    rsx! {
        ForgejoPluginSettings {}
    }
}

/// Render a plugin's declared manifest (informational only — not enforced).
///
/// Lists the external programs the plugin claims it may invoke and the HTTP
/// hosts it claims it may contact, plus the plugin's homepage. The manifest
#[context_menu(None)]
/// is purely for transparency: the host does NOT sandbox or block based on it.
#[component]
pub fn PluginManifestPanel(manifest: poly_client::PluginManifest) -> Element {
    let exec_list = manifest.exec_programs.join(", ");
    let host_list = if manifest.http_hosts.is_empty() {
        "(none)".to_string()
    } else {
        manifest.http_hosts.join(", ")
    };
    let exec_display = if manifest.exec_programs.is_empty() {
        "(none)".to_string()
    } else {
        exec_list
    };
    rsx! {
        div { class: "plugin-manifest-panel",
            h3 { class: "plugin-manifest-title", "Plugin manifest" }
            p { class: "plugin-manifest-note",
                "Declarative — these values describe what the plugin says it does. The host does not enforce them."
            }
            p { class: "plugin-manifest-row",
                strong { "Description: " }
                "{manifest.description}"
            }
            p { class: "plugin-manifest-row",
                strong { "External programs: " }
                code { "{exec_display}" }
            }
            p { class: "plugin-manifest-row",
                strong { "HTTP hosts: " }
                code { "{host_list}" }
            }
            if let Some(home) = manifest.homepage {
                p { class: "plugin-manifest-row",
                    strong { "Homepage: " }
                    a { href: "{home}", target: "_blank", rel: "noopener", "{home}" }
                }
            }
        }
    }
}
