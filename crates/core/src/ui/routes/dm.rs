//! DM-domain route adapter components.
//!
//! All account-scoped DM routes + the DMs layout wrapper + the shared
//! `restore_dm_chat` helper (called by DM viewers and the pending-call adapters).

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{
    AccountSessions, BatchedSignal, ChatLists, ChatViewState, NavState, UiOverlays, VoiceState,
    use_spawn_once,
};
use crate::ui::account::common::{FeatureUnsupportedPlaceholder, UnsupportedFeature};
use crate::ui::account::common::chat_history::initial_message_query;
use crate::ui::account::common::chat_history::request_restore_scroll_position_or_bottom;
use crate::ui::account::common::direct_call::{
    DirectCallRequest, start_direct_call_from_active_account,
};
use crate::ui::account::{
    ChatView, ConversationSearchView, NewConversationView, OutgoingDirectCallOverlay,
};
use crate::ui::account::common::VoiceAccountFooter;
use crate::ui::client_ui::ClientSidebar;
use crate::ui::split_shell::SplitMenuShell;
use dioxus::prelude::*;
use poly_client::{Channel, ChannelType};
use poly_ui_macros::{context_menu, ui_action};

use super::Route;

// ── Shared helper ────────────────────────────────────────────────────────────

pub(super) fn restore_dm_chat(
    dm_id: String,
    account_id: String,
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<ChatLists>,
    chat_view_state: BatchedSignal<ChatViewState>,
) {
    let already_set = chat_view_state
        .peek()
        .current_channel
        .as_ref()
        .is_some_and(|ch| ch.id == dm_id);
    if already_set {
        return;
    }

    let channel = {
        let cl = chat_lists.peek();
        cl.dm_channels
            .iter()
            .find(|dm| dm.id == dm_id && dm.account_id == account_id)
            .map(|dm| Channel {
                id: dm.id.clone(),
                name: dm.user.display_name.clone(),
                channel_type: ChannelType::Text,
                server_id: String::new(),
                unread_count: dm.unread_count,
                mention_count: 0,
                last_message_id: None,
                forum_tags: None,
                parent_channel_id: None,
                thread_metadata: None,
            })
            .or_else(|| {
                cl.groups
                    .iter()
                    .find(|g| g.id == dm_id && g.account_id == account_id)
                    .map(|g| {
                        let name = g.name.clone().unwrap_or_else(|| {
                            g.members
                                .iter()
                                .map(|m| m.display_name.clone())
                                .collect::<Vec<_>>()
                                .join(", ")
                        });
                        Channel {
                            id: g.id.clone(),
                            name,
                            channel_type: ChannelType::Text,
                            server_id: String::new(),
                            unread_count: 0,
                            mention_count: 0,
                            last_message_id: None,
                            forum_tags: None,
                            parent_channel_id: None,
                            thread_metadata: None,
                        }
                    })
            })
    };
    if let Some(ch) = channel {
        // Single write guard — batching current_channel + current_server into
        // one guard so Dioxus schedules one re-render, not two.
        chat_view_state.batch(move |cv| {
            cv.current_channel = Some(ch);
            cv.current_server = None;
        });
    }

    spawn(async move {
        // Fire an initial reset cascade so the UI paints a loading state
        // before we await the backend.
        chat_view_state.batch(|cv| {
            cv.loading = true;
            cv.set_messages(Vec::new());
            cv.members = Vec::new();
        });

        let unread_count = chat_view_state
            .peek()
            .current_channel
            .as_ref()
            .filter(|channel| channel.id == dm_id)
            .map_or(0, |channel| channel.unread_count);

        let backend_arc = client_manager.peek().get_backend(&account_id);
        let Some(backend_arc) = backend_arc else {
            chat_view_state.batch(|cv| cv.loading = false);
            return;
        };

        // tokio::time::timeout uses Instant::now() which panics on
        // wasm32-unknown-unknown ("time not implemented on this platform").
        // The executor is single-threaded on web so plain .await is fine.
        let guard = backend_arc.read().await;
        let messages = guard
            .get_messages(&dm_id, initial_message_query(unread_count))
            .await
            .ok();
        let members = guard.get_channel_members(&dm_id).await.ok();
        drop(guard);

        // ONE terminal cascade for the whole fetch.
        let mut pending = chat_view_state.pending_update();
        if let Some(msgs) = messages {
            pending.set(move |cv| cv.set_messages(msgs));
            request_restore_scroll_position_or_bottom(&dm_id);
        }
        if let Some(mbrs) = members {
            pending.set(move |cv| cv.members = mbrs);
        }
        pending.set(|cv| cv.loading = false);
        pending.apply();
    });
}

// ── Layout: DMs ──────────────────────────────────────────────────────────────

/// Layout wrapper for DM views — provides the channel list panel.
///
/// Persists ChannelList state (search filter, scroll position) across
/// DmsHome ↔ DmChat navigation since the layout stays mounted.
///
/// VoiceBar and AccountBar share a `voice-account-footer` wrapper that inherits
/// the same `margin-left: -72px` trick as the old account-bar standalone, so
/// both panels extend to cover the favourites sidebar column.
// DECISION(V-1): VoiceBar + AccountBar share voice-account-footer for correct alignment.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DmsLayout() -> Element {
    rsx! {
        SplitMenuShell {
            root_class: "account-view-main".to_string(),
            sidebar_class: "channel-list-wrapper".to_string(),
            content_class: String::new(),
            sidebar: rsx! {
                ClientSidebar {}
                VoiceAccountFooter {}
            },
            content: rsx! {
                Outlet::<Route> {}
            },
        }
    }
}

// ── Route pages ──────────────────────────────────────────────────────────────

/// DM home — placeholder when no conversation is selected.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DmsHome(backend: String, instance_id: String, account_id: String) -> Element {
    let nav_state: BatchedSignal<NavState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let nav = navigator();
    // Capability guard: backends without DMs (HN, Lemmy, GitHub) render an
    // unsupported-feature placeholder in place. We must NOT redirect here:
    // a use_effect → navigator().replace() chain in the guard causes a
    // cascade (DmsHome → Root → DmsHome) that deadlocks the WASM main thread
    // when combined with sync_route_to_app_state signal writes. The
    // favorites-sidebar click handler is responsible for picking the right
    // landing route for forum/non-DM accounts.
    let caps = client_manager.peek().capabilities_for_slug(&backend);
    if matches!(caps.dms, poly_client::DmSupport::None) {
        return rsx! {
            FeatureUnsupportedPlaceholder {
                backend_slug: backend.clone(),
                feature: UnsupportedFeature::Dms,
            }
        };
    }
    let current_route = Route::DmsHome {
        backend: backend.clone(),
        instance_id: instance_id.clone(),
        account_id: account_id.clone(),
    };

    use_effect(move || {
        if crate::ui::main_layout::mobile_left_drawer_open() {
            return;
        }

        let Some(last_dm_url) = nav_state
            .read()
            .account_last_dm_routes
            .get(&account_id)
            .cloned()
        else {
            return;
        };

        if last_dm_url == format!("{current_route}") {
            return;
        }

        let Ok(route) = last_dm_url.parse::<Route>() else {
            return;
        };

        if let Route::DmChat {
            account_id: route_account_id,
            ..
        } = &route
            && route_account_id == &account_id
        {
            nav.replace(route);
        }
    });

    rsx! {
        main { class: "chat-view",
            div { class: "chat-header",
                span { class: "chat-channel-name", "{t(\"nav-dms\")}" }
            }
            div { class: "message-list",
                div { class: "message-empty",
                    div { class: "empty-wave", "💬" }
                    h3 { "{t(\"chat-select-conversation\")}" }
                }
            }
            div { class: "message-input-area",
                div { class: "message-input-disabled", "{t(\"chat-select-conversation\")}" }
            }
        }
    }
}

/// DM chat — renders a conversation with an individual or group.
///
/// Handles both click navigation (DMChannelItem sets up data before routing)
/// and URL-restore navigation (account switch, page reload) by loading data
/// in a `use_effect` when `current_channel` doesn't already match `dm_id`.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DmChat(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    let nav_state: BatchedSignal<NavState> = use_context();
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let voice_state: BatchedSignal<VoiceState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let dm_id_for_pending = dm_id.clone();
    let account_id_for_pending = account_id.clone();

    use_effect(move || {
        restore_dm_chat(dm_id.clone(), account_id.clone(), client_manager, chat_lists, chat_view_state);
    });

    // Key on the route's own (account_id, dm_id) — stable props that uniquely
    // identify this DmChat mount. The pending-call dispatch is a one-shot per
    // mount; `.take()` inside the async body consumes the pending option so
    // later renders become no-ops even if `use_spawn_once` weren't guarding us.
    use_spawn_once(
        (account_id_for_pending.clone(), dm_id_for_pending.clone()),
        move |(route_account_id, route_dm_id)| async move {
            let pending = ui_overlays.peek().pending_direct_call.clone();
            let Some(pending) = pending else {
                return;
            };
            if pending.account_id != route_account_id || pending.dm_id != route_dm_id {
                return;
            }
            #[cfg(target_arch = "wasm32")]
            {
                let mut eval = document::eval(
                    "(function(){ \
                        const ready = !!window.__polyPendingDirectCallReady; \
                        if (ready) window.__polyPendingDirectCallReady = false; \
                        dioxus.send(ready ? 'ready' : 'wait'); \
                    })()",
                );
                let status = eval.recv::<String>().await.unwrap_or_default();
                if status != "ready" {
                    return;
                }
            }

            let Some(pending) = ui_overlays.batch(|st| st.pending_direct_call.take()) else {
                return;
            };
            start_direct_call_from_active_account(
                DirectCallRequest {
                    target_user: pending.target_user,
                    start_video: pending.start_video,
                    allow_add_to_active_temporary: pending.allow_add_to_active_temporary,
                },
                nav_state,
                chat_lists,
                account_sessions,
                voice_state,
                client_manager,
            );
        },
    );

    rsx! {
        ChatView {}
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DmPendingCall(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    rsx! {
        DmPendingCallInner {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video: false,
            allow_add_to_active_temporary: false,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DmPendingVideoCall(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    rsx! {
        DmPendingCallInner {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video: true,
            allow_add_to_active_temporary: false,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DmPendingAddCall(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    rsx! {
        DmPendingCallInner {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video: false,
            allow_add_to_active_temporary: true,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DmPendingAddVideoCall(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    rsx! {
        DmPendingCallInner {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video: true,
            allow_add_to_active_temporary: true,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmPendingCallInner(
    backend: String,
    instance_id: String,
    account_id: String,
    dm_id: String,
    start_video: bool,
    allow_add_to_active_temporary: bool,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let dm_id_for_effect = dm_id.clone();
    let account_id_for_effect = account_id.clone();

    use_effect(move || {
        restore_dm_chat(
            dm_id_for_effect.clone(),
            account_id_for_effect.clone(),
            client_manager,
            chat_lists,
            chat_view_state,
        );
    });

    rsx! {
        ChatView {}
        OutgoingDirectCallOverlay {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video,
            allow_add_to_active_temporary,
        }
    }
}

/// D.3 — incoming DM call from a remote user (e.g. Discord CALL_CREATE).
///
/// Shown when `ClientEvent::IncomingCall` navigates here. Renders the
/// existing chat view in the background plus an accept / decline overlay
/// so the user can respond without losing conversation context.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DmIncomingCall(
    backend: String,
    instance_id: String,
    account_id: String,
    dm_id: String,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let voice_state: BatchedSignal<VoiceState> = use_context();
    let nav = navigator();

    let dm_id_for_effect = dm_id.clone();
    let account_id_for_effect = account_id.clone();

    // poly-lint: allow stale-effect-capture — one-shot mount effect; dm_id_for_effect
    // and account_id_for_effect are URL props that are stable for this component's
    // lifetime. Route re-navigation creates a new component scope.
    use_effect(move || {
        restore_dm_chat(
            dm_id_for_effect.clone(),
            account_id_for_effect.clone(),
            client_manager,
            chat_lists,
            chat_view_state,
        );
    });

    // Look up the caller's display name from the DM list.
    let caller_name = chat_lists
        .peek() // poly-lint: allow render-time-read — prop snapshot, subscription not needed
        .dm_channels
        .iter()
        .find(|dm| dm.account_id == account_id && dm.id == dm_id)
        .map(|dm| dm.user.display_name.clone())
        .unwrap_or_else(|| t("call-unknown-caller"));

    let backend_c = backend.clone();
    let instance_id_c = instance_id.clone();
    let account_id_c = account_id.clone();
    let dm_id_accept = dm_id.clone();
    let dm_id_decline = dm_id.clone();

    rsx! {
        // Background chat view (blurred/dimmed by CSS when overlay is active).
        ChatView {}

        // Incoming call overlay.
        div { class: "incoming-call-overlay",
            div { class: "incoming-call-card",
                div { class: "incoming-call-avatar",
                    span { "📞" }
                }
                div { class: "incoming-call-info",
                    p { class: "incoming-call-label", {t("call-incoming-label")} }
                    p { class: "incoming-call-caller", "{caller_name}" }
                }
                div { class: "incoming-call-actions",
                    // D.4 — Accept: start direct call (pseudo-backend or real transport).
                    button {
                        class: "btn btn-accept-call",
                        title: t("call-accept"),
                        onclick: move |_| {
                            let target_user = chat_lists
                                .peek()
                                .dm_channels
                                .iter()
                                .find(|dm| dm.account_id == account_id_c && dm.id == dm_id_accept)
                                .map(|dm| dm.user.clone());
                            let Some(target_user) = target_user else { return; };
                            let nav_state: BatchedSignal<crate::state::NavState> =
                                dioxus::prelude::consume_context();
                            start_direct_call_from_active_account(
                                DirectCallRequest {
                                    target_user,
                                    start_video: false,
                                    allow_add_to_active_temporary: false,
                                },
                                nav_state,
                                chat_lists,
                                account_sessions,
                                voice_state,
                                client_manager,
                            );
                        },
                        {t("call-accept")}
                    }
                    // D.4 — Decline: navigate back to the DM chat.
                    button {
                        class: "btn btn-decline-call",
                        title: t("call-decline"),
                        onclick: move |_| {
                            nav.push(Route::DmChat {
                                backend: backend_c.clone(),
                                instance_id: instance_id_c.clone(),
                                account_id: account_id.clone(),
                                dm_id: dm_id_decline.clone(),
                            });
                        },
                        {t("call-decline")}
                    }
                }
            }
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DmMediaViewerRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    dm_id: String,
    message_id: String,
    attachment_index: usize,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let overlay_channel_id = dm_id.clone();
    let overlay_message_id = message_id.clone();
    let dm_id_for_effect = dm_id.clone();
    let account_id_for_effect = account_id.clone();

    // poly-lint: allow stale-effect-capture — one-shot mount effect; dm_id_for_effect
    // and account_id_for_effect are URL props stable for this component's lifetime.
    use_effect(move || {
        restore_dm_chat(
            dm_id_for_effect.clone(),
            account_id_for_effect.clone(),
            client_manager,
            chat_lists,
            chat_view_state,
        );
    });

    rsx! {
        ChatView {}
        crate::ui::account::common::MessageMediaViewerOverlay {
            channel_id: overlay_channel_id,
            message_id: overlay_message_id,
            attachment_index,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn NewConversationRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        NewConversationView {}
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ConversationSearchRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        ConversationSearchView {}
    }
}
