//! Plugin-provided settings pages.
//!
//! Each active backend **registers** its settings page at runtime via
//! [`crate::client_manager::ClientManager::register_plugin_settings`] when it
//! activates, and unregisters it when it deactivates. The settings host has no
//! compile-time knowledge of any specific plugin.
//!
//! ## What lives here
//! Dioxus component implementations for **native** (non-WASM) built-in backends.
//! Currently that is just the Demo backend. WASM plugins render their settings
//! through schema-driven widgets hosted by the plugin-host crate.
//!
//! ## Translation convention
//! Plugin strings come from the plugin's own FTL bundle (loaded at startup via
//! [`crate::i18n::init`]). Keys use the `plugin-<id>-*` prefix.

use crate::i18n::t;
use dioxus::prelude::*;

/// Settings content for the Demo backend.
///
/// Registered dynamically by [`toggle_demo`] — the settings host never
/// knows about the Demo plugin at compile time.
///
/// Strings come from the plugin's own FTL bundle (prefixed `plugin-demo-`),
/// registered by [`crate::i18n::init`] at startup.
///
/// [`toggle_demo`]: crate::ui::demo::toggle_demo
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
                            // `spawn_forever` runs in `ScopeId::ROOT` so it is never
                            // cancelled when `DemoPluginSettings` unmounts (which happens
                            // the moment toggle_demo calls unregister_plugin_settings and
                            // SettingsAllSections re-renders without the demo entry).
                            // A plain `spawn` here creates a task owned by this component's
                            // scope; dropping that scope mid-await causes the
                            // "RefCell already borrowed" panic at dioxus-core/diff/node.rs.
                            // `spawn_forever` is not in `dioxus::prelude` but is available
                            // via `dioxus::core` (= `dioxus_core`).
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
/// [`crate::client_manager::PluginSettingsEntry`] by [`toggle_demo`] when the
/// demo backend activates. Using an explicit wrapper guarantees the stored
/// value is always `fn() -> Element`, independent of how `#[component]`
/// transforms the component's inner signature.
///
/// [`toggle_demo`]: crate::ui::demo::toggle_demo
pub fn demo_settings_render_fn() -> Element {
    rsx! {
        DemoPluginSettings {}
    }
}
