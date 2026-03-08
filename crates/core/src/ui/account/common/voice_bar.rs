//! Voice bar — compact sidebar panel at the bottom of `channel-list-wrapper`.
//!
//! Rendered between `ChannelList` and `AccountBar` inside the channel-list
//! column.  Displays a 3-row panel:
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │ ● Voice Connected                    │  ← VoiceDockInfo (status + channel)
//! │ Reading Night / Book Club            │
//! ├──────────────────────────────────────┤
//! │ 👤  👤  👤  …                        │  ← VoiceDockParticipants (avatars)
//! ├──────────────────────────────────────┤
//! │ 🎤  🔔  📹  🖥  ⚙  📵               │  ← VoiceDockControls (media + hang)
//! └──────────────────────────────────────┘
//! ```
//!
//! Participant tiles show avatar circles only (names as tooltip) so they fit
//! within the 240 px sidebar column.
//!
//! The video preview panel is always rendered in the DOM (CSS-hidden when no
//! active streams) so that `getUserMedia`/`getDisplayMedia` JS can find the
//! video elements by ID before Rust state updates visibility.
//!
//! # 150-line component rule
//! Each `#[component]` fn body stays under 150 lines. See sub-components below.
// TODO(phase-voice-1): VoiceBar sidebar panel

use crate::i18n::t;
use crate::state::ChatData;
use crate::state::chat_data::user_color;
use dioxus::prelude::*;

// ── JS snippets stored as constants to keep fn bodies under 150 lines ──────

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
const vc = document.getElementById('poly-local-camera');
if (vc) vc.srcObject = null;
"#;

const JS_START_SCREEN: &str = r#"
(async () => {
    try {
        const stream = await navigator.mediaDevices.getDisplayMedia({video: true, audio: false});
        window.__polyScreenStream = stream;
        const v = document.getElementById('poly-local-screen');
        if (v) { v.srcObject = stream; v.play().catch(() => {}); }
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
const vs = document.getElementById('poly-local-screen');
if (vs) vs.srcObject = null;
"#;

const JS_STOP_ALL_STREAMS: &str = r#"
['__polyCameraStream', '__polyScreenStream'].forEach(k => {
    if (window[k]) { window[k].getTracks().forEach(t => t.stop()); window[k] = null; }
});
['poly-local-camera', 'poly-local-screen'].forEach(id => {
    const v = document.getElementById(id);
    if (v) v.srcObject = null;
});
"#;

const JS_ENUMERATE_DEVICES: &str = r#"
(async () => {
    try { await navigator.mediaDevices.getUserMedia({audio: true}); } catch(_) {}
    const devices = await navigator.mediaDevices.enumerateDevices();
    const inputs = devices
        .filter(d => d.kind === 'audioinput')
        .map(d => ({ id: d.deviceId, label: d.label || 'Microphone' }));
    const outputs = devices
        .filter(d => d.kind === 'audiooutput')
        .map(d => ({ id: d.deviceId, label: d.label || 'Speaker' }));
    await dioxus.send(JSON.stringify({ inputs, outputs }));
})();
"#;

const JS_TEST_MIC: &str = r#"
(async () => {
    try {
        const stream = await navigator.mediaDevices.getUserMedia({audio: true});
        const ctx = new AudioContext();
        const src = ctx.createMediaStreamSource(stream);
        const analyser = ctx.createAnalyser();
        src.connect(analyser);
        setTimeout(() => { stream.getTracks().forEach(t => t.stop()); ctx.close(); }, 3000);
    } catch(_) {}
})();
"#;

// ─── Root component ──────────────────────────────────────────────────────────

/// Compact sidebar voice panel.
///
/// Renders a 3-row panel inside `channel-list-wrapper` only when
/// `ChatData.voice_connection` is `Some`.  The video preview panel elements
/// are always in the DOM (even when disconnected) so JS can reference them by
/// ID immediately on `getUserMedia`/`getDisplayMedia` resolution — before Rust
/// re-renders.
///
/// Placed INSIDE `.channel-list-wrapper` between `ChannelList` and `AccountBar`.
// DECISION(V-1): VoiceBar stays in sidebar; compact 3-row layout with avatars + buttons.
#[component]
pub fn VoiceBar() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let voice_conn = chat_data.read().voice_connection.clone();

    // Always render the preview panel so video element IDs exist in the DOM.
    let Some(conn) = voice_conn else {
        return rsx! {
            div { class: "voice-preview-panel" }
        };
    };

    let participants = chat_data
        .read()
        .voice_channel_participants
        .get(&conn.channel_id)
        .cloned()
        .unwrap_or_default();

    let mut show_settings = use_signal(|| false);

    rsx! {
        VoicePreviewPanel { conn: conn.clone() }
        div { class: "voice-bar",
            VoiceDockInfo { conn: conn.clone() }
            VoiceDockParticipants { participants }
            VoiceDockControls { conn: conn.clone(), chat_data, show_settings }
        }
        if *show_settings.read() {
            VoiceSettingsPopup { on_close: move |_| show_settings.set(false) }
        }
    }
}

// ─── Dock sections ───────────────────────────────────────────────────────────

/// Left section: animated dot + "Voice Connected" + channel/server name.
#[component]
fn VoiceDockInfo(conn: poly_client::VoiceConnection) -> Element {
    rsx! {
        div { class: "voice-dock-info",
            div { class: "voice-bar-status",
                span { class: "voice-bar-dot" }
                span { class: "voice-bar-status-text", "{t(\"voice-connected\")}" }
            }
            div { class: "voice-bar-channel", "{conn.channel_name} / {conn.server_name}" }
        }
    }
}

/// Center section: horizontally scrollable row of participant mini-tiles.
#[component]
fn VoiceDockParticipants(participants: Vec<poly_client::VoiceParticipant>) -> Element {
    rsx! {
        div { class: "voice-dock-participants",
            for p in participants {
                VoiceDockTile { participant: p }
            }
        }
    }
}

/// Single participant mini-tile: avatar + truncated name + status icons.
#[component]
fn VoiceDockTile(participant: poly_client::VoiceParticipant) -> Element {
    let color = user_color(&participant.user.id);
    let first_char: String = participant
        .user
        .display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let name = participant.user.display_name.clone();
    let tile_class = if participant.is_speaking {
        "voice-dock-tile speaking"
    } else {
        "voice-dock-tile"
    };

    rsx! {
        div { class: "{tile_class}", title: "{name}",
            if let Some(url) = &participant.user.avatar_url {
                img { class: "voice-dock-avatar", src: "{url}", alt: "{name}" }
            } else {
                div {
                    class: "voice-dock-avatar voice-dock-avatar-fallback",
                    style: "background-color: {color};",
                    "{first_char}"
                }
            }
            span { class: "voice-dock-name", "{name}" }
            if participant.is_muted {
                span { class: "voice-dock-icon", "🔇" }
            }
            if participant.is_streaming {
                span { class: "voice-dock-icon", "🖥" }
            }
            if participant.is_video_on {
                span { class: "voice-dock-icon", "📹" }
            }
        }
    }
}

/// Right section: mute, deafen, camera, screen-share, settings, disconnect.
// DECISION(V-2): JS eval used for getUserMedia/getDisplayMedia.
#[component]
fn VoiceDockControls(
    conn: poly_client::VoiceConnection,
    mut chat_data: Signal<ChatData>,
    mut show_settings: Signal<bool>,
) -> Element {
    let is_muted = conn.is_muted;
    let is_deafened = conn.is_deafened;
    let is_video_on = conn.is_video_on;
    let is_streaming = conn.is_streaming;

    rsx! {
        div { class: "voice-dock-controls",
            button {
                class: if is_muted { "voice-bar-quick-btn active" } else { "voice-bar-quick-btn" },
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
            button {
                class: if is_deafened { "voice-bar-quick-btn active" } else { "voice-bar-quick-btn" },
                title: if is_deafened { t("voice-undeafen") } else { t("voice-deafen") },
                onclick: move |_| {
                    if let Some(ref mut vc) = chat_data.write().voice_connection {
                        vc.is_deafened = !vc.is_deafened;
                    }
                },
                if is_deafened {
                    "🔕"
                } else {
                    "🔔"
                }
            }
            button {
                class: if is_video_on { "voice-bar-quick-btn active" } else { "voice-bar-quick-btn" },
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
            button {
                class: if is_streaming { "voice-bar-quick-btn active" } else { "voice-bar-quick-btn" },
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
                "🖥"
            }
            button {
                class: if *show_settings.read() { "voice-bar-quick-btn active" } else { "voice-bar-quick-btn" },
                title: "{t(\"voice-audio-settings\")}",
                onclick: move |_| {
                    let cur = *show_settings.read();
                    show_settings.set(!cur);
                },
                "⚙"
            }
            span {
                class: "voice-bar-signal",
                title: "{t(\"voice-signal-quality\")}",
                "📶"
            }
            button {
                class: "voice-bar-quick-btn voice-bar-hangup",
                title: "{t(\"voice-disconnect\")}",
                onclick: move |_| {
                    let _ = document::eval(JS_STOP_ALL_STREAMS);
                    chat_data.write().voice_connection = None;
                },
                "📵"
            }
        }
    }
}

// ─── Video preview panel ─────────────────────────────────────────────────────

/// Local video preview for camera and screen-share feeds.
///
/// Always present in the DOM regardless of stream state so JS can find
/// `#poly-local-camera` and `#poly-local-screen` by ID immediately on
/// `getUserMedia`/`getDisplayMedia` resolution — before Rust re-renders.
///
/// CSS class `visible` on `.voice-preview-panel` and absence of `hidden` on
/// `.voice-preview-item` control visibility without DOM removal.
// DECISION(V-3): Always-rendered video elements with CSS visibility control.
#[component]
fn VoicePreviewPanel(conn: poly_client::VoiceConnection) -> Element {
    let panel_class = if conn.is_video_on || conn.is_streaming {
        "voice-preview-panel visible"
    } else {
        "voice-preview-panel"
    };

    rsx! {
        div { class: "{panel_class}",
            div { class: if conn.is_video_on { "voice-preview-item" } else { "voice-preview-item hidden" },
                p { class: "voice-preview-label", "📹 {t(\"voice-camera-preview\")}" }
                video {
                    id: "poly-local-camera",
                    class: "voice-preview-video",
                    autoplay: true,
                    muted: true,
                }
            }
            div { class: if conn.is_streaming { "voice-preview-item" } else { "voice-preview-item hidden" },
                p { class: "voice-preview-label", "🖥 {t(\"voice-screen-sharing\")}" }
                video {
                    id: "poly-local-screen",
                    class: "voice-preview-video",
                    autoplay: true,
                    muted: true,
                }
            }
        }
    }
}

// ─── Audio settings popup ────────────────────────────────────────────────────

/// Audio settings popup — opened by the ⚙ button in VoiceDockControls.
///
/// Enumerates audio devices via JS and renders mic/speaker pickers,
/// a noise-cancellation toggle, and a microphone test button.
// DECISION(V-4): nnnoiseless toggle wired to UI; actual audio worklet is Phase 3.
#[component]
fn VoiceSettingsPopup(on_close: EventHandler<()>) -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
    let noise_cancel = chat_data.read().voice_media_settings.noise_cancel_enabled;

    let mut mic_devices = use_signal::<Vec<(String, String)>>(Vec::new);
    let mut spk_devices = use_signal::<Vec<(String, String)>>(Vec::new);

    use_effect(move || {
        spawn(async move {
            let mut eval = document::eval(JS_ENUMERATE_DEVICES);
            if let Ok(json) = eval.recv::<serde_json::Value>().await {
                if let Some(arr) = json.get("inputs").and_then(|v| v.as_array()) {
                    mic_devices.set(
                        arr.iter()
                            .filter_map(|d| {
                                let id = d.get("id")?.as_str()?.to_string();
                                let label = d.get("label")?.as_str()?.to_string();
                                Some((id, label))
                            })
                            .collect(),
                    );
                }
                if let Some(arr) = json.get("outputs").and_then(|v| v.as_array()) {
                    spk_devices.set(
                        arr.iter()
                            .filter_map(|d| {
                                let id = d.get("id")?.as_str()?.to_string();
                                let label = d.get("label")?.as_str()?.to_string();
                                Some((id, label))
                            })
                            .collect(),
                    );
                }
            }
        });
    });

    rsx! {
        div { class: "voice-settings-popup",
            div { class: "voice-settings-header",
                h3 { class: "voice-settings-title", "{t(\"voice-audio-settings\")}" }
                button {
                    class: "voice-settings-close",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            // Microphone selection
            div { class: "voice-settings-section",
                label { class: "voice-settings-label", "{t(\"voice-mic-device\")}" }
                select {
                    class: "voice-settings-select",
                    onchange: move |e: Event<FormData>| {
                        let val = e.value();
                        chat_data.write().voice_media_settings.mic_device_id =
                            if val.is_empty() { None } else { Some(val) };
                    },
                    option { value: "", "{t(\"voice-default-device\")}" }
                    for (id , label) in mic_devices.read().iter() {
                        option {
                            value: "{id}",
                            selected: chat_data
                                                                                        .read()
                                                                                        .voice_media_settings
                                                                                        .mic_device_id
                                                                                        .as_deref()
                                                                                        == Some(id.as_str()),
                            "{label}"
                        }
                    }
                }
            }

            // Speaker selection
            div { class: "voice-settings-section",
                label { class: "voice-settings-label", "{t(\"voice-speaker-device\")}" }
                select {
                    class: "voice-settings-select",
                    onchange: move |e: Event<FormData>| {
                        let val = e.value();
                        chat_data.write().voice_media_settings.speaker_device_id =
                            if val.is_empty() { None } else { Some(val) };
                    },
                    option { value: "", "{t(\"voice-default-device\")}" }
                    for (id , label) in spk_devices.read().iter() {
                        option {
                            value: "{id}",
                            selected: chat_data
                                                                                        .read()
                                                                                        .voice_media_settings
                                                                                        .speaker_device_id
                                                                                        .as_deref()
                                == Some(id.as_str()),
                            "{label}"
                        }
                    }
                }
            }

            // Noise cancellation toggle
            div { class: "voice-settings-section",
                div { class: "voice-settings-row",
                    label { class: "voice-settings-label", "{t(\"voice-noise-cancel\")}" }
                    label { class: "toggle-switch",
                        input {
                            r#type: "checkbox",
                            checked: noise_cancel,
                            onchange: move |_| {
                                let cur = chat_data
                                    .read()
                                    .voice_media_settings
                                    .noise_cancel_enabled;
                                chat_data.write().voice_media_settings.noise_cancel_enabled =
                                    !cur;
                            },
                        }
                        span { class: "toggle-slider" }
                    }
                }
                p { class: "voice-settings-desc", "{t(\"voice-noise-cancel-desc\")}" }
            }

            // Microphone test
            button {
                class: "btn btn-secondary voice-settings-test-btn",
                onclick: move |_| {
                    let _ = document::eval(JS_TEST_MIC);
                },
                "{t(\"voice-test-mic\")}"
            }
        }
    }
}
