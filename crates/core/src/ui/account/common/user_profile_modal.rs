//! Unified user profile modal.
//!
//! Shown when clicking any user — member list, DM contact, voice participant,
//! or a chat message's author name.
//!
//! ## Layout
//! - **Overlay**: fixed full-screen backdrop (z-index 2000), click to close.
//! - **Desktop** (>520 px): centered card, max-width 460 px, scrollable if tall.
//! - **Mobile** (≤520 px): full-height sheet, slides up from the bottom.
//!
//! ## Discord-style content
//! Banner → avatar (overlaps banner) with presence dot → name → action row
//! (Message / Call / Video) → divider → backend badge → note text area.
//!
//! ## Back-gesture support (WASM / mobile)
//! `open_user_profile` pushes `#poly-profile` onto the browser history.
//! A JS promise awaits the next `hashchange` event that removes the hash,
//! and resolves back into Rust to close the modal. Closing via "×" also clears
//! the hash by calling `history.back()` (which fires the hashchange + resolves
//! the promise as a harmless no-op).

use crate::state::BatchedSignal;
use super::channel_list::open_direct_message_from_active_account;
use super::direct_call::{DirectCallRequest, navigate_to_pending_direct_call_from_active_account};
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AccountSessions, ChatData, ChatLists, NavState, UiOverlays};
use crate::state::VoiceState;
use crate::state::chat_data::{backend_badge, user_color};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_client::{PresenceStatus, User, VoiceConnectionKind};
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the user profile modal.
#[derive(Debug, Clone)]
pub enum UserProfileModalAction {
    /// Close the modal.
    Close,
    /// Open a DM with the displayed user.
    OpenDm,
    /// Initiate a voice call with the displayed user.
    Call,
    /// Initiate a video call with the displayed user.
    VideoCall,
}

impl UiAction for UserProfileModalAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::Close | Self::OpenDm | Self::Call | Self::VideoCall => {
                todo!("phase-E: UserProfileModalAction requires Signal handles");
            }
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Open the global user profile modal for `user`.
///
/// Stores the user in `AppState.nav.profile_modal_user` and, on WASM,
/// pushes `#poly-profile` to the browser history so the native back
/// gesture closes the modal.
pub fn open_user_profile(ui_overlays: BatchedSignal<UiOverlays>, user: User) {
    ui_overlays.batch(|o| o.profile_modal_user = Some(user));
    #[cfg(target_arch = "wasm32")]
    {
        let _ = document::eval(
            "if(location.hash!=='#poly-profile'){\
                history.pushState(null,'',\
                    location.pathname+location.search+'#poly-profile');\
            }",
        );
    }
}

// ── Internal close helper ─────────────────────────────────────────────────────

/// Close the modal and strip the `#poly-profile` hash from the address bar.
///
/// Uses `history.back()` so the hashchange event fires and the async back-gesture
/// listener (if running) resolves cleanly without a second write.
fn close_modal(ui_overlays: BatchedSignal<UiOverlays>) {
    ui_overlays.batch(|o| o.profile_modal_user = None);
    #[cfg(target_arch = "wasm32")]
    {
        let _ = document::eval(
            "if(location.hash==='#poly-profile'){ history.back(); }\
             else { history.replaceState(null,'',location.pathname+location.search); }",
        );
    }
}

// ── Component ─────────────────────────────────────────────────────────────────

/// Global user profile modal.
///
/// Render this **once** from `AppBody` — it is a no-op when
/// `AppState.nav.profile_modal_user` is `None`.
#[ui_action(UserProfileModalAction)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn UserProfileModal() -> Element {
    let nav_state: BatchedSignal<NavState> = use_context();
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let voice_state: BatchedSignal<VoiceState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let user = ui_overlays.read().profile_modal_user.clone();
    let Some(user) = user else {
        return rsx! {};
    };

    // Back-gesture + Escape support on WASM:
    // await a JS promise that resolves when either:
    // 1) the hash navigates away from #poly-profile (browser back / close), or
    // 2) Escape is pressed.
    //
    // Both event listeners are removed on first resolve.
    #[cfg(target_arch = "wasm32")]
    {
        use_effect(move || {
            spawn(async move {
                let mut eval = document::eval(
                    "(function(){\
                        var done = false;\
                        function cleanup() {\
                            if (done) return;\
                            done = true;\
                            window.removeEventListener('hashchange', onHash);\
                            document.removeEventListener('keydown', onKey, true);\
                        }\
                        function onHash() {\
                            if(location.hash!=='#poly-profile'){\
                                cleanup();\
                                dioxus.send('close');\
                            }\
                        }\
                        function onKey(e) {\
                            if (e.key === 'Escape') {\
                                e.preventDefault();\
                                cleanup();\
                                dioxus.send('close');\
                            }\
                        }\
                        window.addEventListener('hashchange', onHash);\
                        document.addEventListener('keydown', onKey, true);\
                    })()",
                );
                if eval.recv::<String>().await.is_ok() {
                    // Route through close_modal so browser hash and signal state
                    // always converge regardless of whether dismissal came from
                    // Escape, backdrop/cross button, or browser back.
                    close_modal(ui_overlays);
                }
            });
        });
    }

    let color = user_color(&user.id);
    let banner_style = format!(
        "background: linear-gradient(135deg, {color}, \
         color-mix(in srgb, {color} 30%, #0a0a1a));",
    );
    let first_char: String = user
        .display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let presence_dot_class = match user.presence {
        PresenceStatus::Online => "status-dot presence-dot online",
        PresenceStatus::Idle => "status-dot presence-dot away",
        PresenceStatus::DoNotDisturb => "status-dot presence-dot dnd",
        PresenceStatus::Invisible | PresenceStatus::Offline => "status-dot presence-dot offline",
        // Unknown — backend has no presence info; suppress the dot.
        PresenceStatus::Unknown => "status-dot presence-dot offline",
    };
    let presence_label = match user.presence {
        PresenceStatus::Online => t("user-online"),
        PresenceStatus::Idle => t("user-idle"),
        PresenceStatus::DoNotDisturb => t("user-dnd"),
        PresenceStatus::Invisible => t("user-invisible"),
        PresenceStatus::Offline => t("user-offline"),
        PresenceStatus::Unknown => String::new(),
    };
    let backend_icon = backend_badge(&user.backend);
    let backend_name = user.backend.display_name().to_string();
    let avatar_url = user.avatar_url.clone();
    let display_name = user.display_name.clone();
    let active_temp_call = voice_state
        .read()
        .voice_connection
        .clone()
        .filter(|connection| connection.kind == VoiceConnectionKind::TemporaryCall);
    let adding_to_active_call = active_temp_call.as_ref().is_some_and(|connection| {
        !connection.participant_user_ids.iter().any(|id| id == &user.id)
    });
    let call_action_label = if adding_to_active_call {
        t("user-profile-add-to-call")
    } else {
        t("user-profile-call")
    };
    let video_action_label = if adding_to_active_call {
        t("user-profile-add-video-to-call")
    } else {
        t("user-profile-video")
    };
    let action_user = user.clone();
    let call_user = user.clone();
    let video_user = user.clone();

    rsx! {
        // Full-screen backdrop — click to close
        div {
            class: "poly-profile-overlay",
            onclick: move |_| close_modal(ui_overlays),

            // Modal card — stops click propagation
            div {
                class: "poly-profile-modal",
                onclick: move |e| e.stop_propagation(),

                // ── Banner with × and ··· buttons ────────────────────────────
                div { class: "poly-profile-banner", style: "{banner_style}",
                    button {
                        class: "poly-profile-header-btn poly-profile-close-btn",
                        title: "{t(\"action-close\")}",
                        onclick: move |_| close_modal(ui_overlays),
                        "✕"
                    }
                    button {
                        class: "poly-profile-header-btn poly-profile-more-btn",
                        title: "{t(\"user-profile-more-options\")}",
                        "···"
                    }
                }

                // ── Avatar row (overlaps bottom of banner) ───────────────────
                div { class: "poly-profile-avatar-row",
                    div { class: "poly-profile-avatar-wrap",
                        if let Some(ref url) = avatar_url {
                            img {
                                class: "poly-profile-avatar-img",
                                src: "{url}",
                                alt: "{display_name}",
                            }
                        } else {
                            div {
                                class: "poly-profile-avatar-fallback",
                                style: "background-color: {color};",
                                "{first_char}"
                            }
                        }
                        span { class: "{presence_dot_class} poly-profile-presence-dot" }
                    }
                }

                // ── Scrollable body ──────────────────────────────────────────
                div { class: "poly-profile-body",

                    // Name + presence status
                    div { class: "poly-profile-identity",
                        h2 { class: "poly-profile-name", "{display_name}" }
                        div { class: "poly-profile-status-line",
                            span { class: "{presence_dot_class}" }
                            span { class: "poly-profile-status-text", "{presence_label}" }
                        }
                    }

                    // Action buttons
                    div { class: "poly-profile-actions",
                        button { class: "poly-profile-action-btn",
                            onclick: move |_| {
                                close_modal(ui_overlays);
                                open_direct_message_from_active_account(
                                    action_user.id.clone(),
                                    nav_state,
                                    chat_data,
                                    client_manager,
                                    navigator(),
                                );
                            },
                            span { class: "poly-profile-action-icon", "💬" }
                            span { class: "poly-profile-action-label",
                                "{t(\"user-profile-message\")}"
                            }
                        }
                        button { class: "poly-profile-action-btn",
                            onclick: move |_| {
                                close_modal(ui_overlays);
                                navigate_to_pending_direct_call_from_active_account(
                                    DirectCallRequest {
                                        target_user: call_user.clone(),
                                        start_video: false,
                                        allow_add_to_active_temporary: true,
                                    },
                                    nav_state,
                                    ui_overlays,
                                    chat_lists,
                                    account_sessions,
                                    client_manager,
                                    navigator(),
                                );
                            },
                            span { class: "poly-profile-action-icon", "📞" }
                            span { class: "poly-profile-action-label",
                                "{call_action_label}"
                            }
                        }
                        button { class: "poly-profile-action-btn",
                            onclick: move |_| {
                                close_modal(ui_overlays);
                                navigate_to_pending_direct_call_from_active_account(
                                    DirectCallRequest {
                                        target_user: video_user.clone(),
                                        start_video: true,
                                        allow_add_to_active_temporary: true,
                                    },
                                    nav_state,
                                    ui_overlays,
                                    chat_lists,
                                    account_sessions,
                                    client_manager,
                                    navigator(),
                                );
                            },
                            span { class: "poly-profile-action-icon", "🎥" }
                            span { class: "poly-profile-action-label",
                                "{video_action_label}"
                            }
                        }
                    }

                    div { class: "poly-profile-divider" }

                    // Backend / source badge
                    div { class: "poly-profile-meta",
                        span { class: "account-backend-badge", "{backend_icon}" }
                        span { class: "poly-profile-status-text", "{backend_name}" }
                    }

                    // Note section
                    div { class: "poly-profile-note-section",
                        label { class: "poly-profile-note-label",
                            "{t(\"user-profile-note\")}"
                        }
                        NoteEditor {}
                    }
                }
            }
        }
    }
}

/// Editable note text area with a live character counter.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(allow_default)]
#[component]
fn NoteEditor() -> Element {
    let mut note = use_signal(String::new);
    let len = note.read().len();
    rsx! {
        div { class: "poly-profile-note-wrap",
            textarea {
                class: "poly-profile-note-input",
                placeholder: "{t(\"user-profile-note-placeholder\")}",
                maxlength: "256",
                value: "{note}",
                oninput: move |e| note.set(e.value()),
            }
            span { class: "poly-profile-note-count", "{len}/256" }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn user_profile_modal_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<UserProfileModalAction>();
        let _ = UserProfileModalAction::Close;
        let _ = UserProfileModalAction::OpenDm;
        let _ = UserProfileModalAction::Call;
        let _ = UserProfileModalAction::VideoCall;
    }
}
