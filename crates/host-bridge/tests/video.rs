//! Round-trip integration tests for the `/host/video/*` H.264 encode/decode endpoints.
//!
//! These tests build an in-process axum router (no real network socket) and
//! exercise the full encode→decode round-trip through the openh264 codec.
//!
//! ## Build gate
//!
//! Only compiled when the `video` feature is enabled. On platforms where the
//! openh264 source build fails (some CI environments lack cmake/nasm), the
//! cargo feature won't resolve and these tests simply won't be built.
//!
//! Run with:
//!   cargo test -p poly-host-bridge --features video --test video
//!
//! Skip CI with feature absent:
//!   cargo test -p poly-host-bridge --test video  # compiles nothing, harmless

#![cfg(feature = "video")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::post,
};
use base64::Engine as _;
use poly_host_bridge::video::{
    CloseSessionRequest, CloseSessionResponse, DecodeH264Request, DecodeH264Response,
    EncodeH264Request, EncodeH264Response, VideoState, close_session, decode_h264, encode_h264,
};
use tower::util::ServiceExt; // for `oneshot`

// ─── Test helpers ─────────────────────────────────────────────────────────────

/// Build a minimal in-process router with only the video routes.
fn test_router() -> Router {
    Router::new()
        .route("/host/video/encode_h264", post(encode_h264))
        .route("/host/video/decode_h264", post(decode_h264))
        .route("/host/video/close_session", post(close_session))
        .with_state(VideoState::new())
}

/// Helper: POST JSON body to `path`, return response body as parsed T.
async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    router: &Router,
    path: &str,
    body: &B,
) -> (StatusCode, T) {
    let body_bytes = serde_json::to_vec(body).unwrap();
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body_bytes))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let parsed: T = serde_json::from_slice(&body).unwrap();
    (status, parsed)
}

fn b64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64_decode(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .unwrap()
}

/// Generate a synthetic BGRA frame with a simple gradient pattern.
/// Width * height * 4 bytes total.
fn make_bgra_frame(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let mut frame = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        for col in 0..width {
            // Simple gradient: varies by position + seed so consecutive frames differ.
            let b = ((col * 255 / width) as u8).wrapping_add(seed);
            let g = ((row * 255 / height) as u8).wrapping_add(seed / 2);
            let r = seed.wrapping_add((col + row) as u8);
            let a = 255u8;
            frame.extend_from_slice(&[b, g, r, a]);
        }
    }
    frame
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Basic encode: a single BGRA keyframe should yield at least one NAL unit.
#[tokio::test]
async fn encode_bgra_produces_nal_units() {
    let router = test_router();
    let width = 64u32;
    let height = 64u32;
    let frame = make_bgra_frame(width, height, 0);

    let req = EncodeH264Request {
        width,
        height,
        format: "bgra".into(),
        data_b64: b64_encode(&frame),
        force_keyframe: true,
        session_id: "test-encode-basic".into(),
        target_bps: None,
    };
    let (status, resp): (StatusCode, EncodeH264Response) =
        post_json(&router, "/host/video/encode_h264", &req).await;

    assert_eq!(status, StatusCode::OK, "encode returned non-200");
    assert!(resp.ok, "encode ok=false: {:?}", resp.err);
    assert!(
        !resp.nal_units_b64.is_empty(),
        "expected at least one NAL unit"
    );
    assert!(resp.is_keyframe, "expected keyframe=true for first frame");
}

/// Round-trip: encode 4 BGRA frames then decode the NAL stream and verify
/// at least one frame emerges with correct dimensions.
#[tokio::test]
async fn round_trip_encode_decode() {
    let router = test_router();
    let width = 64u32;
    let height = 64u32;
    let session = "round-trip-session";

    let mut all_nals_b64: Vec<String> = Vec::new();

    // Encode 4 frames
    for i in 0..4u8 {
        let frame = make_bgra_frame(width, height, i * 17);
        let req = EncodeH264Request {
            width,
            height,
            format: "bgra".into(),
            data_b64: b64_encode(&frame),
            force_keyframe: i == 0,
            session_id: session.into(),
            target_bps: None,
        };
        let (status, resp): (StatusCode, EncodeH264Response) =
            post_json(&router, "/host/video/encode_h264", &req).await;
        assert_eq!(status, StatusCode::OK);
        assert!(resp.ok, "encode frame {i} failed: {:?}", resp.err);
        all_nals_b64.extend(resp.nal_units_b64);
    }

    assert!(!all_nals_b64.is_empty(), "no NAL units produced");

    // Decode all collected NAL units in one call
    let dec_req = DecodeH264Request {
        nal_units_b64: all_nals_b64,
        session_id: "round-trip-decode".into(),
    };
    let (status, dec_resp): (StatusCode, DecodeH264Response) =
        post_json(&router, "/host/video/decode_h264", &dec_req).await;

    assert_eq!(status, StatusCode::OK);
    assert!(dec_resp.ok, "decode failed: {:?}", dec_resp.err);
    assert!(
        !dec_resp.frames.is_empty(),
        "expected at least one decoded frame"
    );

    for frame in &dec_resp.frames {
        assert_eq!(frame.format, "yuv420p");
        assert_eq!(frame.width, width, "decoded frame width mismatch");
        assert_eq!(frame.height, height, "decoded frame height mismatch");

        // The decoded data is stride-padded by openh264 (iStride[0] may be
        // wider than the pixel width for alignment). Just assert non-empty and
        // that the base64 decodes without error.
        let data = b64_decode(&frame.data_b64);
        assert!(
            !data.is_empty(),
            "decoded frame data should not be empty for {}x{}",
            width,
            height
        );
        // The YUV data must be at least the unpadded planar size.
        let min_expected = (width * height + 2 * (width / 2) * (height / 2)) as usize;
        assert!(
            data.len() >= min_expected,
            "decoded frame data {} bytes is less than minimum unpadded size {} for {}x{}",
            data.len(),
            min_expected,
            width,
            height
        );
    }
}

/// Verify that bad format string returns 400 with ok=false.
#[tokio::test]
async fn encode_bad_format_returns_error() {
    let router = test_router();
    let req = EncodeH264Request {
        width: 64,
        height: 64,
        format: "rgb888_invalid".into(),
        data_b64: b64_encode(&[0u8; 64 * 64 * 4]),
        force_keyframe: false,
        session_id: "bad-format".into(),
        target_bps: None,
    };
    let (status, resp): (StatusCode, EncodeH264Response) =
        post_json(&router, "/host/video/encode_h264", &req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!resp.ok);
    assert!(resp.err.is_some(), "expected error message");
}

/// Verify that close_session succeeds (idempotent — even for unknown sessions).
#[tokio::test]
async fn close_session_is_idempotent() {
    let router = test_router();

    // Close a session that was never opened — should not error
    let req = CloseSessionRequest {
        session_id: "nonexistent-session".into(),
    };
    let (status, resp): (StatusCode, CloseSessionResponse) =
        post_json(&router, "/host/video/close_session", &req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(resp.ok);
    assert!(!resp.removed, "should not have removed a nonexistent session");

    // Open a session, then close it
    let enc_req = EncodeH264Request {
        width: 64,
        height: 64,
        format: "bgra".into(),
        data_b64: b64_encode(&make_bgra_frame(64, 64, 0)),
        force_keyframe: true,
        session_id: "close-me".into(),
        target_bps: None,
    };
    let (_, enc_resp): (_, EncodeH264Response) =
        post_json(&router, "/host/video/encode_h264", &enc_req).await;
    assert!(enc_resp.ok);

    let close_req = CloseSessionRequest {
        session_id: "close-me".into(),
    };
    let (status, close_resp): (StatusCode, CloseSessionResponse) =
        post_json(&router, "/host/video/close_session", &close_req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(close_resp.ok);
    assert!(close_resp.removed, "should have removed the encoder session");

    // Closing again should succeed but removed=false
    let (status, close_resp2): (StatusCode, CloseSessionResponse) =
        post_json(&router, "/host/video/close_session", &close_req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(close_resp2.ok);
    assert!(!close_resp2.removed);
}
