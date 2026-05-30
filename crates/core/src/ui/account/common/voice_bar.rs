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
//! │ 📹  🖥  [NC]  ▌▌▌▌  📵              │  ← VoiceDockControls
//! └──────────────────────────────────────┘
//! ```
//!
//! Mute and deafen controls live in the `AccountBar` — they are NOT
//! duplicated here.  The sidebar bar only shows: camera, screen share (SVG
//! icon), noise-cancel toggle, CSS latency bar, and disconnect.
//!
//! The video preview panel is always rendered in the DOM (CSS-hidden when no
//! active streams) so that `getUserMedia`/`getDisplayMedia` JS can find the
//! video elements by ID before Rust state updates visibility.
//!
//! # 150-line component rule
//! Each `#[component]` fn body stays under 150 lines. See sub-components below.
// TODO(phase-voice-1): VoiceBar sidebar panel

use crate::state::BatchedSignal;
use super::direct_call::{disconnect_active_call, swap_to_first_held_call};
use crate::i18n::t;
use crate::state::VoiceState;
use crate::state::chat_data::user_color;
use dioxus::prelude::*;
use poly_client::VoiceConnectionKind;
use poly_ui_macros::{context_menu, ui_action};

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

const JS_STOP_CAMERA: &str = r"
if (window.__polyCameraStream) {
    window.__polyCameraStream.getTracks().forEach(t => t.stop());
    window.__polyCameraStream = null;
}
const vc = document.getElementById('poly-local-camera');
if (vc) vc.srcObject = null;
";

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

const JS_STOP_SCREEN: &str = r"
if (window.__polyScreenStream) {
    window.__polyScreenStream.getTracks().forEach(t => t.stop());
    window.__polyScreenStream = null;
}
const vs = document.getElementById('poly-local-screen');
if (vs) vs.srcObject = null;
";

const JS_STOP_ALL_STREAMS: &str = r"
['__polyCameraStream', '__polyScreenStream'].forEach(k => {
    if (window[k]) { window[k].getTracks().forEach(t => t.stop()); window[k] = null; }
});
['poly-local-camera', 'poly-local-screen'].forEach(id => {
    const v = document.getElementById(id);
    if (v) v.srcObject = null;
});
";

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
// DECISION(V-mute): Mute/deafen buttons live only in AccountBar — not duplicated here.
#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
pub fn VoiceBar() -> Element {
    let voice_state: BatchedSignal<VoiceState> = use_context();
    let voice_conn = voice_state.read().voice_connection.clone();

    // Always render the preview panel so video element IDs exist in the DOM.
    let Some(conn) = voice_conn else {
        return rsx! {
            div { class: "voice-preview-panel" }
        };
    };

    if conn.kind == VoiceConnectionKind::TemporaryCall {
        return rsx! {
            div { class: "voice-preview-panel" }
        };
    }

    let participants = voice_state
        .read()
        .voice_channel_participants
        .get(&conn.channel_id)
        .cloned()
        .unwrap_or_default();

    rsx! {
        VoicePreviewPanel { conn: conn.clone() }
        div { class: "voice-bar",
            VoiceDockInfo { conn: conn.clone() }
            VoiceDockParticipants { participants }
            VoiceDockControls { conn: conn.clone(), voice_state }
        }
    }
}

// ─── Dock sections ───────────────────────────────────────────────────────────

/// Left section: animated dot + "Voice Connected" + channel/server name.
#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn VoiceDockInfo(conn: poly_client::VoiceConnection) -> Element {
    rsx! {
        // Row layout: left column (status above channel) | right end (server + latency)
        div { class: "voice-dock-info",
            div { class: "voice-dock-left",
                div { class: "voice-bar-status",
                    span { class: "voice-bar-dot" }
                    span { class: "voice-bar-status-text", "{t(\"voice-connected\")}" }
                }
                div { class: "voice-bar-channel", title: "{conn.channel_name}",
                    span { class: "voice-bar-channel", "{conn.channel_name} / {conn.server_name}" }
                }
            }
            div { class: "voice-dock-end",
                VoiceLatencyBar {}
            }
        }
    }
}

/// Center section: horizontally scrollable row of participant mini-tiles.
#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
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
#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
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

/// Controls: camera, screen-share (SVG icon), noise-cancel toggle, latency bar, disconnect.
///
/// Mute and deafen are intentionally NOT here — they live in `AccountBar`.
// DECISION(V-2): JS eval used for getUserMedia/getDisplayMedia.
// DECISION(V-mute): Mute/deafen in AccountBar only to avoid duplication.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn VoiceDockControls(
    conn: poly_client::VoiceConnection,
    voice_state: BatchedSignal<VoiceState>,
) -> Element {
    let is_video_on = conn.is_video_on;
    let is_streaming = conn.is_streaming;
    let noise_cancel = voice_state.read().voice_media_settings.noise_cancel_enabled;
    let held_count = voice_state.read().held_voice_connections.len();

    rsx! {
        div { class: "voice-dock-controls",
            if held_count > 0 {
                button {
                    class: "voice-bar-quick-btn",
                    title: t("voice-swap-held-call"),
                    onclick: move |_| swap_to_first_held_call(voice_state),
                    "🔁"
                }
            }
            // Camera toggle
            button {
                class: if is_video_on { "voice-bar-quick-btn active" } else { "voice-bar-quick-btn" },
                title: if is_video_on { t("voice-stop-camera") } else { t("voice-camera") },
                onclick: move |_| {
                    if is_video_on {
                        let _ = document::eval(JS_STOP_CAMERA);
                        voice_state.batch(|v| {
                            if let Some(ref mut vc) = v.voice_connection {
                                vc.is_video_on = false;
                            }
                        });
                    } else {
                        spawn(async move {
                            let mut eval = document::eval(JS_START_CAMERA);
                            if matches!(eval.recv::<String>().await, Ok(ref s) if s == "ok") {
                                voice_state.batch(|v| {
                                    if let Some(ref mut vc) = v.voice_connection {
                                        vc.is_video_on = true;
                                    }
                                });
                            }
                        });
                    }
                },
                "📹"
            }
            // Screen share — SVG monitor icon
            button {
                class: if is_streaming { "voice-bar-quick-btn active" } else { "voice-bar-quick-btn" },
                title: if is_streaming { t("voice-stop-share") } else { t("voice-screen-share") },
                onclick: move |_| {
                    if is_streaming {
                        let _ = document::eval(JS_STOP_SCREEN);
                        voice_state.batch(|v| {
                            if let Some(ref mut vc) = v.voice_connection {
                                vc.is_streaming = false;
                            }
                        });
                    } else {
                        spawn(async move {
                            let mut eval = document::eval(JS_START_SCREEN);
                            if matches!(eval.recv::<String>().await, Ok(ref s) if s == "ok") {
                                voice_state.batch(|v| {
                                    if let Some(ref mut vc) = v.voice_connection {
                                        vc.is_streaming = true;
                                    }
                                });
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
            // Noise cancellation toggle — default ON
            // DECISION(V-noise): NC toggle lives in sidebar bar (not settings popup).
            button {
                class: if noise_cancel { "voice-bar-quick-btn active" } else { "voice-bar-quick-btn" },
                title: if noise_cancel { t("voice-noise-cancel-on") } else { t("voice-noise-cancel-off") },
                onclick: move |_| {
                    voice_state.batch(|v| {
                        v.voice_media_settings.noise_cancel_enabled =
                            !v.voice_media_settings.noise_cancel_enabled;
                    });
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
            // Disconnect
            button {
                class: "voice-bar-quick-btn voice-bar-hangup",
                title: "{t(\"voice-disconnect\")}",
                onclick: move |_| {
                    let _ = document::eval(JS_STOP_ALL_STREAMS);
                    disconnect_active_call(voice_state);
                },
                "📵"
            }
        }
    }
}

/// CSS-based latency bar with hover popup showing connection quality details.
///
/// Four vertical stripes grow in height left-to-right. All four lit = excellent.
/// Demo hardcodes 42 ms excellent signal at EU-West.
// DECISION(V-5): CSS bars for signal quality; hardcoded demo latency.
#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn VoiceLatencyBar() -> Element {
    let latency_ms: u32 = 42;
    let server_loc = "EU-West (demo)";
    rsx! {
        div { class: "voice-latency-container",
            div { class: "voice-latency-bar",
                div {
                    class: "voice-latency-stripe active",
                    style: "height:9px;",
                }
                div {
                    class: "voice-latency-stripe active",
                    style: "height:13px;",
                }
                div {
                    class: "voice-latency-stripe active",
                    style: "height:17px;",
                }
                div {
                    class: "voice-latency-stripe active",
                    style: "height:21px;",
                }
            }
            div { class: "voice-latency-popup",
                div { class: "voice-latency-popup-row",
                    span { class: "voice-latency-popup-label", "{t(\"voice-signal-quality\")}" }
                    span { class: "voice-latency-popup-value voice-latency-good", "{latency_ms} ms" }
                }
                div { class: "voice-latency-popup-row",
                    span { class: "voice-latency-popup-label", "{t(\"voice-server-location\")}" }
                    span { class: "voice-latency-popup-value", "{server_loc}" }
                }
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
// DECISION(V-3): Always-rendered video elements with CSS visibility control.
#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
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
                p { class: "voice-preview-label",
                    svg {
                        class: "voice-icon-svg voice-icon-svg--label",
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
                    " {t(\"voice-screen-sharing\")}"
                }
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
