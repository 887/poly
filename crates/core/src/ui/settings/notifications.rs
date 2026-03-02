//! Notifications settings — desktop permission, per-event toggles, sounds, badges.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::i18n::t;
use dioxus::prelude::*;

/// Notifications settings section.
///
/// Controls desktop notification permission, per-event notification
/// toggles, sound preferences and badge visibility.
/// The actual permission request uses the Web Notifications API (on web)
/// and OS notifications on desktop — wired up in later phases.
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

    rsx! {
        div { class: "settings-section notif-settings",
            h2 { "{t(\"settings-notifications\")}" }

            // Desktop notification permission
            div { class: "notif-toggle-row notif-permission-row",
                div { class: "notif-toggle-label",
                    span { class: "notif-toggle-title", "{t(\"notif-enable-desktop\")}" }
                    span { class: "notif-toggle-desc", "Requires browser / OS permission" }
                }
                div { class: "notif-permission-controls",
                    label { class: "toggle-switch",
                        input {
                            r#type: "checkbox",
                            checked: *desktop_notifs.read(),
                            onchange: move |e| desktop_notifs.set(e.checked()),
                        }
                        span { class: "toggle-slider" }
                    }
                    if !*desktop_notifs.read() {
                        button {
                            class: "btn btn-primary btn-sm",
                            onclick: move |_| {
                                // TODO(phase-3): call Notification.requestPermission() via JS eval
                                desktop_notifs.set(true);
                            },
                            "{t(\"notif-permission-request\")}"
                        }
                    }
                }
            }

            h3 { class: "notif-section-header", "Notify me about" }

            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-streams\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *notif_streams.read(),
                        onchange: move |e| notif_streams.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-friends-voice\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *notif_friends_voice.read(),
                        onchange: move |e| notif_friends_voice.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-reactions\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *notif_reactions.read(),
                        onchange: move |e| notif_reactions.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }

            h3 { class: "notif-section-header", "Sounds" }

            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-sounds-new-message\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *sound_new_msg.read(),
                        onchange: move |e| sound_new_msg.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-sounds-dm\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *sound_dm.read(),
                        onchange: move |e| sound_dm.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-sounds-ring\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *sound_ring.read(),
                        onchange: move |e| sound_ring.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }

            h3 { class: "notif-section-header", "Badges" }

            div { class: "notif-toggle-row",
                span { class: "notif-toggle-title", "{t(\"notif-badge-unread\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *badge_unread.read(),
                        onchange: move |e| badge_unread.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
        }
    }
}
