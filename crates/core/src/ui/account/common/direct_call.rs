//! Temporary direct/group call helpers.
//!
//! These calls are currently a Poly-side pseudo-backend feature: they reuse the
//! existing global voice controls and participant UI, but are anchored to DMs
//! rather than real server voice channels.

use crate::state::BatchedSignal;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{AppState, ChatData, PendingDirectCallRequest};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{DmChannel, User, VoiceConnection, VoiceConnectionKind, VoiceParticipant};

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
    app_state: BatchedSignal<AppState>,
    chat_data: BatchedSignal<ChatData>,
) -> Option<(String, String)> {
    let account_id = app_state.read().nav.active_account_id.cloned()?;
    let instance_id = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .map(|session| session.instance_id.clone())
        .or_else(|| app_state.read().nav.active_instance_id.cloned())
        .unwrap_or_default();
    Some((account_id, instance_id))
}

async fn resolve_direct_message_for_active_account(
    user_id: String,
    app_state: BatchedSignal<AppState>,
    chat_data: BatchedSignal<ChatData>,
    client_manager: Signal<ClientManager>,
) -> Option<(DmChannel, String)> {
    let (account_id, instance_id) = active_account_context(app_state, chat_data)?;

    let existing_dm = {
        let chat_data_read = chat_data.read();
        chat_data_read
            .dm_channels
            .iter()
            .find(|dm| dm.account_id == account_id && dm.user.id == user_id)
            .cloned()
    };

    if let Some(existing_dm) = existing_dm {
        return Some((existing_dm, instance_id));
    }

    let backend = client_manager.read().get_backend(&account_id)?;
    let opened_dm = {
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("direct_call: backend read timed out opening DM channel");
                return None;
            }
        };
        guard.open_direct_message_channel(&user_id).await.ok()?
    };

    {
        let dm_c = opened_dm.clone();
        chat_data.batch(move |cd| {
            cd.dm_channels.retain(|dm| {
                !(dm.account_id == account_id && (dm.id == dm_c.id || dm.user.id == user_id))
            });
            cd.dm_channels.push(dm_c);
        });
    }

    Some((opened_dm, instance_id))
}

/// Resolve/open the DM for a target user and navigate to the pending direct-call route.
pub(crate) fn navigate_to_pending_direct_call_from_active_account(
    request: DirectCallRequest,
    app_state: BatchedSignal<AppState>,
    chat_data: BatchedSignal<ChatData>,
    client_manager: Signal<ClientManager>,
    nav: crate::ui::dioxus_router::Navigator,
) {
    spawn(async move {
        let Some((dm, instance_id)) = resolve_direct_message_for_active_account(
            request.target_user.id.clone(),
            app_state,
            chat_data,
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

        app_state.batch(|st| {
            st.nav.pending_direct_call = Some(PendingDirectCallRequest {
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

fn hold_active_call_if_needed(new_channel_id: &str, mut chat_data: BatchedSignal<ChatData>) {
    let current = chat_data.read().voice_connection.clone();
    let Some(current) = current else {
        return;
    };

    if current.channel_id == new_channel_id {
        return;
    }

    chat_data.batch(move |cd| {
        cd.held_voice_connections
            .retain(|held| held.channel_id != current.channel_id);
        cd.held_voice_connections.insert(0, current);
        cd.voice_connection = None;
    });
}

struct TemporaryCallSpec {
    channel_id: String,
    dm_id: Option<String>,
    account_id: String,
    instance_id: String,
}

fn activate_existing_or_new_call(
    spec: TemporaryCallSpec,
    remote_users: Vec<User>,
    start_video: bool,
    chat_data: BatchedSignal<ChatData>,
) {
    let self_session = chat_data
        .read()
        .account_sessions
        .get(&spec.account_id)
        .cloned();
    let Some(self_session) = self_session else {
        return;
    };

    let backend = self_session.backend;
    let self_user = self_session.user.clone();

    let existing_held = {
        let writer = chat_data.read();
        writer
            .held_voice_connections
            .iter()
            .find(|held| held.channel_id == spec.channel_id)
            .cloned()
    };

    let mut participants = chat_data
        .read()
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

    chat_data.batch(move |cd| {
        cd.voice_channel_participants
            .insert(spec.channel_id.clone(), participants);
        cd.held_voice_connections
            .retain(|held| held.channel_id != spec.channel_id);
        cd.voice_connection = Some(connection);
    });
}

async fn maybe_start_video_camera(start_video: bool, mut chat_data: BatchedSignal<ChatData>) {
    if !start_video {
        return;
    }

    let mut eval = document::eval(JS_START_CAMERA);
    if matches!(eval.recv::<String>().await, Ok(ref s) if s == "ok") {
        chat_data.batch(|cd| {
            if let Some(ref mut vc) = cd.voice_connection {
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
pub(crate) fn start_direct_call_from_active_account(
    request: DirectCallRequest,
    app_state: BatchedSignal<AppState>,
    chat_data: BatchedSignal<ChatData>,
    client_manager: Signal<ClientManager>,
) {
    spawn(async move {
        let _ = document::eval(JS_REQUEST_AUDIO_PERMISSION)
            .recv::<String>()
            .await;

        let resolved_dm = resolve_direct_message_for_active_account(
            request.target_user.id.clone(),
            app_state,
            chat_data,
            client_manager,
        )
        .await;

        let Some((account_id, instance_id)) = active_account_context(app_state, chat_data) else {
            return;
        };

        let active_connection = chat_data.read().voice_connection.clone();
        if request.allow_add_to_active_temporary
            && let Some(active) = active_connection.clone()
            && active.kind == VoiceConnectionKind::TemporaryCall
            && active.account_id == account_id
            && !active
                .participant_user_ids
                .iter()
                .any(|id| id == &request.target_user.id)
        {
            let self_user_id = chat_data
                .read()
                .account_sessions
                .get(&account_id)
                .map(|session| session.user.id.clone())
                .unwrap_or_default();
            let channel_id = active.channel_id.clone();
            let mut participants = chat_data
                .read()
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
                chat_data.batch(move |cd| {
                    cd.voice_channel_participants
                        .insert(channel_id_c, participants_c);
                    if let Some(ref mut current) = cd.voice_connection {
                        current
                            .participant_user_ids
                            .push(target_user_id);
                        current.channel_name = direct_call_label(&remote_users_c);
                        current.server_name = direct_call_bucket_label(remote_users_c.len());
                    }
                });
            }
            maybe_start_video_camera(request.start_video, chat_data).await;
            return;
        }

        let dm_id = resolved_dm.as_ref().map(|(dm, _)| dm.id.clone());
        let channel_id =
            temporary_call_channel_id(&account_id, dm_id.as_deref(), &request.target_user.id);

        hold_active_call_if_needed(&channel_id, chat_data);
        activate_existing_or_new_call(
            TemporaryCallSpec {
                channel_id,
                dm_id,
                account_id,
                instance_id,
            },
            vec![request.target_user],
            request.start_video,
            chat_data,
        );
        maybe_start_video_camera(request.start_video, chat_data).await;
    });
}

/// Swap the active call with the first held call, if any.
pub(crate) fn swap_to_first_held_call(mut chat_data: BatchedSignal<ChatData>) {
    let current = chat_data.read().voice_connection.clone();
    let Some(current) = current else {
        return;
    };

    chat_data.batch(move |cd| {
        if cd.held_voice_connections.is_empty() {
            return;
        }
        let next = cd.held_voice_connections.remove(0);
        cd.held_voice_connections.push(current);
        cd.voice_connection = Some(next);
    });
}

/// Disconnect the active call and automatically resume the most recent held call.
pub(crate) fn disconnect_active_call(chat_data: BatchedSignal<ChatData>) {
    chat_data.batch(|cd| {
        if cd.voice_connection.is_none() {
            return;
        }
        cd.voice_connection = cd.held_voice_connections.first().cloned();
        if !cd.held_voice_connections.is_empty() {
            cd.held_voice_connections.remove(0);
        }
    });
}
