//! Poly Server profile settings — shown as the "Profile" tab in per-account
//! settings when the active backend is `poly`.
//!
//! Displays: avatar (read+placeholder for upload), display name, status picker,
//! and a banner placeholder. All data is sourced from the active account session
//! and `ClientManager::presence_statuses`.
//!
//! ## Compilation gating
//! This module is only compiled when the `server` feature flag is enabled (via
//! the `#[cfg(feature = "server")] mod profile;` gate in `mod.rs`). It is
//! deliberately NOT shared with demo, Matrix, or other backends.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::ChatData;
use crate::state::chat_data::user_color;
use dioxus::prelude::*;
use poly_client::AccountPresence;
use poly_ui_macros::{context_menu, ui_action};

/// Typed actions for the Poly Server profile settings panel.
pub enum PolyProfileSettingsAction {
    SetPresence(AccountPresence),
    UploadAvatar,
    UploadBanner,
}

impl crate::ui::actions::UiAction for PolyProfileSettingsAction {
    fn apply(self, _cx: crate::ui::actions::ActionCx<'_>) {
        match self {
            Self::SetPresence(_) => todo!("phase-E: update presence via backend"),
            Self::UploadAvatar => todo!("phase-E: upload avatar"),
            Self::UploadBanner => todo!("phase-E: upload banner"),
        }
    }
}

/// Poly Server profile settings section.
///
/// Rendered inside `AccountSettingsPage` only when `backend == "poly"`.
/// Shows: avatar display, display name, status picker, banner placeholder.
#[rustfmt::skip]
#[ui_action(PolyProfileSettingsAction)]
#[context_menu(none)]
#[component]
pub fn PolyProfileSettings(account_id: String) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let mut client_manager: Signal<ClientManager> = use_context();

    // Read session info for display name and avatar.
    let (display_name, avatar_url, first_char, color) = {
        let cd = chat_data.read();
        if let Some(session) = cd.account_sessions.get(&account_id) {
            let name = session.user.display_name.clone();
            let fc = name.chars().next().map(|c| c.to_string()).unwrap_or_default();
            let col = user_color(&session.user.id).to_string();
            (name, session.user.avatar_url.clone(), fc, col)
        } else {
            (account_id.clone(), None, "?".to_string(), user_color("no-session").to_string())
        }
    };

    // Current presence status for this account.
    let current_presence: AccountPresence = client_manager
        .read()
        .presence_statuses
        .get(&account_id)
        .copied()
        .unwrap_or(AccountPresence::Online);

    let status_options: &[AccountPresence] = &[
        AccountPresence::Online,
        AccountPresence::Away,
        AccountPresence::DoNotDisturb,
        AccountPresence::AppearOffline,
    ];

    rsx! {
        div { class: "settings-section",
            // Section header
            div { class: "profile-settings-header",
                h2 { class: "settings-section-title", "{t(\"plugin-poly-profile-title\")}" }
                p { class: "settings-section-description", "{t(\"plugin-poly-profile-section-desc\")}" }
            }

            // ── Avatar row ─────────────────────────────────────────────────
            div { class: "profile-row",
                div { class: "profile-row-label",
                    label { class: "settings-toggle-label", "{t(\"plugin-poly-profile-avatar-label\")}" }
                    p { class: "settings-toggle-desc", "{t(\"plugin-poly-profile-avatar-coming-soon\")}" }
                }
                div { class: "profile-avatar-preview",
                    // Show image avatar or letter fallback
                    if let Some(ref url) = avatar_url {
                        img {
                            src: "{url}",
                            alt: "{display_name}",
                            class: "profile-avatar-img",
                        }
                    } else {
                        div {
                            class: "profile-avatar-fallback",
                            style: "background-color: {color};",
                            "{first_char}"
                        }
                    }
                }
            }

            // ── Display name row ───────────────────────────────────────────
            div { class: "profile-row",
                div { class: "profile-row-label",
                    label { class: "settings-toggle-label", "{t(\"plugin-poly-profile-display-name-label\")}" }
                    p { class: "settings-toggle-desc", "{t(\"plugin-poly-profile-display-name-desc\")}" }
                }
                div { class: "profile-row-value",
                    // Read-only for now — editing requires a server API (add_trait_method TODO).
                    span { class: "profile-display-name-text", "{display_name}" }
                }
            }

            // ── Status / presence picker ───────────────────────────────────
            div { class: "profile-row profile-row-column",
                div { class: "profile-row-label",
                    label { class: "settings-toggle-label", "{t(\"plugin-poly-profile-status-label\")}" }
                    p { class: "settings-toggle-desc", "{t(\"plugin-poly-profile-status-desc\")}" }
                }
                div { class: "profile-status-grid",
                    for &presence in status_options {
                        {
                            let css = presence.css_class();
                            let label = presence.display_name().to_string();
                            let aid = account_id.clone();
                            let is_selected = presence == current_presence;
                            rsx! {
                                button {
                                    class: if is_selected { "profile-status-option profile-status-selected" } else { "profile-status-option" },
                                    onclick: move |_| {
                                        client_manager.write().presence_statuses.insert(aid.clone(), presence);
                                    },
                                    span { class: "status-dot {css}" }
                                    span { "{label}" }
                                }
                            }
                        }
                    }
                }
            }

            // ── Banner / background placeholder ────────────────────────────
            div { class: "profile-row profile-row-column",
                div { class: "profile-row-label",
                    label { class: "settings-toggle-label", "{t(\"plugin-poly-profile-background-label\")}" }
                    p { class: "settings-toggle-desc", "{t(\"plugin-poly-profile-banner-coming-soon\")}" }
                }
                div { class: "profile-banner-placeholder",
                    span { "🖼 Banner upload coming soon" }
                }
            }
        }
    }
}
