//! Account switcher bar — multi-account quick access for Direct Messages.
//!
//! Displayed at the bottom of the channel list in Direct Messages (like AccountBar
//! in server view), but simplified to show account switching and settings only.
//! Identical layout to AccountBar for consistency.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.20): Account switcher bar for DMs

use crate::state::BatchedSignal;
use super::super::super::routes::Route;
use crate::i18n::t;
use crate::state::{AppState, SettingsSection, VoiceState};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Account switcher bar component (replaces AccountBar in DMs).
///
/// Shows account switching and settings buttons in a bar at the bottom
/// of the channel list (same style/position as AccountBar for servers).
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AccountSwitcher() -> Element {
    let user_prefs: crate::state::BatchedSignal<crate::state::UserPrefs> = use_context();
    let voice_state: BatchedSignal<VoiceState> = use_context();
    let voice_conn = voice_state.read().voice_connection.clone();

    let is_muted = voice_conn.as_ref().is_some_and(|vc| vc.is_muted);
    let is_deafened = voice_conn.as_ref().is_some_and(|vc| vc.is_deafened);

    rsx! {
        div { class: "account-switcher-bar",
            // Account switcher button (left side, like user avatar in AccountBar)
            button {
                class: "account-switcher-main-btn",
                title: "{t(\"account-switch\")}",
                onclick: move |_| {
                    user_prefs.batch(|p| p.settings_section = SettingsSection::Accounts);
                    crate::nav!(Route::SettingsRoute);
                },
                "👥"
            }
            // Mute/Deafen controls (always shown)
            div { class: "account-switcher-controls",
                // Mic mute toggle
                button {
                    class: if is_muted { "account-switcher-control-btn active" } else { "account-switcher-control-btn" },
                    title: if is_muted { t("voice-unmute") } else { t("voice-mute") },
                    onclick: move |_| {
                        voice_state.batch(|v| {
                            if let Some(ref mut vc) = v.voice_connection {
                                vc.is_muted = !vc.is_muted;
                            }
                        });
                    },
                    if is_muted {
                        "🔇"
                    } else {
                        "🎤"
                    }
                }
                // Deafen toggle
                button {
                    class: if is_deafened { "account-switcher-control-btn active" } else { "account-switcher-control-btn" },
                    title: if is_deafened { t("voice-undeafen") } else { t("voice-deafen") },
                    onclick: move |_| {
                        voice_state.batch(|v| {
                            if let Some(ref mut vc) = v.voice_connection {
                                vc.is_deafened = !vc.is_deafened;
                            }
                        });
                    },
                    if is_deafened {
                        "🔕"
                    } else {
                        "🔊"
                    }
                }
            }
            // Settings button (right side, like controls in AccountBar)
            button {
                class: "account-switcher-setting-btn",
                title: "{t(\"nav-settings\")}",
                onclick: move |_| {
                    user_prefs.batch(|p| p.settings_section = SettingsSection::General);
                    crate::nav!(Route::SettingsRoute);
                },
                "⚙"
            }
        }
    }
}
