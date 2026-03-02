//! Notifications settings — desktop permission, per-event toggles, sounds, badges.
//!
//! Settings are persisted to storage via `NotificationSettings`.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::i18n::t;
use crate::storage::NotificationSettings;
use dioxus::prelude::*;

/// Persist current notification settings to storage.
fn save_notif_settings(settings: &NotificationSettings) {
    let settings = settings.clone();
    spawn(async move {
        if let Some(storage) = crate::STORAGE.get()
            && let Err(e) = storage.set_notification_settings(&settings).await
        {
            tracing::warn!("Failed to save notification settings: {e}");
        }
    });
}

/// Notifications settings section.
///
/// Controls desktop notification permission, per-event notification
/// toggles, sound preferences and badge visibility. Persisted to storage.
#[component]
pub(super) fn NotificationsSettings() -> Element {
    let mut desktop_notifs = use_signal(|| false);
    let mut notif_streams = use_signal(|| true);
    let mut notif_friends_voice = use_signal(|| true);
    let mut notif_reactions = use_signal(|| true);
    let mut sound_new_msg = use_signal(|| true);
    let mut sound_dm = use_signal(|| true);
    let mut sound_ring = use_signal(|| true);
    let mut badge_unread = use_signal(|| true);

    // Load persisted settings on mount
    let _load = use_future(move || async move {
        if let Some(storage) = crate::STORAGE.get()
            && let Ok(s) = storage.get_notification_settings().await
        {
            desktop_notifs.set(s.desktop_enabled);
            notif_streams.set(s.notify_streams);
            notif_friends_voice.set(s.notify_friends_voice);
            notif_reactions.set(s.notify_reactions);
            sound_new_msg.set(s.sound_new_message);
            sound_dm.set(s.sound_dm);
            sound_ring.set(s.sound_ring);
            badge_unread.set(s.badge_unread);
        }
    });

    // Helper closure that saves all settings
    let save_all = move || {
        save_notif_settings(&NotificationSettings {
            desktop_enabled: *desktop_notifs.read(),
            notify_streams: *notif_streams.read(),
            notify_friends_voice: *notif_friends_voice.read(),
            notify_reactions: *notif_reactions.read(),
            sound_new_message: *sound_new_msg.read(),
            sound_dm: *sound_dm.read(),
            sound_ring: *sound_ring.read(),
            badge_unread: *badge_unread.read(),
        });
    };

    rsx! {
        div { class: "settings-section notif-settings",
            h2 { "{t(\"settings-notifications\")}" }
            NotifPermissionRow {
                enabled: *desktop_notifs.read(),
                on_toggle: move |v| {
                    desktop_notifs.set(v);
                    save_all();
                },
            }
            h3 { class: "notif-section-header", "Notify me about" }
            NotifToggleRow {
                label: t("notif-streams"),
                checked: *notif_streams.read(),
                on_toggle: move |v| {
                    notif_streams.set(v);
                    save_all();
                },
            }
            NotifToggleRow {
                label: t("notif-friends-voice"),
                checked: *notif_friends_voice.read(),
                on_toggle: move |v| {
                    notif_friends_voice.set(v);
                    save_all();
                },
            }
            NotifToggleRow {
                label: t("notif-reactions"),
                checked: *notif_reactions.read(),
                on_toggle: move |v| {
                    notif_reactions.set(v);
                    save_all();
                },
            }
            h3 { class: "notif-section-header", "Sounds" }
            NotifToggleRow {
                label: t("notif-sounds-new-message"),
                checked: *sound_new_msg.read(),
                on_toggle: move |v| {
                    sound_new_msg.set(v);
                    save_all();
                },
            }
            NotifToggleRow {
                label: t("notif-sounds-dm"),
                checked: *sound_dm.read(),
                on_toggle: move |v| {
                    sound_dm.set(v);
                    save_all();
                },
            }
            NotifToggleRow {
                label: t("notif-sounds-ring"),
                checked: *sound_ring.read(),
                on_toggle: move |v| {
                    sound_ring.set(v);
                    save_all();
                },
            }
            h3 { class: "notif-section-header", "Badges" }
            NotifToggleRow {
                label: t("notif-badge-unread"),
                checked: *badge_unread.read(),
                on_toggle: move |v| {
                    badge_unread.set(v);
                    save_all();
                },
            }
        }
    }
}

/// Desktop notification permission row with toggle + request button.
#[component]
fn NotifPermissionRow(enabled: bool, on_toggle: EventHandler<bool>) -> Element {
    rsx! {
        div { class: "notif-toggle-row notif-permission-row",
            div { class: "notif-toggle-label",
                span { class: "notif-toggle-title", "{t(\"notif-enable-desktop\")}" }
                span { class: "notif-toggle-desc", "Requires browser / OS permission" }
            }
            div { class: "notif-permission-controls",
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: enabled,
                        onchange: move |e| on_toggle.call(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
                if !enabled {
                    button {
                        class: "btn btn-primary btn-sm",
                        onclick: move |_| {
                            // TODO(phase-3): call Notification.requestPermission() via JS eval
                            on_toggle.call(true);
                        },
                        "{t(\"notif-permission-request\")}"
                    }
                }
            }
        }
    }
}

/// Generic notification toggle row.
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
