//! Voice/Video channel view — participant tiles, screen share, and floating controls.
//!
//! Common implementation shared across all messenger backends.
//!
//! When a voice or video channel is selected in the channel list, this
//! component replaces the normal ChatView. It shows:
//! - Channel name + participant count in a header
//! - Responsive grid of participant tiles (avatar, name, status icons)
//! - Local screen share area (full-width) when user is streaming
//! - "Join Voice"/"Join Video" button when not connected
//! - `VoiceChatBar` — floating bottom-center control bar when connected
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.14): Voice/Video channel view

use super::direct_call::disconnect_active_call;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData};
use crate::ui::account::common::chat_history::remember_message_list_scroll_position;
use crate::ui::account::common::user_profile_modal::open_user_profile;
use dioxus::prelude::*;
use poly_client::{ChannelType, VoiceConnectionKind, VoiceParticipant};

// ── JS snippets ───────────────────────────────────────────────────────────────

/// Request microphone permission before joining a voice channel.
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

/// Stop all active media streams on disconnect.
const JS_STOP_ALL_STREAMS: &str = r#"
['__polyCameraStream', '__polyScreenStream'].forEach(k => {
    if (window[k]) { window[k].getTracks().forEach(t => t.stop()); window[k] = null; }
});
['poly-local-camera', 'poly-local-screen', 'poly-screenshare-main'].forEach(id => {
    const v = document.getElementById(id);
    if (v) v.srcObject = null;
});
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

const JS_STOP_CAMERA: &str = r#"
if (window.__polyCameraStream) {
    window.__polyCameraStream.getTracks().forEach(t => t.stop());
    window.__polyCameraStream = null;
}
['poly-local-camera'].forEach(id => {
    const v = document.getElementById(id);
    if (v) v.srcObject = null;
});
"#;

const JS_START_SCREEN: &str = r#"
(async () => {
    try {
        const stream = await navigator.mediaDevices.getDisplayMedia({video: true, audio: false});
        window.__polyScreenStream = stream;
        ['poly-local-screen', 'poly-screenshare-main'].forEach(id => {
            const v = document.getElementById(id);
            if (v) { v.srcObject = stream; v.play().catch(() => {}); }
        });
        await dioxus.send("ok");
    } catch(e) {
        await dioxus.send("error: " + e.message);
    }
})();
"#;

const JS_STOP_SCREEN: &str = r#"
if (window.__polyScreenStream) {
    window.__polyScreenStream.getTracks().forEach(t => t.stop());
    window.__polyScreenStream = null;
}
['poly-local-screen', 'poly-screenshare-main'].forEach(id => {
    const v = document.getElementById(id);
    if (v) v.srcObject = null;
});
"#;

/// Attach local screen stream to the main voice-view element after mount.
const JS_ATTACH_SCREEN_TO_MAIN: &str = r#"
const v = document.getElementById('poly-screenshare-main');
if (v && window.__polyScreenStream) {
    v.srcObject = window.__polyScreenStream;
    v.play().catch(() => {});
}
"#;

// ─── Async join helper ────────────────────────────────────────────────────────

/// Join a voice channel.
///
/// 1. Requests microphone permission via `getUserMedia` (browser pop-up).
/// 2. Disconnects from any existing voice channel on any account.
/// 3. Fetches participants from the backend.
/// 4. Adds the local user to the participant list.
/// 5. Sets `ChatData.voice_connection`.
// DECISION(V-join-audio): Request audio permission on join so the browser
// prompt appears at the right time, not mid-conversation.
// DECISION(V-join-disconnect): Joining a new channel auto-disconnects the
// previous one (same behaviour as Discord/Stoat).
async fn join_voice_channel(
    channel_id: String,
    current_channel: Option<poly_client::Channel>,
    current_server: Option<poly_client::Server>,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    mut app_state: Signal<AppState>,
) {
    // Step 1: Request microphone permission so browser shows the prompt now.
    let mut perm_eval = document::eval(JS_REQUEST_AUDIO_PERMISSION);
    let _ = perm_eval.recv::<String>().await; // proceed regardless of grant/deny

    // Step 2: Disconnect from any active voice channel before joining a new one.
    if chat_data.read().voice_connection.is_some() {
        let _ = document::eval(JS_STOP_ALL_STREAMS);
        chat_data.write().voice_connection = None;
    }

    let server_id = app_state.read().nav.selected_server.clone();
    let Some(server_id) = server_id else { return };

    let backend_info = client_manager.read().get_backend_for_server(&server_id);
    let Some((voice_account_id, backend)) = backend_info else {
        return;
    };

    let voice_backend = current_server
        .as_ref()
        .map(|s| s.backend.clone())
        .unwrap_or(poly_client::BackendType::from("demo"));

    // Fetch current participants from backend
    let mut participants = {
        let guard = backend.read().await;
        guard
            .get_voice_participants(&channel_id)
            .await
            .unwrap_or_default()
    };

    // Add local (self) user if not already in the list
    let self_session = chat_data
        .read()
        .account_sessions
        .get(&voice_account_id)
        .cloned();
    if let Some(ref session) = self_session
        && !participants.iter().any(|p| p.user.id == session.user.id)
    {
        participants.push(poly_client::VoiceParticipant {
            user: session.user.clone(),
            is_muted: false,
            is_deafened: false,
            is_streaming: false,
            is_video_on: false,
            is_speaking: false,
        });
    }

    chat_data
        .write()
        .voice_channel_participants
        .insert(channel_id.clone(), participants);

    let voice_instance_id = chat_data
        .read()
        .account_sessions
        .get(&voice_account_id)
        .map(|s| s.instance_id.clone())
        .unwrap_or_default();

    chat_data.write().voice_connection = Some(poly_client::VoiceConnection {
        channel_id: channel_id.clone(),
        server_id: current_server
            .as_ref()
            .map(|s| s.id.clone())
            .unwrap_or_default(),
        channel_name: current_channel
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default(),
        server_name: current_server
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        backend: voice_backend,
        account_id: voice_account_id,
        instance_id: voice_instance_id,
        is_muted: false,
        is_deafened: false,
        is_streaming: false,
        is_video_on: false,
        kind: VoiceConnectionKind::ServerChannel,
        dm_id: None,
        participant_user_ids: Vec::new(),
    });

    if let Some(previous_channel_id) = app_state.read().nav.selected_channel.clone() {
        remember_message_list_scroll_position(&previous_channel_id);
    }
    app_state.write().nav.selected_channel = Some(channel_id);
}

// ─── Public component ─────────────────────────────────────────────────────────

/// Voice channel view root component.
///
/// Renders the full voice/video call experience including the floating
/// `VoiceChatBar` when connected and the local screen share area when streaming.
#[rustfmt::skip]
#[component]
pub fn VoiceChannelView() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let app_state: Signal<AppState> = use_context();

    let current_channel = chat_data.read().current_channel.clone();
    let current_server = chat_data.read().current_server.clone();
    let channel_id = app_state.read().nav.selected_channel.clone();

    let participants = channel_id
        .as_deref()
        .and_then(|cid| {
            chat_data
                .read()
                .voice_channel_participants
                .get(cid)
                .cloned()
        })
        .unwrap_or_default();

    let voice_conn = chat_data.read().voice_connection.clone();
    let is_connected = voice_conn
        .as_ref()
        .is_some_and(|vc| vc.channel_id == channel_id.clone().unwrap_or_default());
    let is_local_streaming = voice_conn.as_ref().is_some_and(|vc| vc.is_streaming);

    let participant_count = participants.len();

    let channel_type = current_channel
        .as_ref()
        .map(|c| c.channel_type)
        .unwrap_or(ChannelType::Voice);

    let type_icon = match channel_type {
        ChannelType::Voice => "🔊",
        ChannelType::Video => "📹",
        ChannelType::Text => "#",
        ChannelType::Forum | ChannelType::HackerNews => "📋",
        ChannelType::Code => "📁",
    };

    rsx! {
        main { class: "voice-view",
            VoiceHeader {
                current_channel: current_channel.clone(),
                current_server: current_server.clone(),
                type_icon,
                participant_count,
            }
            // Local screen share takes priority — show full-width when streaming
            if is_connected && is_local_streaming {
                VoiceScreenShareArea {}
            }
            VoiceParticipantGrid {
                participants: participants.clone(),
                is_connected,
                channel_type,
            }
            // Join button — only when not connected
            if !is_connected {
                VoiceJoinButton {
                    channel_id: channel_id.clone(),
                    current_channel: current_channel.clone(),
                    current_server: current_server.clone(),
                    channel_type,
                    chat_data,
                    client_manager,
                    app_state,
                }
            }
            // Floating control bar — only when connected
            if is_connected {
                VoiceChatBar { chat_data }
            }
        }
    }
}

// ─── Sub-components ───────────────────────────────────────────────────────────

/// Header bar showing channel name, backend badge, and participant count.
#[rustfmt::skip]
#[component]
fn VoiceHeader(
    current_channel: Option<poly_client::Channel>,
    current_server: Option<poly_client::Server>,
    type_icon: &'static str,
    participant_count: usize,
) -> Element {
    rsx! {
        div { class: "voice-header",
            if let Some(ref ch) = current_channel {
                span { class: "voice-channel-icon", "{type_icon}" }
                span { class: "voice-channel-name", "{ch.name}" }
                if let Some(ref server) = current_server {
                    span { class: "voice-source-badge",
                        "{backend_badge(&server.backend)} {server.backend.display_name()}"
                    }
                }
                span { class: "voice-participant-count", "👥 {participant_count}" }
            } else {
                span { class: "voice-channel-name", "{t(\"voice-no-channel\")}" }
            }
        }
    }
}

/// Full-width screen share area shown when the local user is streaming.
///
/// The `<video id="poly-screenshare-main">` element is attached via JS on
/// mount — the same `__polyScreenStream` object that drives the sidebar
/// preview also feeds this element.
// DECISION(V-screenshare-main): Reuse __polyScreenStream for the big view
// video element so no extra capture is needed.
#[rustfmt::skip]
#[component]
fn VoiceScreenShareArea() -> Element {
    rsx! {
        div { class: "voice-screenshare-area",
            div { class: "voice-screenshare-header",
                svg {
                    class: "voice-icon-svg voice-icon-svg--screenshare-label",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    rect {
                        x: "2",
                        y: "3",
                        width: "20",
                        height: "14",
                        rx: "2",
                    }
                    line {
                        x1: "8",
                        y1: "21",
                        x2: "16",
                        y2: "21",
                    }
                    line {
                        x1: "12",
                        y1: "17",
                        x2: "12",
                        y2: "21",
                    }
                }
                span { "{t(\"voice-screen-sharing\")}" }
            }
            video {
                id: "poly-screenshare-main",
                class: "voice-screenshare-video",
                autoplay: true,
                muted: true,
                // Reattach stream after mount — JS sets srcObject once element is in DOM.
                onmounted: move |_| {
                    let _ = document::eval(JS_ATTACH_SCREEN_TO_MAIN);
                },
            }
        }
    }
}

/// Grid of voice participant tiles.
#[rustfmt::skip]
#[component]
fn VoiceParticipantGrid(
    participants: Vec<VoiceParticipant>,
    is_connected: bool,
    channel_type: ChannelType,
) -> Element {
    rsx! {
        div { class: "voice-participants-area",
            if participants.is_empty() && !is_connected {
                div { class: "voice-empty",
                    div { class: "voice-empty-icon",
                        if channel_type == ChannelType::Video {
                            "📹"
                        } else {
                            "🔊"
                        }
                    }
                    h3 { "{t(\"voice-no-one-here\")}" }
                    p { "{t(\"voice-be-first\")}" }
                }
            } else {
                div { class: "voice-participants-grid",
                    for participant in &participants {
                        VoiceTile { participant: participant.clone() }
                    }
                }
            }
        }
    }
}

/// Single participant tile in the voice grid.
#[rustfmt::skip]
#[component]
fn VoiceTile(participant: VoiceParticipant) -> Element {
    let user = &participant.user;
    let app_state: Signal<AppState> = use_context();
    let color = user_color(&user.id);
    let first_char: String = user
        .display_name
        .chars()
        .next()
        .map(|c: char| c.to_string())
        .unwrap_or_default();

    let avatar_url = user.avatar_url.clone();

    let tile_class = if participant.is_streaming {
        "voice-tile voice-tile-streaming"
    } else {
        "voice-tile"
    };
    let speaking_class = if participant.is_speaking {
        "voice-avatar speaking"
    } else {
        "voice-avatar"
    };
    let name = user.display_name.clone();
    let profile_user = participant.user.clone();
    rsx! {
        div {
            class: "{tile_class}",
            onclick: move |_| open_user_profile(app_state, profile_user.clone()),
            style: "cursor: pointer;",
            div { class: "{speaking_class}",
                if let Some(url) = &avatar_url {
                    img {
                        src: "{url}",
                        alt: "{user.display_name}",
                        class: "voice-avatar-image",
                    }
                } else {
                    div {
                        class: "voice-avatar-fallback",
                        style: "background-color: {color};",
                        "{first_char}"
                    }
                }
            }
            div { class: "voice-tile-name", "{name}" }
            div { class: "voice-tile-icons",
                if participant.is_muted {
                    span {
                        class: "voice-status-icon muted",
                        title: "{t(\"voice-muted\")}",
                        "🔇"
                    }
                }
                if participant.is_deafened {
                    span {
                        class: "voice-status-icon deafened",
                        title: "{t(\"voice-deafened\")}",
                        "🔕"
                    }
                }
                if participant.is_streaming {
                    span {
                        class: "voice-status-icon streaming",
                        title: "{t(\"voice-streaming\")}",
                        "🖥"
                    }
                }
                if participant.is_video_on {
                    span {
                        class: "voice-status-icon video-on",
                        title: "{t(\"voice-video-on\")}",
                        "📹"
                    }
                }
            }
            if participant.is_streaming {
                div { class: "voice-stream-label", "{t(\"voice-watching-screen\")}" }
            }
        }
    }
}

/// Join button — rendered when user is NOT connected.
#[rustfmt::skip]
#[component]
fn VoiceJoinButton(
    channel_id: Option<String>,
    current_channel: Option<poly_client::Channel>,
    current_server: Option<poly_client::Server>,
    channel_type: ChannelType,
    mut chat_data: Signal<ChatData>,
    client_manager: Signal<ClientManager>,
    app_state: Signal<AppState>,
) -> Element {
    rsx! {
        div { class: "voice-controls",
            button {
                class: "btn btn-voice-join",
                onclick: move |_| {
                    let Some(ref cid) = channel_id else { return };
                    let cid = cid.clone();
                    let ch = chat_data.read().current_channel.clone();
                    let srv = chat_data.read().current_server.clone();
                    spawn(async move {
                        join_voice_channel(cid, ch, srv, client_manager, chat_data, app_state)
                            .await;
                    });
                },
                if channel_type == ChannelType::Video {
                    "{t(\"voice-join-video\")}"
                } else {
                    "{t(\"voice-join-voice\")}"
                }
            }
        }
    }
}

/// Floating control bar at the bottom-center of the voice channel view.
///
/// Shows when the user IS connected. Contains: mute, deafen, camera,
/// screen share, noise-cancel, signal quality, and disconnect.
/// Mirrors the sidebar `VoiceBar` controls but styled as a floating panel.
// DECISION(V-6): VoiceChatBar duplicates sidebar controls in a larger, more
// accessible floating bar so users don't need to look at the sidebar mid-call.
#[rustfmt::skip]
#[component]
fn VoiceChatBar(mut chat_data: Signal<ChatData>) -> Element {
    let voice_conn = chat_data.read().voice_connection.clone();
    let Some(ref conn) = voice_conn else {
        return rsx! {};
    };

    let is_muted = conn.is_muted;
    let is_deafened = conn.is_deafened;
    let is_video_on = conn.is_video_on;
    let is_streaming = conn.is_streaming;
    let noise_cancel = chat_data.read().voice_media_settings.noise_cancel_enabled;

    rsx! {
        div { class: "voice-chat-bar",
            // Mute microphone
            button {
                class: if is_muted { "voice-chat-btn active" } else { "voice-chat-btn" },
                title: if is_muted { t("voice-unmute-mic") } else { t("voice-mute-mic") },
                onclick: move |_| {
                    if let Some(ref mut vc) = chat_data.write().voice_connection {
                        vc.is_muted = !vc.is_muted;
                    }
                },
                if is_muted {
                    "🔇"
                } else {
                    "🎤"
                }
            }
            // Deafen
            button {
                class: if is_deafened { "voice-chat-btn active" } else { "voice-chat-btn" },
                title: if is_deafened { t("voice-undeafen") } else { t("voice-deafen") },
                onclick: move |_| {
                    if let Some(ref mut vc) = chat_data.write().voice_connection {
                        vc.is_deafened = !vc.is_deafened;
                    }
                },
                if is_deafened {
                    "🔕"
                } else {
                    "🔊"
                }
            }
            // Camera
            button {
                class: if is_video_on { "voice-chat-btn active" } else { "voice-chat-btn" },
                title: if is_video_on { t("voice-stop-camera") } else { t("voice-camera") },
                onclick: move |_| {
                    if is_video_on {
                        let _ = document::eval(JS_STOP_CAMERA);
                        if let Some(ref mut vc) = chat_data.write().voice_connection {
                            vc.is_video_on = false;
                        }
                    } else {
                        spawn(async move {
                            let mut eval = document::eval(JS_START_CAMERA);
                            if matches!(eval.recv::<String>().await, Ok(ref s) if s == "ok")
                                && let Some(ref mut vc) = chat_data.write().voice_connection
                            {
                                vc.is_video_on = true;
                            }
                        });
                    }
                },
                "📹"
            }
            // Screen share — SVG monitor icon
            button {
                class: if is_streaming { "voice-chat-btn active" } else { "voice-chat-btn" },
                title: if is_streaming { t("voice-stop-share") } else { t("voice-screen-share") },
                onclick: move |_| {
                    if is_streaming {
                        let _ = document::eval(JS_STOP_SCREEN);
                        if let Some(ref mut vc) = chat_data.write().voice_connection {
                            vc.is_streaming = false;
                        }
                    } else {
                        spawn(async move {
                            let mut eval = document::eval(JS_START_SCREEN);
                            if matches!(eval.recv::<String>().await, Ok(ref s) if s == "ok")
                                && let Some(ref mut vc) = chat_data.write().voice_connection
                            {
                                vc.is_streaming = true;
                            }
                        });
                    }
                },
                svg {
                    class: "voice-icon-svg",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    rect {
                        x: "2",
                        y: "3",
                        width: "20",
                        height: "14",
                        rx: "2",
                    }
                    line {
                        x1: "8",
                        y1: "21",
                        x2: "16",
                        y2: "21",
                    }
                    line {
                        x1: "12",
                        y1: "17",
                        x2: "12",
                        y2: "21",
                    }
                }
            }
            // Noise cancellation toggle
            button {
                class: if noise_cancel { "voice-chat-btn active" } else { "voice-chat-btn" },
                title: if noise_cancel { t("voice-noise-cancel-on") } else { t("voice-noise-cancel-off") },
                onclick: move |_| {
                    let cur = chat_data.read().voice_media_settings.noise_cancel_enabled;
                    chat_data.write().voice_media_settings.noise_cancel_enabled = !cur;
                },
                svg {
                    class: "voice-icon-svg",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    path { d: "M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" }
                    path { d: "M19 10v2a7 7 0 0 1-14 0v-2" }
                    line {
                        x1: "12",
                        y1: "19",
                        x2: "12",
                        y2: "23",
                    }
                    line {
                        x1: "8",
                        y1: "23",
                        x2: "16",
                        y2: "23",
                    }
                }
            }
            // Signal quality divider
            div { class: "voice-chat-bar-divider" }
            // Disconnect
            button {
                class: "voice-chat-btn voice-chat-btn-hangup",
                title: "{t(\"voice-disconnect\")}",
                onclick: move |_| {
                    let _ = document::eval(JS_STOP_ALL_STREAMS);
                    disconnect_active_call(chat_data);
                },
                "📵"
            }
        }
    }
}
