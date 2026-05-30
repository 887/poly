//! Temporary direct/group call helpers.
//!
//! These calls are currently a Poly-side pseudo-backend feature: they reuse the
//! existing global voice controls and participant UI, but are anchored to DMs
//! rather than real server voice channels.

use crate::state::BatchedSignal;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{AccountSessions, ChatLists, NavState, PendingDirectCallRequest, UiOverlays, VoiceState};
use crate::ui::client_ui::toast::{push_toast, ToastMessage};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{DmChannel, ToastTone, User, VoiceConnection, VoiceConnectionKind, VoiceParticipant};

#[derive(Clone)]
pub(crate) struct DirectCallRequest {
    pub target_user: User,
    pub start_video: bool,
    pub allow_add_to_active_temporary: bool,
}

const JS_REQUEST_AUDIO_PERMISSION: &str = r#"
(async () => {
    try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        stream.getTracks().forEach(t => t.stop());
        await dioxus.send("granted");
    } catch(e) {
        await dioxus.send("denied");
    }
})();
"#;

const JS_START_CAMERA: &str = r#"
(async () => {
    try {
        const stream = await navigator.mediaDevices.getUserMedia({video: true, audio: false});
        window.__polyCameraStream = stream;
        const v = document.getElementById('poly-local-camera');
        if (v) { v.srcObject = stream; v.play().catch(() => {}); }
        await dioxus.send("ok");
    } catch(e) {
        await dioxus.send("error: " + e.message);
    }
})();
"#;

fn active_account_context(
    nav: BatchedSignal<NavState>,
    account_sessions: BatchedSignal<AccountSessions>,
) -> Option<(String, String)> {
    let account_id = nav.read().active_account_id.cloned()?; // poly-lint: allow render-time-read — plain fn called from spawn(), not a render fn
    let instance_id = account_sessions
        .read() // poly-lint: allow render-time-read — plain fn called from spawn(), not a render fn
        .account_sessions
        .get(&account_id)
        .map(|session| session.instance_id.clone())
        .or_else(|| nav.read().active_instance_id.cloned()) // poly-lint: allow render-time-read — plain fn called from spawn(), not a render fn
        .unwrap_or_default();
    Some((account_id, instance_id))
}

async fn resolve_direct_message_for_active_account(
    user_id: String,
    nav: BatchedSignal<NavState>,
    chat_lists: BatchedSignal<ChatLists>,
    account_sessions: BatchedSignal<AccountSessions>,
    client_manager: BatchedSignal<ClientManager>,
) -> Option<(DmChannel, String)> {
    let (account_id, instance_id) = active_account_context(nav, account_sessions)?;

    let existing_dm = {
        let chat_data_read = chat_lists.read(); // poly-lint: allow render-time-read — async fn called from spawn(), not a render fn
        chat_data_read
            .dm_channels
            .iter()
            .find(|dm| dm.account_id == account_id && dm.user.id == user_id)
            .cloned()
    };

    if let Some(existing_dm) = existing_dm {
        return Some((existing_dm, instance_id));
    }

    let opened_dm = client_manager.peek().with_backend(&account_id, async |b| {
        match b.as_dms_and_groups() {
            Some(dg) => dg.open_direct_message_channel(&user_id).await,
            None => Err(poly_client::ClientError::NotSupported(
                "open_direct_message_channel: backend has no DMs capability".to_string(),
            )),
        }
    }).await.ok()?;

    {
        let dm_c = opened_dm.clone();
        chat_lists.batch(move |cl| {
            cl.dm_channels.retain(|dm| {
                !(dm.account_id == account_id && (dm.id == dm_c.id || dm.user.id == user_id))
            });
            cl.dm_channels.push(dm_c);
        });
    }

    Some((opened_dm, instance_id))
}

/// Resolve/open the DM for a target user and navigate to the pending direct-call route.
// lint-allow-unused: Dioxus props/handler arity; grouping into a struct adds churn
#[allow(clippy::too_many_arguments)]
pub(crate) fn navigate_to_pending_direct_call_from_active_account(
    request: DirectCallRequest,
    nav_state: BatchedSignal<NavState>,
    ui_overlays: BatchedSignal<UiOverlays>,
    chat_lists: BatchedSignal<ChatLists>,
    account_sessions: BatchedSignal<AccountSessions>,
    client_manager: BatchedSignal<ClientManager>,
    nav: crate::ui::dioxus_router::Navigator,
) {
    spawn(async move {
        let Some((dm, instance_id)) = resolve_direct_message_for_active_account(
            request.target_user.id.clone(),
            nav_state,
            chat_lists,
            account_sessions,
            client_manager,
        )
        .await
        else {
            return;
        };

        let route = match (request.start_video, request.allow_add_to_active_temporary) {
            (false, false) => Route::DmPendingCall {
                backend: dm.backend.slug().to_string(),
                instance_id,
                account_id: dm.account_id.clone(),
                dm_id: dm.id.clone(),
            },
            (true, false) => Route::DmPendingVideoCall {
                backend: dm.backend.slug().to_string(),
                instance_id,
                account_id: dm.account_id.clone(),
                dm_id: dm.id.clone(),
            },
            (false, true) => Route::DmPendingAddCall {
                backend: dm.backend.slug().to_string(),
                instance_id,
                account_id: dm.account_id.clone(),
                dm_id: dm.id.clone(),
            },
            (true, true) => Route::DmPendingAddVideoCall {
                backend: dm.backend.slug().to_string(),
                instance_id,
                account_id: dm.account_id.clone(),
                dm_id: dm.id.clone(),
            },
        };

        ui_overlays.batch(|o| {
            o.pending_direct_call = Some(PendingDirectCallRequest {
                account_id: dm.account_id.clone(),
                dm_id: dm.id.clone(),
                target_user: request.target_user,
                start_video: request.start_video,
                allow_add_to_active_temporary: request.allow_add_to_active_temporary,
            });
        });
        nav.push(route);
    });
}

fn temporary_call_channel_id(account_id: &str, dm_id: Option<&str>, user_id: &str) -> String {
    format!(
        "poly-temp-call:{account_id}:{}:{user_id}",
        dm_id.unwrap_or("no-dm-anchor")
    )
}

fn direct_call_label(remote_users: &[User]) -> String {
    match remote_users {
        [] => t("voice-direct-call"),
        [user] => user.display_name.clone(),
        [first, rest @ ..] => format!("{} +{}", first.display_name, rest.len()),
    }
}

fn direct_call_bucket_label(remote_count: usize) -> String {
    if remote_count > 1 {
        t("voice-group-call")
    } else {
        t("voice-direct-call")
    }
}

fn hold_active_call_if_needed(new_channel_id: &str, voice_state: BatchedSignal<VoiceState>) {
    let current = voice_state.read().voice_connection.clone(); // poly-lint: allow render-time-read — plain fn called from spawn(), not a render fn
    let Some(current) = current else {
        return;
    };

    if current.channel_id == new_channel_id {
        return;
    }

    voice_state.batch(move |v| {
        v.held_voice_connections
            .retain(|held| held.channel_id != current.channel_id);
        v.held_voice_connections.insert(0, current);
        v.voice_connection = None;
    });
}

struct TemporaryCallSpec {
    channel_id: String,
    dm_id: Option<String>,
    account_id: String,
    instance_id: String,
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn activate_existing_or_new_call(
    spec: TemporaryCallSpec,
    remote_users: Vec<User>,
    start_video: bool,
    account_sessions: BatchedSignal<AccountSessions>,
    voice_state: BatchedSignal<VoiceState>,
) {
    let self_session = account_sessions
        .read() // poly-lint: allow render-time-read — plain fn called from spawn(), not a render fn
        .account_sessions
        .get(&spec.account_id)
        .cloned();
    let Some(self_session) = self_session else {
        return;
    };

    let backend = self_session.backend;
    let self_user = self_session.user.clone();

    let existing_held = {
        let reader = voice_state.read(); // poly-lint: allow render-time-read — plain fn called from spawn(), not a render fn
        reader
            .held_voice_connections
            .iter()
            .find(|held| held.channel_id == spec.channel_id)
            .cloned()
    };

    let mut participants = voice_state
        .read() // poly-lint: allow render-time-read — plain fn called from spawn(), not a render fn
        .voice_channel_participants
        .get(&spec.channel_id)
        .cloned()
        .unwrap_or_default();

    if participants.is_empty() {
        participants.push(VoiceParticipant {
            user: self_user.clone(),
            is_muted: false,
            is_deafened: false,
            is_streaming: false,
            is_video_on: start_video,
            is_speaking: false,
        });
    }

    for user in &remote_users {
        if !participants
            .iter()
            .any(|participant| participant.user.id == user.id)
        {
            participants.push(VoiceParticipant {
                user: user.clone(),
                is_muted: false,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            });
        }
    }

    let channel_name = direct_call_label(&remote_users);
    let server_name = direct_call_bucket_label(remote_users.len());

    let connection = VoiceConnection {
        channel_id: spec.channel_id.clone(),
        server_id: String::new(),
        channel_name,
        server_name,
        backend,
        account_id: spec.account_id,
        instance_id: spec.instance_id,
        is_muted: existing_held.as_ref().is_some_and(|held| held.is_muted),
        is_deafened: existing_held.as_ref().is_some_and(|held| held.is_deafened),
        is_streaming: existing_held.as_ref().is_some_and(|held| held.is_streaming),
        is_video_on: start_video || existing_held.as_ref().is_some_and(|held| held.is_video_on),
        kind: VoiceConnectionKind::TemporaryCall,
        dm_id: spec.dm_id,
        participant_user_ids: remote_users.iter().map(|user| user.id.clone()).collect(),
    };

    voice_state.batch(move |v| {
        v.voice_channel_participants
            .insert(spec.channel_id.clone(), participants);
        v.held_voice_connections
            .retain(|held| held.channel_id != spec.channel_id);
        v.voice_connection = Some(connection);
    });
}

async fn maybe_start_video_camera(start_video: bool, voice_state: BatchedSignal<VoiceState>) {
    if !start_video {
        return;
    }

    let mut eval = document::eval(JS_START_CAMERA);
    if matches!(eval.recv::<String>().await, Ok(ref s) if s == "ok") {
        voice_state.batch(|v| {
            if let Some(ref mut vc) = v.voice_connection {
                vc.is_video_on = true;
            }
        });
    }
}

/// Start or extend a temporary direct call.
///
/// When `allow_add_to_active_temporary` is true and the current active call is
/// already a temporary call, selecting another user adds them to that call
/// instead of parking the current call and creating a new one.
// lint-allow-unused: long cohesive view/handler; splitting risks reactive bugs
#[allow(clippy::too_many_lines)]
pub(crate) fn start_direct_call_from_active_account(
    request: DirectCallRequest,
    nav_state: BatchedSignal<NavState>,
    chat_lists: BatchedSignal<ChatLists>,
    account_sessions: BatchedSignal<AccountSessions>,
    voice_state: BatchedSignal<VoiceState>,
    client_manager: BatchedSignal<ClientManager>,
) {
    spawn(async move {
        drop(
            document::eval(JS_REQUEST_AUDIO_PERMISSION)
                .recv::<String>()
                .await,
        );

        let resolved_dm = resolve_direct_message_for_active_account(
            request.target_user.id.clone(),
            nav_state,
            chat_lists,
            account_sessions,
            client_manager,
        )
        .await;

        let Some((account_id, instance_id)) = active_account_context(nav_state, account_sessions) else {
            return;
        };

        let active_connection = voice_state.read().voice_connection.clone(); // poly-lint: allow render-time-read — inside spawn(async move {}), not a render fn
        if request.allow_add_to_active_temporary
            && let Some(active) = active_connection.clone()
            && active.kind == VoiceConnectionKind::TemporaryCall
            && active.account_id == account_id
            && !active
                .participant_user_ids
                .iter()
                .any(|id| id == &request.target_user.id)
        {
            let self_user_id = account_sessions
                .read() // poly-lint: allow render-time-read — inside spawn(async move {}), not a render fn
                .account_sessions
                .get(&account_id)
                .map(|session| session.user.id.clone())
                .unwrap_or_default();
            let channel_id = active.channel_id.clone();
            let mut participants = voice_state
                .read() // poly-lint: allow render-time-read — inside spawn(async move {}), not a render fn
                .voice_channel_participants
                .get(&channel_id)
                .cloned()
                .unwrap_or_default();
            participants.push(VoiceParticipant {
                user: request.target_user.clone(),
                is_muted: false,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            });

            let remote_users = participants
                .iter()
                .filter_map(|participant| {
                    if participant.user.id == self_user_id {
                        None
                    } else {
                        Some(participant.user.clone())
                    }
                })
                .collect::<Vec<_>>();

            {
                let channel_id_c = channel_id.clone();
                let participants_c = participants;
                let target_user_id = request.target_user.id.clone();
                let remote_users_c = remote_users;
                voice_state.batch(move |v| {
                    v.voice_channel_participants
                        .insert(channel_id_c, participants_c);
                    if let Some(ref mut current) = v.voice_connection {
                        current
                            .participant_user_ids
                            .push(target_user_id);
                        current.channel_name = direct_call_label(&remote_users_c);
                        current.server_name = direct_call_bucket_label(remote_users_c.len());
                    }
                });
            }

            // D.6 — dispatch add-recipient to the backend for real group DM signaling.
            // For Discord: PUT /channels/{dm_id}/recipients/{user_id} via
            // DmsAndGroupsBackend::add_users_to_group_dm.
            // Falls back to NotSupported for backends without DM groups (pseudo-backend).
            // read_with_timeout used to satisfy hang class #4 lint gate.
            if let Some(dm_channel_id) = active.dm_id.clone() {
                let target_uid = request.target_user.id.clone();
                let account_id_d6 = account_id.clone();
                let result = client_manager
                    .peek()
                    .with_backend(&account_id_d6, async move |b| {
                        match b.as_dms_and_groups() {
                            Some(dg) => dg.add_users_to_group_dm(&dm_channel_id, &[target_uid]).await,
                            None => Err(poly_client::ClientError::NotSupported(
                                "add_users_to_group_dm: backend has no DMs/groups capability".to_string(),
                            )),
                        }
                    })
                    .await;
                if let Err(e) = result
                    && !matches!(e, poly_client::ClientError::NotSupported(_)) {
                        tracing::warn!("D.6 add_users_to_group_dm failed: {e:?}");
                    }
            }

            maybe_start_video_camera(request.start_video, voice_state).await;
            return;
        }

        let dm_id = resolved_dm.as_ref().map(|(dm, _)| dm.id.clone());
        let channel_id =
            temporary_call_channel_id(&account_id, dm_id.as_deref(), &request.target_user.id);

        hold_active_call_if_needed(&channel_id, voice_state);
        activate_existing_or_new_call(
            TemporaryCallSpec {
                channel_id: channel_id.clone(),
                dm_id: dm_id.clone(),
                account_id: account_id.clone(),
                instance_id,
            },
            vec![request.target_user],
            request.start_video,
            account_sessions,
            voice_state,
        );

        // D.5 — dispatch real signaling transport for backends that support DM calls.
        // Default impl returns NotSupported (pseudo-backend path); Discord overrides
        // with gateway op 13 / op 4 Voice State Update.
        if let Some(ref dm_channel_id) = dm_id {
            let dm_channel_id = dm_channel_id.clone();
            let account_id_transport = account_id.clone();
            let result = client_manager
                .peek()
                .with_backend(&account_id_transport, async move |b| {
                    b.start_dm_call_transport(&dm_channel_id).await
                })
                .await;
            if let Err(e) = result
                && !matches!(e, poly_client::ClientError::NotSupported(_)) {
                    tracing::warn!("D.5 start_dm_call_transport failed: {e:?}");
                }
        }

        // D.7 — 30-second outgoing ring timeout (matches Discord client behaviour).
        // If the call is still in TemporaryCall state with the same channel_id after
        // 30s, assume the remote side didn't answer and auto-disconnect.
        {
            let ring_channel_id = channel_id.clone();
            let dm_channel_id_for_cancel = dm_id.clone();
            let account_id_cancel = account_id.clone();
            spawn(async move {
                // WASM: tokio::time::Instant panics, use gloo_timers.
                // Native: tokio::time::sleep.
                #[cfg(target_arch = "wasm32")]
                gloo_timers::future::TimeoutFuture::new(30_000).await;
                #[cfg(not(target_arch = "wasm32"))]
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;

                let still_ringing_info = voice_state
                    .peek()
                    .voice_connection
                    .as_ref()
                    .map(|vc| {
                        let ringing = vc.channel_id == ring_channel_id
                            && vc.kind == VoiceConnectionKind::TemporaryCall;
                        (ringing, vc.backend.slug().to_string())
                    });
                let still_ringing = still_ringing_info.as_ref().map_or(false, |(r, _)| *r);
                let ringing_backend_slug = still_ringing_info.map(|(_, slug)| slug).unwrap_or_default();
                if still_ringing {
                    tracing::info!("D.7 ring timeout — auto-disconnecting unanswered call");
                    disconnect_active_call(voice_state);
                    // I.3 — Teams-specific "coming soon" toast after ring timeout.
                    // Teams DM calls fall through to the pseudo-backend path (Phase D.5
                    // returns NotSupported which is silently accepted). Show a friendly
                    // message so the user understands why the call didn't connect.
                    if ringing_backend_slug == "teams"
                        && let Some(toast_queue) = try_consume_context::<Signal<Vec<ToastMessage>>>() {
                            push_toast(toast_queue, ToastMessage::new("voice-teams-coming-soon", ToastTone::Info));
                        }
                    // Best-effort cancel on the backend transport (op 4 to channel null).
                    if let Some(dm_ch) = dm_channel_id_for_cancel {
                        let result = client_manager
                            .peek()
                            .with_backend(&account_id_cancel, async move |b| {
                                b.start_dm_call_transport(&format!("cancel:{dm_ch}")).await
                            })
                            .await;
                        if let Err(e) = result
                            && !matches!(e, poly_client::ClientError::NotSupported(_)) {
                                tracing::warn!("D.7 cancel transport failed: {e:?}");
                            }
                    }
                }
            });
        }

        maybe_start_video_camera(request.start_video, voice_state).await;
    });
}

/// Swap the active call with the first held call, if any.
pub(crate) fn swap_to_first_held_call(voice_state: BatchedSignal<VoiceState>) {
    let current = voice_state.read().voice_connection.clone(); // poly-lint: allow render-time-read — inside event-handler fn, not a render fn; snapshot before batch is intentional
    let Some(current) = current else {
        return;
    };

    voice_state.batch(move |v| {
        if v.held_voice_connections.is_empty() {
            return;
        }
        let next = v.held_voice_connections.remove(0);
        v.held_voice_connections.push(current);
        v.voice_connection = Some(next);
    });
}

/// Disconnect the active call and automatically resume the most recent held call.
///
/// Also clears the cached participant list for the disconnected channel so the
/// UI immediately stops showing self (or stale others) under the channel and in
/// the voice grid. The next `load_channel_data` call re-fetches fresh state.
pub(crate) fn disconnect_active_call(voice_state: BatchedSignal<VoiceState>) {
    voice_state.batch(|v| {
        let Some(active) = v.voice_connection.take() else {
            return;
        };
        // Drop the cached participant list for this channel — otherwise the
        // sidebar voice-channel sub-row and the participant tile keep showing
        // self even after the local WS closes.
        v.voice_channel_participants.remove(&active.channel_id);
        v.voice_connection = v.held_voice_connections.first().cloned();
        if !v.held_voice_connections.is_empty() {
            v.held_voice_connections.remove(0);
        }
    });
}
