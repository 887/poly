//! # Round-trip integration tests for the `/host/codec/opus/*` Opus encode/decode service.
//!
//! Spins up a real axum server and exercises encoder create / encode /
//! decoder create / decode / close end-to-end over real HTTP.
//!
//! Run with:
//!   cargo test -p poly-host-bridge --features codec-opus --test codec-opus

#![cfg(feature = "codec-opus")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_host_bridge::codec_opus::{OpusState, router};
use poly_host_bridge::codec_opus_client::OpusClient;
use tokio::net::TcpListener;

// ── Test helpers ───────────────────────────────────────────────────────────────

async fn spawn_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(OpusState::new());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

// ── Tests ──────────────────────────────────────────────────────────────────────

/// Create encoder (48kHz stereo, voip), create decoder (48kHz stereo),
/// encode 1920 i16 samples (20ms @ 48kHz stereo), decode, assert length == 1920.
///
/// Opus is lossy so we don't check exact sample values, only frame length.
#[tokio::test(flavor = "multi_thread")]
async fn encoder_decoder_round_trip() {
    let base = spawn_server().await;
    let client = OpusClient::new(&base);

    // Create sessions.
    let enc_id = client
        .encoder_create(48_000, 2, "voip")
        .await
        .expect("encoder_create");
    let dec_id = client
        .decoder_create(48_000, 2)
        .await
        .expect("decoder_create");

    // 20ms @ 48kHz stereo = 960 mono frames * 2 channels = 1920 i16 samples.
    let pcm_in: Vec<i16> = (0..1920i16).map(|i| i % 32767).collect();

    // Encode.
    let packet = client.encode(&enc_id, &pcm_in).await.expect("encode");
    assert!(!packet.is_empty(), "encoded packet must be non-empty");

    // Decode.
    let pcm_out = client.decode(&dec_id, &packet).await.expect("decode");

    // Opus returns one stereo frame: 960 sample pairs = 1920 i16 values.
    assert_eq!(
        pcm_out.len(),
        1920,
        "decoded PCM must be 1920 i16 samples (960 stereo pairs)"
    );

    // Clean up.
    client.close(&enc_id).await.expect("close encoder");
    client.close(&dec_id).await.expect("close decoder");
}

/// After close, encode should fail with a server error (session not found).
#[tokio::test(flavor = "multi_thread")]
async fn close_invalidates_session() {
    let base = spawn_server().await;
    let client = OpusClient::new(&base);

    let enc_id = client
        .encoder_create(48_000, 2, "voip")
        .await
        .expect("encoder_create");

    client.close(&enc_id).await.expect("close");

    // Encode on a closed session must fail.
    let pcm = vec![0i16; 1920];
    let err = client.encode(&enc_id, &pcm).await;
    assert!(
        err.is_err(),
        "encode after close should return an error, got: {err:?}"
    );
}
