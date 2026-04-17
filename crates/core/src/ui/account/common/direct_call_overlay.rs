//! Route-backed pending direct call overlay.
//!
//! This provides a lightweight "calling…" / "adding to call…" phase before a
//! temporary direct call becomes connected, so mobile users can swipe back or tap
//! × to cancel before the call actually starts.

#[cfg(not(target_arch = "wasm32"))]
use super::direct_call::{DirectCallRequest, start_direct_call_from_active_account};
#[cfg(not(target_arch = "wasm32"))]
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, ChatData};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::VoiceConnectionKind;
use poly_ui_macros::context_menu;

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn OutgoingDirectCallOverlay(
    backend: String,
    instance_id: String,
    account_id: String,
    dm_id: String,
    start_video: bool,
    allow_add_to_active_temporary: bool,
) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    #[cfg(not(target_arch = "wasm32"))]
    let client_manager: Signal<ClientManager> = use_context();
    let nav = navigator();

    let target_user = chat_data
        .read()
        .dm_channels
        .iter()
        .find(|dm| dm.account_id == account_id && dm.id == dm_id)
        .map(|dm| dm.user.clone());

    let active_temp_call = chat_data
        .read()
        .voice_connection
        .clone()
        .filter(|connection| connection.kind == VoiceConnectionKind::TemporaryCall);
    let is_add_flow = allow_add_to_active_temporary
        && active_temp_call.as_ref().is_some_and(|connection| {
            target_user
                .as_ref()
                .is_some_and(|user| !connection.participant_user_ids.iter().any(|id| id == &user.id))
        });

    let target_name = target_user
        .as_ref()
        .map(|user| user.display_name.clone())
        .unwrap_or_else(|| t("voice-direct-call"));
    let subtitle = if is_add_flow {
        if start_video {
            t("direct-call-adding-video")
        } else {
            t("direct-call-adding")
        }
    } else if start_video {
        t("direct-call-calling-video")
    } else {
        t("direct-call-calling")
    };
    let status_line = if is_add_flow {
        t("direct-call-awaiting-join")
    } else {
        t("direct-call-ringing")
    };
    let avatar_url = target_user.as_ref().and_then(|user| user.avatar_url.clone());
    let first_char = target_name.chars().next().unwrap_or('?');
    let return_route = Route::DmChat {
        backend: backend.clone(),
        instance_id: instance_id.clone(),
        account_id: account_id.clone(),
        dm_id: dm_id.clone(),
    };
    #[cfg(target_arch = "wasm32")]
    let expected_path = match (start_video, allow_add_to_active_temporary) {
        (false, false) => format!("/{backend}/{instance_id}/{account_id}/dms/{dm_id}/call"),
        (true, false) => format!("/{backend}/{instance_id}/{account_id}/dms/{dm_id}/video-call"),
        (false, true) => format!("/{backend}/{instance_id}/{account_id}/dms/{dm_id}/call/add"),
        (true, true) => format!("/{backend}/{instance_id}/{account_id}/dms/{dm_id}/video-call/add"),
    };
    let user_for_connect = target_user.clone();
    let overlay_return_route = return_route.clone();
    let close_button_route = return_route.clone();
    let cancel_button_route = return_route.clone();

    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        if user_for_connect.is_none() {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let Some(target_user) = user_for_connect.clone() else {
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        let nav_for_effect = nav;
        #[cfg(not(target_arch = "wasm32"))]
        let return_route_for_effect = return_route.clone();
        #[cfg(target_arch = "wasm32")]
        let expected_path_for_effect = expected_path.clone();
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            {
                let js = format!(
                    "setTimeout(() => {{ \
                        if (window.location.pathname !== {path:?}) return; \
                        window.__polyPendingDirectCallReady = true; \
                        window.history.back(); \
                    }}, 1350);",
                    path = expected_path_for_effect,
                );
                let _ = document::eval(&js);
                return;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::time::sleep(std::time::Duration::from_millis(1350)).await;
                nav_for_effect.replace(return_route_for_effect);
                start_direct_call_from_active_account(
                    DirectCallRequest {
                        target_user,
                        start_video,
                        allow_add_to_active_temporary,
                    },
                    app_state,
                    chat_data,
                    client_manager,
                );
            }
        });
    });

    rsx! {
        div {
            class: "direct-call-overlay",
            onclick: move |_| {
                app_state.write().nav.pending_direct_call = None;
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = document::eval("window.__polyPendingDirectCallReady = false;");
                }
                nav.replace(overlay_return_route.clone());
            },
            div {
                class: "direct-call-modal",
                onclick: move |e| e.stop_propagation(),
                button {
                    class: "direct-call-close-btn",
                    title: t("action-close"),
                    onclick: move |_| {
                        app_state.write().nav.pending_direct_call = None;
                        #[cfg(target_arch = "wasm32")]
                        {
                            let _ = document::eval("window.__polyPendingDirectCallReady = false;");
                        }
                        nav.replace(close_button_route.clone());
                    },
                    "✕"
                }
                div { class: "direct-call-modal-body",
                    div { class: "direct-call-avatar-wrap",
                        if let Some(ref avatar) = avatar_url {
                            img {
                                class: "direct-call-avatar",
                                src: "{avatar}",
                                alt: "{target_name}",
                            }
                        } else {
                            div { class: "direct-call-avatar direct-call-avatar-fallback", "{first_char}" }
                        }
                    }
                    div { class: "direct-call-title", "{target_name}" }
                    div { class: "direct-call-subtitle", "{subtitle}" }
                    div { class: "direct-call-status", "{status_line}" }
                    div { class: "direct-call-pulse-row",
                        span { class: "direct-call-pulse-dot" }
                        span { class: "direct-call-pulse-dot direct-call-pulse-dot-delay-1" }
                        span { class: "direct-call-pulse-dot direct-call-pulse-dot-delay-2" }
                    }
                    button {
                        class: "direct-call-cancel-btn",
                        onclick: move |_| {
                            app_state.write().nav.pending_direct_call = None;
                            #[cfg(target_arch = "wasm32")]
                            {
                                let _ = document::eval("window.__polyPendingDirectCallReady = false;");
                            }
                            nav.replace(cancel_button_route.clone());
                        },
                        {t("direct-call-cancel")}
                    }
                }
            }
        }
    }
}
