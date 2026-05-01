//! Account-specific notification settings.
//!
//! Provides per-account toggles for event notifications, sounds, and badges.
//! This module lives under `account/settings/` to clearly separate account-scoped
//! settings from app-level settings (theme, language, backup, identity, etc.).
//!
//! # Architecture
//! - `NotificationsSettings` — top-level entry point; accepts a single `account_id`
//!   and loads/persists `AccountNotificationSettings` for it.
//! - `AccountNotifSectionInner` — pure presentation split out to respect the
//!   150-line component rule.
//! - `NotifToggleRow` — reusable labeled toggle row.
//!
//! # Storage key
//! Settings are stored under `"notif:{account_id}"` via
//! `Storage::set_account_notification_settings`.

use crate::i18n::t;
use crate::storage::AccountNotificationSettings;
use dioxus::prelude::*;
use poly_client::capabilities_for_slug;
use poly_ui_macros::{context_menu, ui_action};

/// Typed actions for the per-account notification settings panel.
pub enum NotificationsSettingsAction {
    ToggleStreams(bool),
    ToggleFriendsVoice(bool),
    ToggleReactions(bool),
    ToggleSoundNewMessage(bool),
    ToggleSoundDm(bool),
    ToggleSoundRing(bool),
    ToggleBadgeUnread(bool),
    Save,
}

impl crate::ui::actions::UiAction for NotificationsSettingsAction {
    fn apply(self, _cx: crate::ui::actions::ActionCx<'_>) {
        match self {
            Self::Save => todo!("phase-E: persist notification settings"),
            Self::ToggleStreams(_)
            | Self::ToggleFriendsVoice(_)
            | Self::ToggleReactions(_)
            | Self::ToggleSoundNewMessage(_)
            | Self::ToggleSoundDm(_)
            | Self::ToggleSoundRing(_)
            | Self::ToggleBadgeUnread(_) => todo!("phase-E: update notification state"),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct NotifSignals {
    notif_streams: Signal<bool>,
    notif_friends_voice: Signal<bool>,
    notif_reactions: Signal<bool>,
    sound_new_msg: Signal<bool>,
    sound_dm: Signal<bool>,
    sound_ring: Signal<bool>,
    badge_unread: Signal<bool>,
}

/// Persist per-account notification settings to storage.
fn save_account_notif(account_id: String, settings: AccountNotificationSettings) {
    spawn(async move {
        if let Some(storage) = crate::STORAGE.get()
            && let Err(e) = storage
                .set_account_notification_settings(&account_id, &settings)
                .await
        {
            tracing::warn!("Failed to save account notification settings for {account_id}: {e}");
        }
    });
}

fn current_notif_settings(signals: NotifSignals) -> AccountNotificationSettings {
    AccountNotificationSettings {
        notify_streams: *signals.notif_streams.read(),
        notify_friends_voice: *signals.notif_friends_voice.read(),
        notify_reactions: *signals.notif_reactions.read(),
        sound_new_message: *signals.sound_new_msg.read(),
        sound_dm: *signals.sound_dm.read(),
        sound_ring: *signals.sound_ring.read(),
        badge_unread: *signals.badge_unread.read(),
    }
}

fn load_account_notif_settings(account_id: String, mut signals: NotifSignals) {
    spawn(async move {
        if let Some(storage) = crate::STORAGE.get()
            && let Ok(s) = storage.get_account_notification_settings(&account_id).await
        {
            signals.notif_streams.set(s.notify_streams);
            signals.notif_friends_voice.set(s.notify_friends_voice);
            signals.notif_reactions.set(s.notify_reactions);
            signals.sound_new_msg.set(s.sound_new_message);
            signals.sound_dm.set(s.sound_dm);
            signals.sound_ring.set(s.sound_ring);
            signals.badge_unread.set(s.badge_unread);
        }
    });
}

/// Notification settings panel for a single account.
///
/// `backend` is the backend slug (e.g. `"discord"`, `"lemmy"`) used to gate
/// toggles that only apply to certain backends (voice, friends, etc.).
///
/// Loads saved settings on mount and persists any toggle change immediately.
/// Rendered by [`crate::ui::account::settings::AccountSettingsPage`].
#[ui_action(NotificationsSettingsAction)]
#[context_menu(none)]
#[component]
pub fn NotificationsSettings(account_id: String, backend: String) -> Element {
    let notif_streams = use_signal(|| true);
    let notif_friends_voice = use_signal(|| true);
    let notif_reactions = use_signal(|| true);
    let sound_new_msg = use_signal(|| true);
    let sound_dm = use_signal(|| true);
    let sound_ring = use_signal(|| true);
    let badge_unread = use_signal(|| true);
    let signals = NotifSignals {
        notif_streams,
        notif_friends_voice,
        notif_reactions,
        sound_new_msg,
        sound_dm,
        sound_ring,
        badge_unread,
    };
    let account_id_for_load = account_id.clone();
    let caps = capabilities_for_slug(&backend);

    let _load = use_future(move || {
        let aid = account_id_for_load.clone();
        async move {
            load_account_notif_settings(aid, signals);
        }
    });

    rsx! {
        AccountNotifSignalsSection { account_id, signals, caps }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn AccountNotifSignalsSection(account_id: String, signals: NotifSignals, caps: poly_client::BackendCapabilities) -> Element {
    let make_settings = move || current_notif_settings(signals);

    rsx! {
        AccountNotifSectionInner {
            account_id: account_id.clone(),
            notif_streams: *signals.notif_streams.read(),
            notif_friends_voice: *signals.notif_friends_voice.read(),
            notif_reactions: *signals.notif_reactions.read(),
            sound_new_msg: *signals.sound_new_msg.read(),
            sound_dm: *signals.sound_dm.read(),
            sound_ring: *signals.sound_ring.read(),
            badge_unread: *signals.badge_unread.read(),
            show_friends_voice: caps.should_show_voice() && caps.should_show_friends(),
            show_ring_sound: caps.should_show_voice(),
            show_dm_sound: caps.should_show_dms(),
            on_streams: {
                let aid = account_id.clone();
                move |v: bool| {
                    signals.notif_streams.set(v);
                    save_account_notif(aid.clone(), make_settings());
                }
            },
            on_friends_voice: {
                let aid = account_id.clone();
                move |v: bool| {
                    signals.notif_friends_voice.set(v);
                    save_account_notif(aid.clone(), make_settings());
                }
            },
            on_reactions: {
                let aid = account_id.clone();
                move |v: bool| {
                    signals.notif_reactions.set(v);
                    save_account_notif(aid.clone(), make_settings());
                }
            },
            on_sound_msg: {
                let aid = account_id.clone();
                move |v: bool| {
                    signals.sound_new_msg.set(v);
                    save_account_notif(aid.clone(), make_settings());
                }
            },
            on_sound_dm: {
                let aid = account_id.clone();
                move |v: bool| {
                    signals.sound_dm.set(v);
                    save_account_notif(aid.clone(), make_settings());
                }
            },
            on_sound_ring: {
                let aid = account_id.clone();
                move |v: bool| {
                    signals.sound_ring.set(v);
                    save_account_notif(aid.clone(), make_settings());
                }
            },
            on_badge: {
                let aid = account_id.clone();
                move |v: bool| {
                    signals.badge_unread.set(v);
                    save_account_notif(aid.clone(), make_settings());
                }
            },
        }
    }
}

/// Inner presentation component for an account's notification toggles.
///
/// `show_friends_voice` gates the "Friends join voice channels" toggle.
/// `show_ring_sound` gates the "Incoming Ring" sound toggle.
/// `show_dm_sound` gates the "Direct Messages" sound toggle (needs DM support).
///
/// Split out so `NotificationsSettings` stays under the 150-line limit.
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn AccountNotifSectionInner(
    account_id: String,
    notif_streams: bool,
    notif_friends_voice: bool,
    notif_reactions: bool,
    sound_new_msg: bool,
    sound_dm: bool,
    sound_ring: bool,
    badge_unread: bool,
    /// Show the "Friends join voice channels" toggle (needs voice + friends).
    show_friends_voice: bool,
    /// Show the "Incoming Ring" sound toggle (needs voice).
    show_ring_sound: bool,
    /// Show the "Direct Messages" sound toggle (needs DM support).
    show_dm_sound: bool,
    on_streams: EventHandler<bool>,
    on_friends_voice: EventHandler<bool>,
    on_reactions: EventHandler<bool>,
    on_sound_msg: EventHandler<bool>,
    on_sound_dm: EventHandler<bool>,
    on_sound_ring: EventHandler<bool>,
    on_badge: EventHandler<bool>,
) -> Element {
    rsx! {
        div { class: "settings-section notif-settings",
            h2 { "{t(\"settings-notifications\")}" }
            div { class: "notif-group notif-group-account",
                h3 { class: "notif-section-header", "{account_id}" }
                h4 { class: "notif-subsection-header", "{t(\"notif-notify-about\")}" }
                NotifToggleRow {
                    label: t("notif-streams"),
                    checked: notif_streams,
                    on_toggle: move |v| on_streams.call(v),
                }
                if show_friends_voice {
                    NotifToggleRow {
                        label: t("notif-friends-voice"),
                        checked: notif_friends_voice,
                        on_toggle: move |v| on_friends_voice.call(v),
                    }
                }
                NotifToggleRow {
                    label: t("notif-reactions"),
                    checked: notif_reactions,
                    on_toggle: move |v| on_reactions.call(v),
                }
                h4 { class: "notif-subsection-header", "{t(\"notif-sounds\")}" }
                NotifToggleRow {
                    label: t("notif-sounds-new-message"),
                    checked: sound_new_msg,
                    on_toggle: move |v| on_sound_msg.call(v),
                }
                if show_dm_sound {
                    NotifToggleRow {
                        label: t("notif-sounds-dm"),
                        checked: sound_dm,
                        on_toggle: move |v| on_sound_dm.call(v),
                    }
                }
                if show_ring_sound {
                    NotifToggleRow {
                        label: t("notif-sounds-ring"),
                        checked: sound_ring,
                        on_toggle: move |v| on_sound_ring.call(v),
                    }
                }
                h4 { class: "notif-subsection-header", "{t(\"notif-badges\")}" }
                NotifToggleRow {
                    label: t("notif-badge-unread"),
                    checked: badge_unread,
                    on_toggle: move |v| on_badge.call(v),
                }
            }
        }
    }
}

/// Generic notification toggle row.
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn NotifToggleRow(label: String, checked: bool, on_toggle: EventHandler<bool>) -> Element {
    rsx! {
        div { class: "notif-toggle-row",
            span { class: "notif-toggle-title", "{label}" }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked,
                    onchange: move |e| on_toggle.call(e.checked()),
                }
                span { class: "toggle-slider" }
            }
        }
    }
}
