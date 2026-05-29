//! # `/host/video/*` — H.264 encode/decode service
//!
//! Exposes stateful H.264 encode and decode over HTTP so every plugin and the
//! native video-backend can share a single codec implementation without each
//! linking their own copy of openh264.
//!
//! ## Why host-bridge?
//!
//! The user directive: *"if we have to ship H.264 encoding, make that part of
//! the host functions so we can do it for all plugins."*
//!
//! WASM targets (apps/web) call the endpoint via HTTP — the `video` module is
//! cfg-gated to `not(target_arch = "wasm32")` so openh264 never links into
//! the WASM bundle. Native callers (the `NativeVideoBackend` in
//! `crates/video-backend`) can use [`crate::video_client::VideoBridgeClient`]
//! for a typed Rust wrapper.
//!
//! ## Cisco / patent licensing tradeoff
//!
//! openh264-rs supports two build modes, selected via Cargo features:
//!
//! | Feature | How openh264 is obtained | Patent grant from Cisco? |
//! |---------|--------------------------|--------------------------|
//! | `source` | Built from Cisco's reference C source at compile time | **NO** — BSD-2-Clause only |
//! | `libloading` | Cisco binary loaded at runtime from the OS | **YES** — Cisco's MPEG-LA grant attaches |
//!
//! **This landing uses `source`** (the `openh264` crate `source` feature). The
//! codec is functionally identical to the binary, but you bear MPEG-LA H.264
//! patent risk in distribution. For a consumer-facing release:
//!
//! - Switch to `features = ["libloading"]` and ship/download the Cisco binary
//!   (`libopenh264.so.2` / `.dll` / `.dylib`) alongside the app. Cisco's
//!   binary distribution carries their explicit patent grant.
//! - Or substitute a royalty-free codec (AV1 via `rav1e`/`dav1d`) for the
//!   whole video path.
//!
//! See `docs/dev/video-codec-strategy.md` for the full architecture overview.
//!
//! ## Wire format — base64 for binary fields
//!
//! JSON + raw `Vec<u8>` serializes as `[72, 101, ...]` — one integer per byte,
//! very large. All binary fields (`data_b64`, `nal_units_b64`) use **standard
//! base64** instead (same engine as the rest of the host-bridge crate). The
//! [`crate::video_client::VideoBridgeClient`] encodes/decodes automatically.
//!
//! ## Session model
//!
//! Encoders and decoders are stateful (parameter sets, reference frames). Each
//! caller picks a unique `session_id` string and reuses it across calls for a
//! single stream. Sessions are stored in [`VideoState`]. Call
//! `POST /host/video/close_session` when done to free the codec context.
//!
//! ## Performance note
//!
//! JSON+base64 transport adds ~33% size overhead and a serde round-trip per
//! frame. For a 1080p30 stream that is measurable but acceptable for local IPC
//! (loopback). Migrate to a binary multipart endpoint when latency matters —
//! the `VideoBridgeClient` API is designed so only the transport layer changes.

// This module is a self-contained H.264 codec service. Every arithmetic and
// indexing operation below is YUV-plane math derived from frame dimensions that
// are validated before use (see the explicit `raw.len() < y_size + 2 * uv_size`
// / `nv12.len() < ...` length guards before each plane slice), and the two
// `unwrap()`s on `bitstream.layer(li)` / `layer.nal_unit(ni)` are bounded by the
// `0..num_layers()` / `0..nal_count()` ranges they iterate. These panic-class
// lints would fire on every line of provably-bounded codec math, so they are
// allowed module-wide rather than suppressed line-by-line.
// poly-lint: allow — bounded codec math, see comment above
#![allow(
    clippy::arithmetic_side_effects,
    clippy::integer_division,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    // cast: as_millis()→u64 is bounded (Duration since UNIX_EPOCH is centuries, fits u64);
    //        u32→usize widening is safe (usize ≥ 32 bits on all targets we support).
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    // The MutexGuard for encode/decode sessions must outlive the codec reference that
    // borrows from it; clippy's rewrite suggestion is structurally invalid here.
    clippy::significant_drop_tightening,
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use base64::Engine as _;
use openh264::encoder::{BitRate, Encoder, EncoderConfig, FrameType};
use openh264::formats::{BgraSliceU8, YUVBuffer, YUVSource};
use serde::{Deserialize, Serialize};

/// Route constants for `POST /host/video/*`.
pub const ROUTE_VIDEO_ENCODE: &str = "/host/video/encode_h264";
pub const ROUTE_VIDEO_DECODE: &str = "/host/video/decode_h264";
pub const ROUTE_VIDEO_CLOSE_SESSION: &str = "/host/video/close_session";

// ─── Wire types ──────────────────────────────────────────────────────────────

/// Request body for `POST /host/video/encode_h264`.
///
/// `data_b64` is a **base64-encoded** raw frame. Supported `format` values:
/// - `"bgra"` — 4 bytes/pixel, B/G/R/A order (common from screen-capture
///   APIs like scap on Linux/macOS/Win). Converted via openh264's own
///   `BgraSliceU8` → `YUVBuffer::from_rgb_source` internally.
/// - `"yuv420p"` — planar YUV 4:2:0. Y plane (`width * height`), then
///   Cb (`(w/2)*(h/2)`), then Cr (`(w/2)*(h/2)`). Passed directly to the
///   encoder with no conversion overhead.
/// - `"nv12"` — semi-planar YUV 4:2:0 (common from V4L2/camera APIs).
///   Converted to planar YUV420p in the handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeH264Request {
    pub width: u32,
    pub height: u32,
    /// Pixel format of `data_b64`: `"bgra"`, `"yuv420p"`, or `"nv12"`.
    pub format: String,
    /// Raw frame bytes, **base64-encoded** (standard alphabet, no line breaks).
    pub data_b64: String,
    /// If `true`, force a keyframe (IDR) even if the encoder would not.
    pub force_keyframe: bool,
    /// Unique identifier for the encoder session. Create one encoder per
    /// stream; pass the same `session_id` for every frame in that stream.
    pub session_id: String,
    /// Optional target bitrate in bps for dynamic bandwidth adaptation (Phase E.9).
    ///
    /// When `Some(bps)`, the encoder session is re-initialized with the new bitrate
    /// if the requested value differs from the current session bitrate by more than
    /// 15%.  The next frame is automatically forced as a keyframe (IDR) after
    /// re-initialization so the decoder can resync cleanly.
    ///
    /// When `None` (default), the session bitrate is unchanged (starts at 2 Mbps).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_bps: Option<u32>,
}

/// Response body for `POST /host/video/encode_h264`.
///
/// NAL units are returned individually without Annex-B start codes.
/// Callers that need Annex-B (e.g. WebRTC) can prepend `[0, 0, 0, 1]`
/// to each unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeH264Response {
    pub ok: bool,
    /// Each NAL unit as a separate base64 string (standard alphabet).
    pub nal_units_b64: Vec<String>,
    pub is_keyframe: bool,
    pub timestamp_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Request body for `POST /host/video/decode_h264`.
///
/// Each entry in `nal_units_b64` is one NAL unit, base64-encoded, without
/// Annex-B start codes (strip `[0, 0, 0, 1]` / `[0, 0, 1]` prefixes first).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodeH264Request {
    /// NAL units to decode, each base64-encoded.
    pub nal_units_b64: Vec<String>,
    /// Decoder session identifier. Reuse across calls for the same stream.
    pub session_id: String,
}

/// Response body for `POST /host/video/decode_h264`.
///
/// `frames` may be empty when the NAL unit(s) are config-only (SPS/PPS) or
/// when the decoder is still filling its reference frame buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodeH264Response {
    pub ok: bool,
    /// Decoded frames. Empty when the NAL unit was config-only.
    pub frames: Vec<DecodedFrame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// One decoded video frame.
///
/// `data_b64` carries YUV 4:2:0 planar data (`"yuv420p"`). Layout:
/// `width * height` bytes of Y, then `(width/2) * (height/2)` of Cb,
/// then `(width/2) * (height/2)` of Cr. Callers convert to their preferred
/// format (BGRA, NV12, …) — conversion is cheap and avoids coupling the
/// codec crate to a specific pixel layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    /// Always `"yuv420p"` — the canonical decoded format from openh264.
    pub format: String,
    /// Frame data, **base64-encoded** (standard alphabet).
    pub data_b64: String,
    pub timestamp_ms: u64,
}

/// Body for `POST /host/video/close_session`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseSessionRequest {
    pub session_id: String,
}

/// Response for `POST /host/video/close_session`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseSessionResponse {
    pub ok: bool,
    /// `true` if a session with that id was found and removed.
    pub removed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ─── State ───────────────────────────────────────────────────────────────────

/// Shared video codec state mounted into axum `State`.
///
/// Encoder and decoder maps are separate because a session may hold both
/// (e.g. a loopback test that encodes then decodes its own output), though in
/// practice callers will be either pure encoders or pure decoders.
///
/// The encoder map stores `(Encoder, current_bps)` so that Phase E.9 dynamic
/// bitrate adaptation can detect when the requested `target_bps` differs from
/// the current session bitrate and re-initialize the encoder accordingly.
#[derive(Clone)]
pub struct VideoState {
    encoders: Arc<Mutex<HashMap<String, (Encoder, u32)>>>,
    decoders: Arc<Mutex<HashMap<String, openh264::decoder::Decoder>>>,
}

impl Default for VideoState {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoState {
    /// Construct an empty video state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            encoders: Arc::new(Mutex::new(HashMap::new())),
            decoders: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn b64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())
}

/// Convert NV12 (semi-planar) to planar YUV 4:2:0.
fn nv12_to_yuv420p(nv12: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let w = width as usize;
    let h = height as usize;
    let y_size = w * h;
    let uv_size = (w / 2) * (h / 2);
    if nv12.len() < y_size + 2 * uv_size {
        return Err(format!(
            "nv12 frame too small: expected >= {} got {}",
            y_size + 2 * uv_size,
            nv12.len()
        ));
    }
    let mut out = vec![0u8; y_size + 2 * uv_size];
    // Y plane: copy directly
    out[..y_size].copy_from_slice(&nv12[..y_size]);
    // De-interleave Cb/Cr from interleaved UV plane
    let cb_offset = y_size;
    let cr_offset = y_size + uv_size;
    for i in 0..uv_size {
        out[cb_offset + i] = nv12[y_size + 2 * i];
        out[cr_offset + i] = nv12[y_size + 2 * i + 1];
    }
    Ok(out)
}

// ─── YUV source adapter for openh264 (planar YUV420p) ────────────────────────

/// Adapts a planar YUV420p buffer to `openh264::formats::YUVSource`.
/// Lifetime-bound to the frame buffer — no extra copy.
struct PlaneYuv420p<'a> {
    width: usize,
    height: usize,
    y: &'a [u8],
    cb: &'a [u8],
    cr: &'a [u8],
}

impl YUVSource for PlaneYuv420p<'_> {
    fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
    fn strides(&self) -> (usize, usize, usize) {
        (self.width, self.width / 2, self.width / 2)
    }
    fn y(&self) -> &[u8] {
        self.y
    }
    fn u(&self) -> &[u8] {
        self.cb
    }
    fn v(&self) -> &[u8] {
        self.cr
    }
}

// ─── Encoder constants ────────────────────────────────────────────────────────

/// Default encoder bitrate when no `target_bps` is provided.
const DEFAULT_ENCODER_BPS: u32 = 2_000_000;
/// Reinitialize the encoder if the requested bitrate differs by more than this fraction.
/// 15% hysteresis avoids churn from minor REMB fluctuations.
const BITRATE_REINIT_THRESHOLD: f64 = 0.15;

// ─── Axum handlers ───────────────────────────────────────────────────────────

/// `POST /host/video/encode_h264`
///
/// Stateful H.264 encoder keyed by `session_id`. The encoder is created on
/// first use and kept alive until `POST /host/video/close_session` is called.
///
/// CPU-bound openh264 work is offloaded to a blocking thread via
/// `tokio::task::spawn_blocking` so the axum async runtime is not stalled.
// bgra/yuv420p/nv12 format branches cannot be split without duplicating the
// entire spawn_blocking frame (session-lock + encode + collect).
// lint-allow-unused: codec format dispatch + spawn_blocking frame is one unit
#[allow(clippy::too_many_lines)]
pub async fn encode_h264(
    State(state): State<VideoState>,
    Json(req): Json<EncodeH264Request>,
) -> impl IntoResponse {
    let timestamp_ms = now_ms();

    // Decode base64 frame data
    let raw = match b64_decode(&req.data_b64) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(EncodeH264Response {
                    ok: false,
                    nal_units_b64: vec![],
                    is_keyframe: false,
                    timestamp_ms,
                    err: Some(format!("invalid data_b64: {e}")),
                }),
            );
        }
    };

    // Validate format before spawning so callers get 400 not 500.
    if !matches!(req.format.as_str(), "bgra" | "yuv420p" | "nv12") {
        return (
            StatusCode::BAD_REQUEST,
            Json(EncodeH264Response {
                ok: false,
                nal_units_b64: vec![],
                is_keyframe: false,
                timestamp_ms,
                err: Some(format!(
                    "unsupported format: {:?}; expected bgra|yuv420p|nv12",
                    req.format
                )),
            }),
        );
    }

    let width = req.width;
    let height = req.height;
    let mut force_keyframe = req.force_keyframe;
    let session_id = req.session_id.clone();
    let format = req.format.clone();
    let target_bps = req.target_bps;
    let encoders_arc = Arc::clone(&state.encoders);

    // Offload CPU-bound encode to blocking thread pool.
    let result = tokio::task::spawn_blocking(move || -> Result<(Vec<Vec<u8>>, bool), String> {
        let mut map = encoders_arc
            .lock()
            .map_err(|e| format!("encoder lock poisoned: {e}"))?;

        // Determine the desired bitrate for this frame.
        let desired_bps = target_bps.unwrap_or(DEFAULT_ENCODER_BPS);

        // Create a new encoder session OR reinitialize an existing one if the bitrate
        // has changed beyond the hysteresis threshold (Phase E.9).
        let needs_init = if let Some((_, current_bps)) = map.get(&session_id) {
            let delta = (f64::from(*current_bps) - f64::from(desired_bps)).abs() / f64::from(*current_bps);
            delta > BITRATE_REINIT_THRESHOLD
        } else {
            true
        };

        if needs_init {
            let cfg = EncoderConfig::new()
                .bitrate(BitRate::from_bps(desired_bps))
                .skip_frames(true);
            let enc = Encoder::with_api_config(openh264::OpenH264API::from_source(), cfg)
                .map_err(|e| format!("create encoder (bps={desired_bps}): {e}"))?;
            map.insert(session_id.clone(), (enc, desired_bps));
            // Always force a keyframe after re-initialization so the decoder
            // can resync with the new encoder state.
            force_keyframe = true;
        }

        let (enc, _) = map
            .get_mut(&session_id)
            .ok_or_else(|| "encoder disappeared after insert".to_string())?;

        if force_keyframe {
            enc.force_intra_frame();
        }

        // Encode: convert from input format to what openh264 needs (YUV420p).
        let bitstream = match format.as_str() {
            "bgra" => {
                // openh264 provides BgraSliceU8 → YUVBuffer conversion natively.
                let bgra = BgraSliceU8::new(&raw, (width as usize, height as usize));
                let yuv = YUVBuffer::from_rgb_source(bgra);
                enc.encode(&yuv).map_err(|e| format!("encode (bgra): {e}"))?
            }
            "yuv420p" => {
                let w = width as usize;
                let h = height as usize;
                let y_size = w * h;
                let uv_size = (w / 2) * (h / 2);
                if raw.len() < y_size + 2 * uv_size {
                    return Err(format!(
                        "yuv420p buffer too small: need {} got {}",
                        y_size + 2 * uv_size,
                        raw.len()
                    ));
                }
                let yuv_src = PlaneYuv420p {
                    width: w,
                    height: h,
                    y: &raw[..y_size],
                    cb: &raw[y_size..y_size + uv_size],
                    cr: &raw[y_size + uv_size..y_size + 2 * uv_size],
                };
                enc.encode(&yuv_src)
                    .map_err(|e| format!("encode (yuv420p): {e}"))?
            }
            "nv12" => {
                let yuv_vec =
                    nv12_to_yuv420p(&raw, width, height).map_err(|e| format!("nv12 convert: {e}"))?;
                let w = width as usize;
                let h = height as usize;
                let y_size = w * h;
                let uv_size = (w / 2) * (h / 2);
                let yuv_src = PlaneYuv420p {
                    width: w,
                    height: h,
                    y: &yuv_vec[..y_size],
                    cb: &yuv_vec[y_size..y_size + uv_size],
                    cr: &yuv_vec[y_size + uv_size..y_size + 2 * uv_size],
                };
                enc.encode(&yuv_src)
                    .map_err(|e| format!("encode (nv12): {e}"))?
            }
            // Safety: format validated before spawn_blocking.
            _ => unreachable!("format validated before spawn_blocking")
        };

        let is_keyframe = bitstream.frame_type() == FrameType::IDR
            || bitstream.frame_type() == FrameType::I;

        let nal_units: Vec<Vec<u8>> = (0..bitstream.num_layers())
            .flat_map(|li| {
                let layer = bitstream.layer(li).unwrap(); // bounds checked by num_layers
                (0..layer.nal_count())
                    .map(|ni| layer.nal_unit(ni).unwrap().to_vec())
                    .collect::<Vec<_>>()
            })
            .collect();

        Ok((nal_units, is_keyframe))
    })
    .await;

    match result {
        Ok(Ok((nal_units, is_keyframe))) => {
            let nal_units_b64 = nal_units.iter().map(|u| b64_encode(u)).collect();
            (
                StatusCode::OK,
                Json(EncodeH264Response {
                    ok: true,
                    nal_units_b64,
                    is_keyframe,
                    timestamp_ms,
                    err: None,
                }),
            )
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(EncodeH264Response {
                ok: false,
                nal_units_b64: vec![],
                is_keyframe: false,
                timestamp_ms,
                err: Some(e),
            }),
        ),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(EncodeH264Response {
                ok: false,
                nal_units_b64: vec![],
                is_keyframe: false,
                timestamp_ms,
                err: Some(format!("spawn_blocking panic: {join_err}")),
            }),
        ),
    }
}

/// `POST /host/video/decode_h264`
///
/// Stateful H.264 decoder keyed by `session_id`. The decoder is created on
/// first use and persists until `close_session`. Each NAL unit in
/// `nal_units_b64` is decoded in sequence; `frames` may be empty when NALs
/// are parameter-set only (SPS/PPS).
pub async fn decode_h264(
    State(state): State<VideoState>,
    Json(req): Json<DecodeH264Request>,
) -> impl IntoResponse {
    // Decode base64 NAL units up-front in async context (cheap, no blocking).
    let mut nal_bufs: Vec<Vec<u8>> = Vec::with_capacity(req.nal_units_b64.len());
    for (i, b64_str) in req.nal_units_b64.iter().enumerate() {
        match b64_decode(b64_str) {
            Ok(b) => nal_bufs.push(b),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(DecodeH264Response {
                        ok: false,
                        frames: vec![],
                        err: Some(format!("nal_units_b64[{i}]: invalid base64: {e}")),
                    }),
                );
            }
        }
    }

    let session_id = req.session_id.clone();
    let decoders_arc = Arc::clone(&state.decoders);

    let result = tokio::task::spawn_blocking(move || -> Result<Vec<DecodedFrame>, String> {
        let mut map = decoders_arc
            .lock()
            .map_err(|e| format!("decoder lock poisoned: {e}"))?;

        if !map.contains_key(&session_id) {
            let dec = openh264::decoder::Decoder::new()
                .map_err(|e| format!("create decoder: {e}"))?;
            map.insert(session_id.clone(), dec);
        }

        let dec = map
            .get_mut(&session_id)
            .ok_or_else(|| "decoder disappeared after insert".to_string())?;

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let mut frames = Vec::new();

        for nal in &nal_bufs {
            match dec.decode(nal) {
                Ok(Some(yuv_frame)) => {
                    let (width, height) = yuv_frame.dimensions();
                    // Flatten YUV planes: Y | Cb | Cr
                    let mut data = Vec::with_capacity(width * height * 3 / 2);
                    data.extend_from_slice(yuv_frame.y());
                    data.extend_from_slice(yuv_frame.u());
                    data.extend_from_slice(yuv_frame.v());
                    frames.push(DecodedFrame {
                        width: width as u32,
                        height: height as u32,
                        format: "yuv420p".to_string(),
                        data_b64: b64_encode(&data),
                        timestamp_ms: ts,
                    });
                }
                Ok(None) => {
                    // Config-only NAL (SPS/PPS) — no frame emitted, not an error.
                }
                Err(e) => {
                    return Err(format!("decode NAL: {e}"));
                }
            }
        }

        Ok(frames)
    })
    .await;

    match result {
        Ok(Ok(frames)) => (
            StatusCode::OK,
            Json(DecodeH264Response {
                ok: true,
                frames,
                err: None,
            }),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(DecodeH264Response {
                ok: false,
                frames: vec![],
                err: Some(e),
            }),
        ),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(DecodeH264Response {
                ok: false,
                frames: vec![],
                err: Some(format!("spawn_blocking panic: {join_err}")),
            }),
        ),
    }
}

/// `POST /host/video/close_session`
///
/// Drops the encoder and/or decoder for `session_id`. Always returns
/// `ok: true` even if no session was found (idempotent cleanup).
// lint-allow-unused: axum route handlers must be async even when the body is sync.
#[allow(clippy::unused_async)]
pub async fn close_session(
    State(state): State<VideoState>,
    Json(req): Json<CloseSessionRequest>,
) -> impl IntoResponse {
    let enc_removed = state
        .encoders
        .lock()
        .map(|mut m| m.remove(&req.session_id).is_some())
        .unwrap_or(false);
    let dec_removed = state
        .decoders
        .lock()
        .map(|mut m| m.remove(&req.session_id).is_some())
        .unwrap_or(false);

    (
        StatusCode::OK,
        Json(CloseSessionResponse {
            ok: true,
            removed: enc_removed || dec_removed,
            err: None,
        }),
    )
}
