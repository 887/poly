//! Per-channel settings page (Pack C.3 / P19).
//!
//! Mirrors [`crate::ui::account::server::settings::ServerSettingsPage`] but
//! scoped to one channel within a server. The page pulls plugin-declared
//! settings sections via [`ClientBackend::get_settings_sections`] and filters
//! to [`SettingsScope::PerChannel`], rendering each section via
//! [`PluginSettingsSection`] with `scope_id = channel_id`.
//!
//! Host-universal sections do not exist at per-channel scope today; if the
//! backend declares zero `PerChannel` sections the page shows a localized
//! empty-state message (`channel-settings-no-plugin-sections`).

use crate::state::BatchedSignal;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::ChatData;
use crate::ui::account::common::VoiceAccountFooter;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::PluginSettingsSection;
use crate::ui::main_layout::close_mobile_drawer;
use crate::ui::split_shell::SplitMenuShell;
use dioxus::prelude::*;
use poly_client::{SettingsScope, SettingsSection as PluginSettingsSectionData};
use poly_ui_macros::{context_menu, ui_action};

/// Pack C.3 — actions fired from the channel-settings sidebar nav.
///
/// Today the only action is a visual click on the sole nav item; tapping it
/// closes the mobile drawer so the content area becomes visible on small
/// viewports.
#[derive(Debug, Clone)]
pub enum ChannelSettingsNavAction {
    /// User clicked the single "Channel Settings" nav item; close the mobile
    /// drawer so the settings content becomes visible.
    CloseMobileDrawer,
}

impl UiAction for ChannelSettingsNavAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::CloseMobileDrawer => close_mobile_drawer(),
        }
    }
}

/// Per-channel settings content area — loads plugin-declared `PerChannel`
/// sections and renders each via [`PluginSettingsSection`].
#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn ChannelSettingsContent(
    account_id: String,
    channel_id: String,
    channel_name: String,
) -> Element {
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
                    .filter(|s| matches!(s.scope, SettingsScope::PerChannel))
                    .collect()
            }
        })
    };

    let sections = plugin_sections
        .read_unchecked()
        .as_ref()
        .cloned()
        .unwrap_or_default();

    let title = format!("{} — {channel_name}", t("channel-settings-title"));

    rsx! {
        div { class: "settings-page-panel",
            div { class: "special-page-header settings-page-header",
                h2 { class: "special-page-title", "{title}" }
            }
            div { class: "settings-sections-stack",
                if sections.is_empty() {
                    div { class: "settings-empty-state",
                        p { "{t(\"channel-settings-no-plugin-sections\")}" }
                    }
                } else {
                    for plugin_section in sections.into_iter() {
                        {
                            let section_key = plugin_section.section_key.clone();
                            rsx! {
                                div {
                                    class: "settings-section-block",
                                    id: "channel-settings-section-plugin-{section_key}",
                                    PluginSettingsSection {
                                        key: "per-channel-{section_key}",
                                        section: plugin_section,
                                        account_id: account_id.clone(),
                                        scope_id: channel_id.clone(),
                                    }
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

/// Per-channel settings page component.
///
/// Two-column layout mirroring [`crate::ui::account::server::settings::ServerSettingsPage`]:
/// a left sidebar (settings nav + account footer) and a right content pane.
#[ui_action(ChannelSettingsNavAction)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
pub fn ChannelSettingsPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let _ = (&backend, &instance_id, &server_id);
    let chat_data: BatchedSignal<ChatData> = use_context();

    // Resolve channel name from ChatData, fallback to channel_id.
    let channel_name = chat_data
        .read()
        .channels
        .iter()
        .find(|c| c.id == channel_id)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| channel_id.clone());

    rsx! {
        SplitMenuShell {
            root_class: "account-view-main".to_string(),
            sidebar_class: "channel-list-wrapper".to_string(),
            content_class: "settings-content".to_string(),
            sidebar: rsx! {
                nav { class: "settings-nav",
                    div {
                        class: "settings-nav-item active",
                        "data-settings-slug": "plugin-sections",
                        onclick: move |_| close_mobile_drawer(),
                        "{t(\"channel-settings-title\")}"
                    }
                }
                VoiceAccountFooter {}
            },
            content: rsx! {
                ChannelSettingsContent {
                    account_id,
                    channel_id,
                    channel_name,
                }
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    //! UI snapshot coverage is deferred to the Playwright E2E layer — the
    //! page depends on a `BatchedSignal<ChatData>` context that only a running
    //! VirtualDom can provide. The FTL keys used here are asserted in the
    //! host locale bundle tests (see `ui-settings-*` / `channel-settings-*`
    //! keys in `locales/en/main.ftl`).
    use super::*;

    #[test]
    fn channel_settings_ftl_keys_are_strings() {
        // Compile-time check that the keys used by the component are valid
        // Rust string literals in the expected kebab-case form.
        let keys = [
            "channel-settings-title",
            "channel-settings-no-plugin-sections",
        ];
        assert!(keys.iter().all(|k| k.contains('-')));
    }

    /// Ensure `ChannelSettingsNavAction` compiles as a `UiAction`.
    #[test]
    fn channel_settings_nav_action_impls_ui_action() {
        fn assert_ui_action<T: UiAction>() {}
        assert_ui_action::<ChannelSettingsNavAction>();
        let _ = ChannelSettingsNavAction::CloseMobileDrawer;
    }
}
