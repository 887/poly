//! # Round-trip integration tests for the `/host/aead/*` AEAD encrypt/decrypt service.
//!
//! Spins up a real axum server and exercises create / encrypt / decrypt / close
//! for both XChaCha20-Poly1305 and AES-256-GCM over real HTTP.
//!
//! Run with:
//!   cargo test -p poly-host-bridge --features aead --test aead

#![cfg(feature = "aead")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_host_bridge::aead::{AeadState, router};
use poly_host_bridge::aead_client::AeadClient;
use tokio::net::TcpListener;

// ── Test helpers ───────────────────────────────────────────────────────────────

async fn spawn_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(AeadState::new());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

// ── Tests ──────────────────────────────────────────────────────────────────────

/// XChaCha20-Poly1305 round-trip: encrypt then decrypt returns the original plaintext.
#[tokio::test(flavor = "multi_thread")]
async fn xchacha20_round_trip() {
    let base = spawn_server().await;
    let client = AeadClient::new(&base);

    let key = [0x42u8; 32];
    let nonce = [0x11u8; 24]; // 24 bytes for XChaCha20
    let plaintext = b"xchacha20 test plaintext";

    let session_id = client
        .create("xchacha20poly1305", &key)
        .await
        .expect("create session");

    let ciphertext = client
        .encrypt(&session_id, &nonce, plaintext, None)
        .await
        .expect("encrypt");

    assert_ne!(
        ciphertext.as_slice(),
        plaintext.as_slice(),
        "ciphertext must differ from plaintext"
    );

    let decrypted = client
        .decrypt(&session_id, &nonce, &ciphertext, None)
        .await
        .expect("decrypt");

    assert_eq!(decrypted.as_slice(), plaintext, "decrypted must match original plaintext");

    client.close(&session_id).await.expect("close");
}

/// AES-256-GCM round-trip: encrypt then decrypt returns the original plaintext.
#[tokio::test(flavor = "multi_thread")]
async fn aes256gcm_round_trip() {
    let base = spawn_server().await;
    let client = AeadClient::new(&base);

    let key = [0xABu8; 32];
    let nonce = [0x99u8; 12]; // 12 bytes for AES-256-GCM
    let plaintext = b"aes-256-gcm test plaintext";

    let session_id = client
        .create("aes256gcm", &key)
        .await
        .expect("create session");

    let ciphertext = client
        .encrypt(&session_id, &nonce, plaintext, None)
        .await
        .expect("encrypt");

    assert_ne!(
        ciphertext.as_slice(),
        plaintext.as_slice(),
        "ciphertext must differ from plaintext"
    );

    let decrypted = client
        .decrypt(&session_id, &nonce, &ciphertext, None)
        .await
        .expect("decrypt");

    assert_eq!(decrypted.as_slice(), plaintext, "decrypted must match original plaintext");

    client.close(&session_id).await.expect("close");
}

/// Encrypting with `aad=b"hello"` and decrypting with `aad=b"world"` must fail.
#[tokio::test(flavor = "multi_thread")]
async fn aad_mismatch_decryption_fails() {
    let base = spawn_server().await;
    let client = AeadClient::new(&base);

    let key = [0x55u8; 32];
    let nonce = [0x22u8; 24]; // XChaCha20
    let plaintext = b"aad mismatch test";

    let session_id = client
        .create("xchacha20poly1305", &key)
        .await
        .expect("create session");

    let ciphertext = client
        .encrypt(&session_id, &nonce, plaintext, Some(b"hello"))
        .await
        .expect("encrypt with aad=hello");

    // Decrypt with wrong AAD — must fail (authentication tag mismatch).
    let err = client
        .decrypt(&session_id, &nonce, &ciphertext, Some(b"world"))
        .await;

    assert!(
        err.is_err(),
        "decrypt with mismatched AAD must return an error, got: {err:?}"
    );

    client.close(&session_id).await.expect("close");
}

/// After close, encrypt should fail with a server error (session not found).
#[tokio::test(flavor = "multi_thread")]
async fn close_invalidates_session() {
    let base = spawn_server().await;
    let client = AeadClient::new(&base);

    let key = [0xFFu8; 32];
    let nonce = [0x00u8; 24];

    let session_id = client
        .create("xchacha20poly1305", &key)
        .await
        .expect("create session");

    client.close(&session_id).await.expect("close");

    // Encrypt on a closed session must fail.
    let err = client.encrypt(&session_id, &nonce, b"dead", None).await;
    assert!(
        err.is_err(),
        "encrypt after close should return an error, got: {err:?}"
    );
}
