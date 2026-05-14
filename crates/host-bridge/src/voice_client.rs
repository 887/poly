//! # `VoiceBridgeClient` — typed client for `/host/voice/*`
//!
//! Available on **all targets** including `wasm32-unknown-unknown`. The browser
//! WASM client uses this to drive Discord voice through the fullstack
//! server-half's UDP + Opus machinery.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use poly_host_bridge::voice_client::{VoiceBridgeClient, VoiceConnectRequest};
//!
//! let client = VoiceBridgeClient::new("http://127.0.0.1:9333");
//!
//! let resp = client.connect(VoiceConnectRequest {
//!     backend: "discord".into(),
//!     account_id: "1234567890".into(),
//!     channel_id: "987654321".into(),
//!     ws_endpoint: "us-west1-b-discord.media.discord.gg:443".into(),
//!     ws_token: "...".into(),
//!     ws_session_id: "abc123".into(),
//!     guild_id: Some("111222333".into()),
//!     user_id: "1234567890".into(),
//! }).await?;
//!
//! let session_id = resp.session_id;
//!
//! // Stream events (speaking indicators, decoded audio PCM, H.264 NALs).
//! let events = client.subscribe_events(&session_id);
//!
//! // Send mic PCM.
//! client.send_audio(&session_id, &pcm_frame).await?;
//!
//! // Clean up.
//! client.disconnect(&session_id).await?;
//! ```
//!
//! ## Receive path
//!
//! `subscribe_events` returns a `Stream<Item = VoiceEvent>`. Remote participants'
//! decoded audio arrives as `VoiceEvent::FrameAudio { pcm_b64, .. }` — the
//! browser passes these to `WebAudioBackend::open_output().push()`. Incoming
//! H.264 is forwarded as `VoiceEvent::FrameH264 { nal_units_b64, .. }` — the
//! browser feeds these to `WebCodecs VideoDecoder.decode()` for efficient GPU
//! decoding without server-side transcoding.

// Re-export wire types so callers only import this one module.
pub use crate::voice_wire::{
    SendAudioRequest, SendAudioResponse, SendVideoRequest, SendVideoResponse,
    SetMuteRequest, SetMuteResponse, VideoFrameWire, VoiceConnectRequest,
    VoiceConnectResponse, VoiceDisconnectRequest, VoiceDisconnectResponse, VoiceEvent,
    ROUTE_VOICE_CONNECT, ROUTE_VOICE_DISCONNECT, ROUTE_VOICE_SEND_AUDIO,
    ROUTE_VOICE_SEND_VIDEO, ROUTE_VOICE_SET_MUTE,
};

use base64::Engine as _;
use futures::Stream;
use thiserror::Error;

/// Errors from [`VoiceBridgeClient`].
#[derive(Debug, Error)]
pub enum VoiceClientError {
    /// HTTP transport failure.
    #[error("voice bridge transport: {0}")]
    Transport(#[from] reqwest::Error),
    /// JSON parse/serialize failure.
    #[error("voice bridge JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The server returned `ok: false`.
    #[error("voice bridge server error: {0}")]
    Server(String),
}

/// Typed client for the `/host/voice/*` endpoints.
///
/// `Clone + Send + Sync` — share a single instance across tasks and components.
#[derive(Clone, Debug)]
pub struct VoiceBridgeClient {
    http: reqwest::Client,
    /// Base URL, e.g. `"http://127.0.0.1:9333"` or `"http://localhost:3000"`.
    /// No trailing slash.
    base_url: String,
}

impl VoiceBridgeClient {
    /// Construct a client pointing at `base_url`. No trailing slash needed.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Construct a client pointing at the default local bridge port (9333).
    ///
    /// Use this from native (non-WASM) callers like `NativeVideoBackend`.
    /// WASM callers should use `from_origin()` instead.
    #[must_use]
    pub fn default_local() -> Self {
        Self::new(crate::BRIDGE_BASE_URL)
    }

    /// On WASM, construct a client whose base URL is `window.location.origin`.
    ///
    /// Ensures requests target the same origin that served the WASM bundle
    /// (no CORS, works with all fullstack shells on any port).
    #[cfg(target_arch = "wasm32")]
    #[must_use]
    pub fn from_origin() -> Self {
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_else(|| crate::BRIDGE_BASE_URL.to_string());
        Self::new(origin)
    }

    // ── Endpoints ──────────────────────────────────────────────────────────────

    /// `POST /host/voice/connect` — open a voice session.
    ///
    /// Performs the full Discord voice WS handshake (op 8 Hello, op 0 IDENTIFY,
    /// op 2 Ready, UDP IP-discovery, op 1 SELECT PROTOCOL, op 4 SESSION
    /// DESCRIPTION) on the native side and returns a `session_id`.
    ///
    /// # Errors
    ///
    /// Returns [`VoiceClientError::Server`] when the bridge reports `ok: false`
    /// (e.g. WS connection refused, encryption mode mismatch).
    pub async fn connect(
        &self,
        req: VoiceConnectRequest,
    ) -> Result<VoiceConnectResponse, VoiceClientError> {
        let url = format!("{}{}", self.base_url, ROUTE_VOICE_CONNECT);
        let resp: VoiceConnectResponse = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(resp)
        } else {
            Err(VoiceClientError::Server(
                resp.err.unwrap_or_else(|| "voice/connect failed".into()),
            ))
        }
    }

    /// `POST /host/voice/disconnect` — tear down a voice session.
    ///
    /// Idempotent — safe to call even if the session has already been removed
    /// (e.g. orphan GC already cleaned it up).
    ///
    /// # Errors
    ///
    /// Returns `Err` only on transport / JSON failure.
    pub async fn disconnect(&self, session_id: &str) -> Result<(), VoiceClientError> {
        let url = format!("{}{}", self.base_url, ROUTE_VOICE_DISCONNECT);
        let req = VoiceDisconnectRequest {
            session_id: session_id.to_string(),
        };
        let resp: VoiceDisconnectResponse = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(())
        } else {
            Err(VoiceClientError::Server(
                resp.err.unwrap_or_else(|| "voice/disconnect failed".into()),
            ))
        }
    }

    /// `POST /host/voice/send_audio` — push PCM frames for encoding and sending.
    ///
    /// `pcm` must be 48 kHz stereo signed-16-bit (matching `AudioFormat::DISCORD_VOICE`).
    /// Frames are accumulated on the native side; 20ms Opus frames are encoded and
    /// sent as Discord RTP packets.
    ///
    /// Returns `sent_bytes = 0` when muted — callers can use this to show
    /// a "muted" indicator without separate state.
    ///
    /// # Errors
    ///
    /// Returns [`VoiceClientError::Server`] on native encode/send failure.
    pub async fn send_audio(
        &self,
        session_id: &str,
        pcm: &[i16],
    ) -> Result<SendAudioResponse, VoiceClientError> {
        let url = format!("{}{}", self.base_url, ROUTE_VOICE_SEND_AUDIO);
        let mut pcm_bytes = Vec::with_capacity(pcm.len() * 2);
        for &s in pcm {
            pcm_bytes.extend_from_slice(&s.to_le_bytes());
        }
        let pcm_b64 = base64::engine::general_purpose::STANDARD.encode(&pcm_bytes);
        let req = SendAudioRequest {
            session_id: session_id.to_string(),
            pcm_b64,
        };
        let resp: SendAudioResponse = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(resp)
        } else {
            Err(VoiceClientError::Server(
                resp.err.unwrap_or_else(|| "voice/send_audio failed".into()),
            ))
        }
    }

    /// `POST /host/voice/send_video` — push a video frame for encoding and sending.
    ///
    /// The frame is H.264-encoded native-side (same openh264 codec used by
    /// `NativeVideoEncoder`) and sent as Discord RTP over the existing voice
    /// UDP socket.
    ///
    /// # Errors
    ///
    /// Returns [`VoiceClientError::Server`] on encode/send failure.
    pub async fn send_video(
        &self,
        session_id: &str,
        frame: VideoFrameWire,
    ) -> Result<(), VoiceClientError> {
        let url = format!("{}{}", self.base_url, ROUTE_VOICE_SEND_VIDEO);
        let req = SendVideoRequest {
            session_id: session_id.to_string(),
            frame,
        };
        let resp: SendVideoResponse = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(())
        } else {
            Err(VoiceClientError::Server(
                resp.err.unwrap_or_else(|| "voice/send_video failed".into()),
            ))
        }
    }

    /// `POST /host/voice/set_mute` — update mute/deafen state on the native side.
    ///
    /// Causes the native encode loop to suppress transmission and sends Discord
    /// op 5 SPEAKING with `speaking = 0` on the voice WebSocket.
    ///
    /// # Errors
    ///
    /// Returns [`VoiceClientError::Server`] when the session is not found or
    /// the WS send fails.
    pub async fn set_mute(
        &self,
        session_id: &str,
        muted: bool,
        deafened: bool,
    ) -> Result<(), VoiceClientError> {
        let url = format!("{}{}", self.base_url, ROUTE_VOICE_SET_MUTE);
        let req = SetMuteRequest {
            session_id: session_id.to_string(),
            muted,
            deafened,
        };
        let resp: SetMuteResponse = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(())
        } else {
            Err(VoiceClientError::Server(
                resp.err.unwrap_or_else(|| "voice/set_mute failed".into()),
            ))
        }
    }

    /// `GET /host/voice/events/:session_id` — subscribe to the SSE event stream.
    ///
    /// Returns a `Stream<Item = VoiceEvent>`. Events include:
    /// - `Speaking` — speaking indicators for remote participants.
    /// - `ParticipantJoin` / `ParticipantLeave` — roster changes.
    /// - `FrameAudio` — decoded PCM from remote participants (hand to WebAudio).
    /// - `FrameH264` — encoded H.264 NALs from remote participants (hand to WebCodecs).
    /// - `Disconnected` — session terminated; stream ends after this.
    ///
    /// The stream is backed by a simple line-by-line SSE parser over a streaming
    /// HTTP response — works on both native (`reqwest` async body) and WASM
    /// (browser `fetch` streaming response).
    pub fn subscribe_events(&self, session_id: &str) -> impl Stream<Item = VoiceEvent> {
        let url = format!("{}/host/voice/events/{}", self.base_url, session_id);
        let http = self.http.clone();
        make_sse_stream(http, url)
    }

    // ── private helper ─────────────────────────────────────────────────────────

    async fn post_json<T, B>(&self, url: &str, body: &B) -> Result<T, VoiceClientError>
    where
        T: serde::de::DeserializeOwned,
        B: serde::Serialize,
    {
        let text = self
            .http
            .post(url)
            .json(body)
            .send()
            .await?
            .text()
            .await?;
        let v: T = serde_json::from_str(&text)?;
        Ok(v)
    }
}

// ── SSE stream ────────────────────────────────────────────────────────────────

fn make_sse_stream(http: reqwest::Client, url: String) -> impl Stream<Item = VoiceEvent> {
    async_stream::stream! {
        let resp = match http.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    target: "poly_host_bridge::voice_client",
                    error = %e, "SSE connect failed"
                );
                return;
            }
        };

        use futures::StreamExt;
        let mut bytes_stream = resp.bytes_stream();
        let mut line_buf = String::new();

        while let Some(chunk_result) = bytes_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        target: "poly_host_bridge::voice_client",
                        error = %e, "SSE read error"
                    );
                    break;
                }
            };

            let text = match std::str::from_utf8(&chunk) {
                Ok(t) => t,
                Err(_) => continue,
            };
            line_buf.push_str(text);

            // Process complete lines.
            while let Some(pos) = line_buf.find('\n') {
                let line: String = line_buf.drain(..=pos).collect();
                let line = line.trim_end_matches(['\n', '\r']);
                if let Some(data) = line.strip_prefix("data: ") {
                    match serde_json::from_str::<VoiceEvent>(data) {
                        Ok(ev) => {
                            let is_disconnect = matches!(ev, VoiceEvent::Disconnected { .. });
                            yield ev;
                            if is_disconnect { return; }
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "poly_host_bridge::voice_client",
                                error = %e,
                                data,
                                "failed to parse SSE VoiceEvent"
                            );
                        }
                    }
                }
            }
        }
    }
}
