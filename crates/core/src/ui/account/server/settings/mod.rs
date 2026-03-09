//! Per-server settings page.
//!
//! A focused settings view for one server within one account.
//! Scoped to: notifications, per-server profile, general/leave.
//!
//! ## Sections
//! | Section | Component |
//! |---|---|
//! | Notifications | `ServerNotificationsSettings` — per-server notification toggles |
//! | Profile | `ServerProfileSettings` — per-server nickname |
//! | General | `ServerGeneralSettings` — server info + leave server |
//!
//! ## 150-line component rule
//! Every `#[component]` fn body MUST stay under **150 lines** of RSX + logic.

mod general;
mod notifications;
mod overview;
mod profile;

use crate::i18n::t;
use crate::state::AppState;
use crate::ui::account::common::VoiceAccountFooter;
use dioxus::prelude::*;
use general::ServerGeneralSettings;
use notifications::ServerNotificationsSettings;
use overview::ServerOverviewSettings;
use profile::ServerProfileSettings;

const SERVER_SETTINGS_SECTIONS: [(&str, ServerSettingsSection); 4] = [
    ("server-settings-overview", ServerSettingsSection::Overview),
    (
        "server-settings-notifications",
        ServerSettingsSection::Notifications,
    ),
    ("server-settings-profile", ServerSettingsSection::Profile),
    ("server-settings-general", ServerSettingsSection::General),
];

fn matches_server_settings_search(filter: &str, label: &str) -> bool {
    filter.is_empty() || label.to_lowercase().contains(filter)
}

#[rustfmt::skip]
#[component]
fn ServerSettingsSearchBar(search_text: Signal<String>) -> Element {
    let current = search_text.read().clone();

    rsx! {
        div { class: "settings-search-bar",
            input {
                r#type: "text",
                class: "settings-search-input",
                placeholder: "{t(\"settings-search\")}",
                value: "{current}",
                oninput: move |e| search_text.set(e.value()),
            }
            if !current.is_empty() {
                button {
                    class: "settings-search-clear",
                    onclick: move |_| search_text.set(String::new()),
                    "×"
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn ServerSettingsNavigation(
    active_section: ServerSettingsSection,
    search_text: Signal<String>,
    on_select: EventHandler<ServerSettingsSection>,
) -> Element {
    let filter = search_text.read().to_lowercase();

    rsx! {
        nav { class: "settings-nav",
            ServerSettingsSearchBar { search_text }
            for (label_key , section) in SERVER_SETTINGS_SECTIONS {
                {
                    let label = t(label_key);
                    if matches_server_settings_search(&filter, &label) {
                        rsx! {
                            ServerSettingsNavItem {
                                label,
                                active: active_section == section,
                                onclick: move |_| on_select.call(section),
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn ServerSettingsContent(
    section: ServerSettingsSection,
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    server_name: String,
) -> Element {
    rsx! {
        div { class: "settings-content",
            div { class: "settings-header",
                h2 { "{t(\"server-settings-title\")} — {server_name}" }
            }
            match section {
                ServerSettingsSection::Overview => rsx! {
                    ServerOverviewSettings {
                        server_id: server_id.clone(),
                        server_name: server_name.clone(),
                        backend_slug: backend.clone(),
                    }
                },
                ServerSettingsSection::Notifications => rsx! {
                    ServerNotificationsSettings { server_id: server_id.clone(), server_name: server_name.clone() }
                },
                ServerSettingsSection::Profile => rsx! {
                    ServerProfileSettings { server_id: server_id.clone(), server_name: server_name.clone() }
                },
                ServerSettingsSection::General => rsx! {
                    ServerGeneralSettings {
                        server_id,
                        server_name,
                        backend_slug: backend,
                        instance_id,
                        account_id,
                    }
                },
            }
        }
    }
}

/// Which section of server settings is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ServerSettingsSection {
    #[default]
    Overview,
    Notifications,
    Profile,
    General,
}

/// Per-server settings page component.
///
/// Shares the same two-column layout (nav sidebar + content) as `AccountSettingsPage`
/// and `SettingsPage`. Server name shown in the content header.
#[rustfmt::skip]
#[component]
pub fn ServerSettingsPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    let mut section = use_signal(ServerSettingsSection::default);
    let _locale = crate::i18n::use_locale().read().clone();
    let search_text = use_signal(String::new);
    let chat_data: Signal<crate::state::ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();

    // Resolve server name from ChatData, fallback to server_id
    let server_name = chat_data
        .read()
        .servers
        .iter()
        .find(|s| s.id == server_id)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| server_id.clone());

    // Keep nav.selected_server in sync (needed if arrived via context menu)
    let server_id_for_effect = server_id.clone();
    use_effect(move || {
        let sid = server_id_for_effect.clone();
        if app_state.read().nav.selected_server.as_deref() != Some(&sid) {
            // Don't forcibly override — the route handler already sets this.
        }
    });

    rsx! {
        div { class: "channel-list-wrapper",
            nav { class: "settings-nav",
                ServerSettingsNavigation {
                    active_section: section(),
                    search_text,
                    on_select: move |next| section.set(next),
                }
            }
            VoiceAccountFooter {}
        }
        ServerSettingsContent {
            section: section(),
            backend,
            instance_id,
            account_id,
            server_id,
            server_name,
        }
    }
}

/// Navigation item for the server settings sidebar.
#[rustfmt::skip]
#[component]
fn ServerSettingsNavItem(
    label: String,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: if active { "settings-nav-item active" } else { "settings-nav-item" },
            onclick: move |evt| onclick.call(evt),
            "{label}"
        }
    }
}
