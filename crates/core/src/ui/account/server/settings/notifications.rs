//! Per-server notification settings.
//!
//! Mirrors Discord's server notification settings:
//! - All Messages / Only @mentions / Nothing
//! - Suppress @everyone and @here
//! - Suppress All Role @mentions
//! - Suppress Highlights
//! - Mute New Events
//! - Mobile Push Notifications

use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

pub enum ServerNotificationsSettingsAction {
    SetLevel(NotifLevel),
    SetSuppressEveryone(bool),
    SetSuppressRoles(bool),
    SetSuppressHighlights(bool),
    SetMuteEvents(bool),
    SetMobilePush(bool),
}

impl UiAction for ServerNotificationsSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetLevel(_) => todo!("phase-E: update server notification level"),
            Self::SetSuppressEveryone(_) => todo!("phase-E: update suppress_everyone"),
            Self::SetSuppressRoles(_) => todo!("phase-E: update suppress_roles"),
            Self::SetSuppressHighlights(_) => todo!("phase-E: update suppress_highlights"),
            Self::SetMuteEvents(_) => todo!("phase-E: update mute_events"),
            Self::SetMobilePush(_) => todo!("phase-E: update mobile_push"),
        }
    }
}

/// Per-server notification settings panel.
///
/// Notification preferences are currently in-memory only (no storage
/// persistence yet — that is planned for Phase 2.11).
#[ui_action(ServerNotificationsSettingsAction)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
pub fn ServerNotificationsSettings(server_id: String, server_name: String) -> Element {
    let mut notif_level = use_signal(|| NotifLevel::Mentions);
    let mut suppress_everyone = use_signal(|| false);
    let mut suppress_roles = use_signal(|| false);
    let mut suppress_highlights = use_signal(|| false);
    let mut mute_events = use_signal(|| false);
    let mut mobile_push = use_signal(|| true);

    rsx! {
        div { class: "settings-section",
            h3 { class: "settings-section-title", "{t(\"server-settings-notifications\")}" }

            // Notification level radio group
            div { class: "notif-level-group",
                NotifLevelOption {
                    label: t("server-notif-all"),
                    selected: notif_level() == NotifLevel::All,
                    onclick: move |_| notif_level.set(NotifLevel::All),
                }
                NotifLevelOption {
                    label: t("server-notif-mentions"),
                    selected: notif_level() == NotifLevel::Mentions,
                    onclick: move |_| notif_level.set(NotifLevel::Mentions),
                }
                NotifLevelOption {
                    label: t("server-notif-nothing"),
                    selected: notif_level() == NotifLevel::Nothing,
                    onclick: move |_| notif_level.set(NotifLevel::Nothing),
                }
            }

            // Suppression toggles
            div { class: "notif-toggles",
                NotifToggleRow {
                    label: t("server-notif-suppress-everyone"),
                    checked: suppress_everyone(),
                    onchange: move |v| suppress_everyone.set(v),
                }
                NotifToggleRow {
                    label: t("server-notif-suppress-roles"),
                    checked: suppress_roles(),
                    onchange: move |v| suppress_roles.set(v),
                }
                NotifToggleRow {
                    label: t("server-notif-suppress-highlights"),
                    checked: suppress_highlights(),
                    onchange: move |v| suppress_highlights.set(v),
                }
                NotifToggleRow {
                    label: t("server-notif-mute-events"),
                    checked: mute_events(),
                    onchange: move |v| mute_events.set(v),
                }
                NotifToggleRow {
                    label: t("server-notif-mobile-push"),
                    checked: mobile_push(),
                    onchange: move |v| mobile_push.set(v),
                }
            }
        }
    }
}

/// Notification level option (radio-button style).
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn NotifLevelOption(label: String, selected: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div {
            class: if selected { "notif-level-option selected" } else { "notif-level-option" },
            onclick: move |evt| onclick.call(evt),
            div { class: "notif-level-radio" }
            span { "{label}" }
        }
    }
}

/// Toggle row for a notification suppression option.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn NotifToggleRow(label: String, checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "notif-toggle-row",
            span { class: "notif-toggle-label", "{label}" }
            input {
                r#type: "checkbox",
                class: "notif-toggle-checkbox",
                checked,
                onchange: move |e| onchange.call(e.checked()),
            }
        }
    }
}

/// Notification level for a server.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NotifLevel {
    All,
    Mentions,
    Nothing,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn server_notifications_settings_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<ServerNotificationsSettingsAction>();
        let _ = ServerNotificationsSettingsAction::SetLevel(NotifLevel::All);
        let _ = ServerNotificationsSettingsAction::SetSuppressEveryone(true);
        let _ = ServerNotificationsSettingsAction::SetSuppressRoles(false);
        let _ = ServerNotificationsSettingsAction::SetSuppressHighlights(true);
        let _ = ServerNotificationsSettingsAction::SetMuteEvents(false);
        let _ = ServerNotificationsSettingsAction::SetMobilePush(true);
    }
}
