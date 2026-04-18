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

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::AppState;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::PluginSettingsSection;
use poly_client::{SettingsScope, SettingsSection as PluginSettingsSectionData};
use poly_ui_macros::{context_menu, ui_action};
use crate::ui::account::common::VoiceAccountFooter;
use crate::ui::main_layout::close_mobile_drawer;
use crate::ui::settings::scroll_spy::scroll_to_settings_section;
#[cfg(target_arch = "wasm32")]
use crate::ui::settings::scroll_spy::{
    SettingsScrollSpyConfig, install_settings_scroll_spy as install_shared_settings_scroll_spy,
};
use crate::ui::split_shell::SplitMenuShell;
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

/// Count matching sections. Returns 0 when filter is empty.
fn server_total_match_count(filter: &str) -> usize {
    if filter.is_empty() {
        return 0;
    }
    SERVER_SETTINGS_SECTIONS
        .iter()
        .filter(|(key, _)| t(key).to_lowercase().contains(filter))
        .count()
}

fn scroll_to_server_section(slug: &str) {
    scroll_to_settings_section("server-settings-section-", slug);
}

fn install_server_settings_scroll_spy(_section: Signal<ServerSettingsSection>) {
    #[cfg(target_arch = "wasm32")]
    {
        let mut section = _section;
        let config = SettingsScrollSpyConfig {
            runtime_flag: "__polyServerSettingsScrollSpyInstalled",
            scroll_root_selectors: vec![
                ".poly-split-content.settings-content > .poly-split-content-stage",
                ".settings-content",
            ],
            section_prefix: "server-settings-section-",
            section_ids: [
                "server-settings-section-overview",
                "server-settings-section-notifications",
                "server-settings-section-profile",
                "server-settings-section-general",
            ]
            .into_iter()
            .map(ToString::to_string)
            .collect(),
            plugin_section_prefix: None,
        };
        install_shared_settings_scroll_spy(config, move |slug| {
            let next = ServerSettingsSection::from_slug(&slug);
            if *section.read() != next {
                section.set(next);
            }
        });
    }
}

pub enum ServerSettingsSearchBarAction {
    SetFilter(String),
    ClearFilter,
}

impl UiAction for ServerSettingsSearchBarAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetFilter(_) => todo!("phase-E: update server settings search filter"),
            Self::ClearFilter => todo!("phase-E: clear server settings search filter"),
        }
    }
}

#[ui_action(ServerSettingsSearchBarAction)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn ServerSettingsSearchBar(search_text: Signal<String>) -> Element {
    let current = search_text.read().clone();
    let total = server_total_match_count(&current.to_lowercase());

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
                span { class: "settings-search-count",
                    "{total} {t(\"settings-search-found\")}"
                }
                button {
                    class: "settings-search-clear",
                    onclick: move |_| search_text.set(String::new()),
                    "×"
                }
            }
        }
    }
}

#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn ServerSettingsContentHeader(search_text: Signal<String>, server_name: String) -> Element {
    rsx! {
        div { class: "special-page-header settings-page-header",
            h2 { class: "special-page-title", "{t(\"server-settings-title\")} — {server_name}" }
            ServerSettingsSearchBar { search_text }
        }
    }
}

#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn ServerSettingsNavigation(
    active_section: ServerSettingsSection,
    search_text: Signal<String>,
    on_select: EventHandler<ServerSettingsSection>,
) -> Element {
    let filter = search_text.read().to_lowercase();

    let searching = !filter.is_empty();

    rsx! {
        nav { class: "settings-nav",
            for (label_key , section) in SERVER_SETTINGS_SECTIONS {
                {
                    let label = t(label_key);
                    let has_match = matches_server_settings_search(&filter, &label);
                    // Always render so scroll spy can find data-settings-slug; hide via CSS
                    let hidden = searching && !has_match;
                    rsx! {
                        ServerSettingsNavItem {
                            label,
                            active: active_section == section,
                            slug: section.to_slug().to_string(),
                            show_match_badge: searching && has_match,
                            hidden,
                            onclick: move |_| on_select.call(section),
                        }
                    }
                }
            }
        }
    }
}

#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn ServerSettingsContent(
    search_text: Signal<String>,
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    server_name: String,
) -> Element {
    let _ = (&backend, &instance_id);
    let filter = search_text.read().to_lowercase();

    // Fetch plugin-declared PerServer settings sections for this account's
    // backend. Empty when the backend declares no per-server sections.
    let plugin_sections = {
        let account_id = account_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            async move {
                let client_manager: Signal<ClientManager> = match try_consume_context() {
                    Some(cm) => cm,
                    None => return Vec::<PluginSettingsSectionData>::new(),
                };
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Vec::new();
                };
                let guard = backend.read().await;
                guard
                    .get_settings_sections()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|s| matches!(s.scope, SettingsScope::PerServer))
                    .collect()
            }
        })
    };

    let plugin_sections_snapshot = plugin_sections
        .read_unchecked()
        .as_ref()
        .cloned()
        .unwrap_or_default();

    rsx! {
        div { class: "settings-page-panel",
            ServerSettingsContentHeader { search_text, server_name: server_name.clone() }
            div { class: "settings-sections-stack",
                for (label_key, section) in SERVER_SETTINGS_SECTIONS {
                    {
                        let label = t(label_key);
                        let has_match = matches_server_settings_search(&filter, &label);
                        let searching = !filter.is_empty();
                        // Always render so scroll spy IDs remain in the DOM; hide via CSS
                        let id = format!("server-settings-section-{}", section.to_slug());
                        let class = if searching && !has_match {
                            "settings-section-block settings-section-hidden"
                        } else {
                            "settings-section-block"
                        };
                        // Plugin-declared PerServer sections render in their own
                        // sibling blocks after "Overview" but before "Notifications".
                        let inject_plugin_sections =
                            matches!(section, ServerSettingsSection::Notifications);
                        rsx! {
                            if inject_plugin_sections {
                                for plugin_section in plugin_sections_snapshot.clone().into_iter() {
                                    {
                                        let section_key = plugin_section.section_key.clone();
                                        rsx! {
                                            div {
                                                class: "settings-section-block",
                                                id: "server-settings-section-plugin-{section_key}",
                                                PluginSettingsSection {
                                                    key: "per-server-{section_key}",
                                                    section: plugin_section,
                                                    account_id: account_id.clone(),
                                                    scope_id: server_id.clone(),
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { id, class,
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
                div { class: "settings-scroll-spacer" }
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

impl ServerSettingsSection {
    fn to_slug(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Notifications => "notifications",
            Self::Profile => "profile",
            Self::General => "general",
        }
    }

    fn from_slug(slug: &str) -> Self {
        match slug {
            "notifications" => Self::Notifications,
            "profile" => Self::Profile,
            "general" => Self::General,
            _ => Self::Overview,
        }
    }
}

/// Per-server settings page component.
///
/// Shares the same two-column layout (nav sidebar + content) as `AccountSettingsPage`
/// and `SettingsPage`. Server name shown in the content header.
#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
pub fn ServerSettingsPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    section: String,
) -> Element {
    let initial_section = section.clone();
    let mut section = use_signal(|| ServerSettingsSection::from_slug(&initial_section));
    let route_section = ServerSettingsSection::from_slug(&initial_section);
    if *section.read() != route_section {
        section.set(route_section);
    }
    let _locale = crate::i18n::use_locale().read().clone();
    let search_text = use_signal(String::new);
    let chat_data: Signal<crate::state::ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let mut published_section = use_signal(String::new);

    #[cfg(target_arch = "wasm32")]
    let backend_for_route = backend.clone();
    #[cfg(target_arch = "wasm32")]
    let instance_id_for_route = instance_id.clone();
    #[cfg(target_arch = "wasm32")]
    let account_id_for_route = account_id.clone();
    #[cfg(target_arch = "wasm32")]
    let server_id_for_route = server_id.clone();

    // Resolve server name from ChatData, fallback to server_id
    let server_name = chat_data
        .read()
        .servers
        .iter()
        .find(|s| s.id == server_id)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| server_id.clone());

    use_effect(move || {
        let slug = section.read().to_slug().to_string();
        scroll_to_server_section(&slug);
        if published_section.read().as_str() != slug {
            published_section.set(slug.clone());
            #[cfg(target_arch = "wasm32")]
            {
                let route_url = format!(
                    "/{}/{}/{}/servers/{}/settings/{}",
                    backend_for_route,
                    instance_id_for_route,
                    account_id_for_route,
                    server_id_for_route,
                    slug,
                );
                let js = format!("history.replaceState({{}}, '', '{}')", route_url);
                let _ = document::eval(&js);
            }
        }
    });

    use_effect(move || {
        install_server_settings_scroll_spy(section);
    });

    // Keep nav.selected_server in sync (needed if arrived via context menu)
    let server_id_for_effect = server_id.clone();
    use_effect(move || {
        let sid = server_id_for_effect.clone();
        if app_state.read().nav.selected_server.as_deref() != Some(&sid) {
            // Don't forcibly override — the route handler already sets this.
        }
    });

    rsx! {
        SplitMenuShell {
            root_class: "account-view-main".to_string(),
            sidebar_class: "channel-list-wrapper".to_string(),
            content_class: "settings-content".to_string(),
            sidebar: rsx! {
                nav { class: "settings-nav",
                    ServerSettingsNavigation {
                        active_section: section(),
                        search_text,
                        on_select: move |next| {
                            section.set(next);
                            close_mobile_drawer();
                        },
                    }
                }
                VoiceAccountFooter {}
            },
            content: rsx! {
                ServerSettingsContent {
                    search_text,
                    backend,
                    instance_id,
                    account_id,
                    server_id,
                    server_name,
                }
            },
        }
    }
}

/// Navigation item for the server settings sidebar.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn ServerSettingsNavItem(
    label: String,
    active: bool,
    slug: String,
    #[props(default = false)] show_match_badge: bool,
    #[props(default = false)] hidden: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    // Hide takes priority over active — if hidden, always hide.
    let class = match (hidden, active) {
        (true, _) => "settings-nav-item settings-nav-item-hidden",
        (_, true) => "settings-nav-item active",
        _ => "settings-nav-item",
    };
    rsx! {
        div {
            class,
            "data-settings-slug": slug,
            onclick: move |evt| onclick.call(evt),
            "{label}"
            if show_match_badge {
                span { class: "settings-nav-match-count", "(1)" }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn server_settings_search_bar_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<ServerSettingsSearchBarAction>();
        let _ = ServerSettingsSearchBarAction::SetFilter("test".to_string());
        let _ = ServerSettingsSearchBarAction::ClearFilter;
    }
}
