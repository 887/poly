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
mod profile;

use crate::i18n::t;
use crate::state::AppState;
use dioxus::prelude::*;
use general::ServerGeneralSettings;
use notifications::ServerNotificationsSettings;
use profile::ServerProfileSettings;

/// Which section of server settings is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ServerSettingsSection {
    #[default]
    Notifications,
    Profile,
    General,
}

/// Per-server settings page component.
///
/// Shares the same two-column layout (nav sidebar + content) as `AccountSettingsPage`
/// and `SettingsPage`. Server name shown in the content header.
#[component]
pub fn ServerSettingsPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    let mut section = use_signal(ServerSettingsSection::default);
    let _locale = crate::i18n::use_locale().read().clone();
    let mut search_text = use_signal(String::new);
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

    let sf_raw = search_text.read().clone();
    let sf = sf_raw.to_lowercase();
    let shows = move |label: &str| -> bool { sf.is_empty() || label.to_lowercase().contains(&sf) };

    let notif_label = t("server-settings-notifications");
    let profile_label = t("server-settings-profile");
    let general_label = t("server-settings-general");

    // Keep nav.selected_server in sync (needed if arrived via context menu)
    let server_id_for_effect = server_id.clone();
    use_effect(move || {
        let sid = server_id_for_effect.clone();
        if app_state.read().nav.selected_server.as_deref() != Some(&sid) {
            // Don't forcibly override — the route handler already sets this.
        }
    });

    rsx! {
        div { class: "settings-page",
            nav { class: "settings-nav",
                // Search bar
                div { class: "settings-search-bar",
                    input {
                        r#type: "text",
                        class: "settings-search-input",
                        placeholder: "{t(\"settings-search\")}",
                        value: "{sf_raw}",
                        oninput: move |e| search_text.set(e.value()),
                    }
                    if !sf_raw.is_empty() {
                        button {
                            class: "settings-search-clear",
                            onclick: move |_| search_text.set(String::new()),
                            "×"
                        }
                    }
                }
                // Navigation items
                if shows(&notif_label) {
                    ServerSettingsNavItem {
                        label: notif_label.clone(),
                        active: section() == ServerSettingsSection::Notifications,
                        onclick: move |_| section.set(ServerSettingsSection::Notifications),
                    }
                }
                if shows(&profile_label) {
                    ServerSettingsNavItem {
                        label: profile_label.clone(),
                        active: section() == ServerSettingsSection::Profile,
                        onclick: move |_| section.set(ServerSettingsSection::Profile),
                    }
                }
                if shows(&general_label) {
                    ServerSettingsNavItem {
                        label: general_label.clone(),
                        active: section() == ServerSettingsSection::General,
                        onclick: move |_| section.set(ServerSettingsSection::General),
                    }
                }
            }

            // Content area
            div { class: "settings-content",
                div { class: "settings-header",
                    h2 { "{t(\"server-settings-title\")} — {server_name}" }
                }
                match section() {
                    ServerSettingsSection::Notifications => rsx! {
                        ServerNotificationsSettings { server_id: server_id.clone(), server_name: server_name.clone() }
                    },
                    ServerSettingsSection::Profile => rsx! {
                        ServerProfileSettings { server_id: server_id.clone(), server_name: server_name.clone() }
                    },
                    ServerSettingsSection::General => rsx! {
                        ServerGeneralSettings {
                            server_id: server_id.clone(),
                            server_name: server_name.clone(),
                            backend_slug: backend.clone(),
                            instance_id: instance_id.clone(),
                            account_id: account_id.clone(),
                        }
                    },
                }
            }
        }
    }
}

/// Navigation item for the server settings sidebar.
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
