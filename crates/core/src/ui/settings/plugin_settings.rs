//! Plugin-provided settings pages.
//!
//! Each active backend can contribute a settings sub-page. This module
//! renders a sub-navigation of available plugin settings and delegates
//! to the appropriate settings component based on the selected backend.
//!
//! ## Translation convention
//! All plugin-facing strings come from the plugin's own FTL bundle, which is
//! merged into the global i18n system at plugin load time. Keys use the
//! `plugin-<id>-*` prefix. The host never hard-codes plugin display strings.
//!
//! ## Settings schema
//! Each plugin exposes a `Vec<SettingDescriptor>` via the WIT `plugin-metadata`
//! interface. The host renders each descriptor using the appropriate widget.
//! For native (non-WASM) backends the schema is declared inline here.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.

use crate::i18n::t;
use dioxus::prelude::*;

/// Backend icon for the sub-navigation.
fn backend_settings_icon(backend: poly_client::BackendType) -> &'static str {
    match backend {
        poly_client::BackendType::Stoat => "🦦",
        poly_client::BackendType::Matrix => "🟩",
        poly_client::BackendType::Discord => "🟣",
        poly_client::BackendType::Teams => "🟦",
        poly_client::BackendType::Demo => "🧪",
        poly_client::BackendType::Poly => "🔷",
    }
}

/// Unique backend types from active sessions, for sub-navigation.
///
/// Deduplicates by `BackendType` so multiple accounts on the same
/// backend don't create duplicate settings entries.
fn unique_backend_types(
    cm: &crate::client_manager::ClientManager,
) -> Vec<(poly_client::BackendType, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for (account_id, session) in &cm.sessions {
        if seen.insert(session.backend) {
            result.push((session.backend, account_id.clone()));
        }
    }
    result
}

/// Sub-navigation item for a plugin settings entry.
#[rustfmt::skip]
#[component]
fn PluginSettingsNavItem(
    icon: String,
    label: String,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: if active { "plugin-settings-nav-item active" } else { "plugin-settings-nav-item" },
            onclick: move |evt| onclick.call(evt),
            span { class: "plugin-settings-nav-icon", "{icon}" }
            span { class: "plugin-settings-nav-label", "{label}" }
        }
    }
}

/// Settings content for the Demo backend.
///
/// Strings come from the plugin's own FTL bundle (prefixed `plugin-demo-`),
/// loaded by the host at startup via the WIT `plugin-metadata` interface.
#[rustfmt::skip]
#[component]
pub(super) fn DemoPluginSettings() -> Element {
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
                            spawn(async move {
                                let was_active = client_manager.read().demo_active;
                                crate::ui::favorites_sidebar::toggle_demo(
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

/// Placeholder settings for backends without custom settings pages.
#[rustfmt::skip]
#[component]
fn GenericPluginSettings(backend_type: poly_client::BackendType) -> Element {
    let name = backend_type.display_name();
    let icon = backend_settings_icon(backend_type);
    rsx! {
        div { class: "settings-section plugin-section",
            div { class: "plugin-section-header",
                span { class: "plugin-section-icon", "{icon}" }
                h2 { class: "plugin-section-title", "{name}" }
                span { class: "plugin-section-badge", "{t(\"settings-plugins-badge\")}" }
            }
            p { class: "settings-section-description",
                "{t(\"plugin-settings-generic-description\")}"
            }
        }
    }
}

/// Plugin settings page — sub-navigation + content.
///
/// Lists each unique active backend type. Clicking one shows that
/// backend's settings. Demo backend gets the toggle; others get
/// a placeholder until they implement custom settings.
#[rustfmt::skip]
#[component]
pub fn PluginSettingsPage() -> Element {
    let client_manager: Signal<crate::client_manager::ClientManager> = use_context();
    let cm = client_manager.read();
    let backends = unique_backend_types(&cm);

    // Selected backend — default to first available
    let mut selected = use_signal(|| {
        backends.first().map(|(bt, _)| *bt)
    });

    // If the backend list changed and our selection is gone, reset
    let sel = *selected.read();
    if sel.is_some() && !backends.iter().any(|(bt, _)| Some(*bt) == sel) {
        selected.set(backends.first().map(|(bt, _)| *bt));
    }

    rsx! {
        div { class: "plugin-settings-page",
            // Sub-navigation
            nav { class: "plugin-settings-nav",
                h3 { class: "plugin-settings-nav-title",
                    "{t(\"plugin-settings-nav-title\")}"
                }
                if backends.is_empty() {
                    p { class: "plugin-settings-empty",
                        "{t(\"plugin-settings-none\")}"
                    }
                }
                for (backend_type , _account_id) in &backends {
                    {
                        let bt = *backend_type;
                        // Use plugin's own FTL for display name if available,
                        // fall back to display_name() from poly-client
                        let label_key = format!("plugin-{}-title", bt.display_name().to_lowercase());
                        let raw = t(&label_key);
                        let label = if raw == label_key {
                            bt.display_name().to_string()
                        } else {
                            raw
                        };
                        let icon = backend_settings_icon(bt).to_string();
                        let is_active = sel == Some(bt);
                        rsx! {
                            PluginSettingsNavItem {
                                key: "{bt:?}",
                                icon,
                                label,
                                active: is_active,
                                onclick: move |_| selected.set(Some(bt)),
                            }
                        }
                    }
                }
            }
            // Content
            div { class: "plugin-settings-content",
                match sel {
                    Some(poly_client::BackendType::Demo) => rsx! {
                        DemoPluginSettings {}
                    },
                    Some(bt) => rsx! {
                        GenericPluginSettings { backend_type: bt }
                    },
                    None => rsx! {
                        div { class: "plugin-settings-empty-content",
                            "{t(\"plugin-settings-none\")}"
                        }
                    },
                }
            }
        }
    }
}
