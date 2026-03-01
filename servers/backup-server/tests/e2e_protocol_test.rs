//! End-to-end protocol tests for the Poly Backup Server.
//!
//! Spins up a real in-process Axum server bound to a random port (TCP 127.0.0.1:0)
//! with SurrealKV stored in a temporary directory, then exercises the full
//! client protocol:
//!
//! 1. `POST /api/challenge`  — request a PoW nonce
//! 2. Solve PoW (SHA-256 leading-zero bits)
//! 3. `POST /api/auth`       — exchange PoW solution for a session token
//! 4. `POST /api/sync/push`  — push an encrypted blob
//! 5. `GET /api/sync/pull`   — pull blobs since a sequence number
//! 6. `GET /api/sync/status` — verify blob count and latest sequence
//! 7. Error paths: 401 without token, 400 for malformed request, re-authentication

use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result, anyhow};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use poly_backup_server::{AdminState, AppState, Config, create_app, init_db};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

// ── Shared test types (mirrors server-side shapes) ───────────────────────────

#[derive(Debug, Serialize)]
struct ChallengeRequest<'a> {
    public_key: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChallengeResponse {
    nonce: String,
    difficulty: u32,
}

#[derive(Debug, Serialize)]
struct AuthRequest<'a> {
    public_key: &'a str,
    nonce: String,
    counter: u64,
    passphrase: &'a str,
    device_name: &'a str,
}

#[derive(Debug, Deserialize)]
struct AuthResponse {
    token: String,
}

#[derive(Debug, Serialize)]
struct PushRequest {
    encrypted_blob: String,
}

#[derive(Debug, Deserialize)]
struct PushResponse {
    sequence: i64,
}

#[derive(Debug, Deserialize)]
struct BlobEntry {
    sequence: i64,
    encrypted_blob: String,
}

#[derive(Debug, Deserialize)]
struct PullResponse {
    blobs: Vec<BlobEntry>,
    latest_sequence: i64,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    blob_count: i64,
    latest_sequence: i64,
}

// ── Test server harness ──────────────────────────────────────────────────────

/// Assembled test server: Axum handle + bound address + temp dir lifetime.
struct TestServer {
    addr: SocketAddr,
    _data_dir: TempDir,
}

impl TestServer {
    /// Spin up a server with the given passphrase and PoW difficulty.
    async fn start(passphrase: &str, pow_difficulty: u32) -> Result<Self> {
        let data_dir = TempDir::new().context("create temp dir")?;

        let config = Arc::new(Config {
            server_name: "Test Poly Server".to_owned(),
            passphrase: passphrase.to_owned(),
            max_accounts: 0,
            token_expiry_days: 365,
            pow_difficulty,
            admin_pow_difficulty: 4,
            bind: "127.0.0.1:0".parse().context("parse bind addr")?,
            data_dir: data_dir.path().to_path_buf(),
            rate_limit_max: 100,
            rate_limit_window_secs: 3600,
            admin_user: "admin".to_owned(),
            admin_password: "adminpass".to_owned(),
            admin_session_hours: 24,
            admin_rate_limit_per_minute: 100,
        });

        let db = init_db(&config).await.context("init db")?;

        let state = AppState {
            db,
            config,
            admin: AdminState::new(),
        };

        let router = create_app(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind listener")?;
        let addr = listener.local_addr().context("local addr")?;

        tokio::spawn(async move {
            // Ignore serve error — test will fail naturally if server dies.
            let _ = axum::serve(listener, router).await;
        });

        Ok(TestServer {
            addr,
            _data_dir: data_dir,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }
}

// ── PoW solver ───────────────────────────────────────────────────────────────

/// Solve a PoW challenge: find the smallest `counter` such that the first
/// `difficulty` bits of `SHA-256(nonce + counter.to_string())` are zero.
fn solve_pow(nonce: &str, difficulty: u32) -> u64 {
    for counter in 0u64..u64::MAX {
        let input = format!("{nonce}{counter}");
        let hash = Sha256::digest(input.as_bytes());
        if leading_zero_bits(&hash) >= difficulty {
            return counter;
        }
    }
    // u64::MAX is the fallback sentinel — with difficulty=4 this is effectively unreachable.
    u64::MAX
}

/// Count the number of leading zero bits in a byte slice.
fn leading_zero_bits(hash: &[u8]) -> u32 {
    let mut bits = 0u32;
    for byte in hash {
        bits += byte.leading_zeros();
        if *byte != 0 {
            break;
        }
    }
    bits
}

// ── Helper: full auth flow ────────────────────────────────────────────────────

/// Perform the full challenge → PoW → auth flow; returns the bearer token.
async fn authenticate(
    client: &Client,
    server: &TestServer,
    public_key: &str,
    passphrase: &str,
    device_name: &str,
) -> Result<String> {
    // 1. Request challenge
    let resp = client
        .post(server.url("/api/challenge"))
        .json(&ChallengeRequest { public_key })
        .send()
        .await
        .context("challenge request")?;

    if resp.status() != 200 {
        let status = resp.status();
        let body = resp.text().await.context("challenge error body")?;
        return Err(anyhow!("challenge returned {status}: {body}"));
    }

    let challenge: ChallengeResponse = resp.json().await.context("challenge json")?;

    // 2. Solve PoW
    let counter = solve_pow(&challenge.nonce, challenge.difficulty);

    // 3. Authenticate
    let resp = client
        .post(server.url("/api/auth"))
        .json(&AuthRequest {
            public_key,
            nonce: challenge.nonce,
            counter,
            passphrase,
            device_name,
        })
        .send()
        .await
        .context("auth request")?;

    if resp.status() != 200 {
        let status = resp.status();
        let body = resp.text().await.context("auth error body")?;
        return Err(anyhow!("auth returned {status}: {body}"));
    }

    let auth: AuthResponse = resp.json().await.context("auth json")?;
    Ok(auth.token)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Test constant: 64-char hex string used as a fake Ed25519 public key.
///
/// The backup server only validates format (hex, 64 chars) — crypto is
/// performed client-side only.
const TEST_PK_A: &str = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
const TEST_PK_B: &str = "1122334455667788990011223344556677889900112233445566778899001122";

/// Full E2E happy path: challenge → auth → push → pull → status.
#[tokio::test]
async fn test_full_protocol_happy_path() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    // Authenticate
    let token = authenticate(&client, &server, TEST_PK_A, "testpass", "Test Device").await?;
    assert_eq!(token.len(), 128, "token should be 128 alphanumeric chars");
    assert!(
        token.chars().all(|c| c.is_ascii_alphanumeric()),
        "token must be alphanumeric, got: {token}"
    );

    // Push blob 1
    let blob1 = B64.encode(b"encrypted-settings-blob-version-1");
    let resp = client
        .post(server.url("/api/sync/push"))
        .bearer_auth(&token)
        .json(&PushRequest {
            encrypted_blob: blob1.clone(),
        })
        .send()
        .await
        .context("push 1")?;
    assert_eq!(resp.status(), 200, "push 1 status");
    let push1: PushResponse = resp.json().await.context("push1 json")?;
    assert_eq!(push1.sequence, 1, "first push sequence = 1");

    // Push blob 2
    let blob2 = B64.encode(b"encrypted-settings-blob-version-2");
    let resp = client
        .post(server.url("/api/sync/push"))
        .bearer_auth(&token)
        .json(&PushRequest {
            encrypted_blob: blob2.clone(),
        })
        .send()
        .await
        .context("push 2")?;
    assert_eq!(resp.status(), 200, "push 2 status");
    let push2: PushResponse = resp.json().await.context("push2 json")?;
    assert_eq!(push2.sequence, 2, "second push sequence = 2");

    // Pull all blobs (since=0)
    let resp = client
        .get(server.url("/api/sync/pull?since=0"))
        .bearer_auth(&token)
        .send()
        .await
        .context("pull since=0")?;
    assert_eq!(resp.status(), 200, "pull since=0 status");
    let pull: PullResponse = resp.json().await.context("pull json")?;
    assert_eq!(pull.blobs.len(), 2, "should have 2 blobs");
    let blob0 = pull.blobs.first().context("blob 0 missing")?;
    let blob1_entry = pull.blobs.get(1).context("blob 1 missing")?;
    assert_eq!(blob0.encrypted_blob, blob1, "blob 1 data round-trips");
    assert_eq!(blob0.sequence, 1);
    assert_eq!(blob1_entry.encrypted_blob, blob2, "blob 2 data round-trips");
    assert_eq!(blob1_entry.sequence, 2);
    assert_eq!(pull.latest_sequence, 2);

    // Pull since=1 (only newer blobs)
    let resp = client
        .get(server.url("/api/sync/pull?since=1"))
        .bearer_auth(&token)
        .send()
        .await
        .context("pull since=1")?;
    assert_eq!(resp.status(), 200, "pull since=1 status");
    let pull_since: PullResponse = resp.json().await.context("pull since=1 json")?;
    assert_eq!(pull_since.blobs.len(), 1, "only blob 2 after since=1");
    assert_eq!(
        pull_since.blobs.first().context("blob missing")?.sequence,
        2
    );

    // Status
    let resp = client
        .get(server.url("/api/sync/status"))
        .bearer_auth(&token)
        .send()
        .await
        .context("status")?;
    assert_eq!(resp.status(), 200, "status code");
    let status: StatusResponse = resp.json().await.context("status json")?;
    assert_eq!(status.blob_count, 2, "2 blobs pushed");
    assert_eq!(status.latest_sequence, 2, "latest sequence = 2");

    Ok(())
}

/// 401 Unauthorized with no bearer token.
#[tokio::test]
async fn test_push_without_token_is_401() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    let resp = client
        .post(server.url("/api/sync/push"))
        .json(&PushRequest {
            encrypted_blob: B64.encode(b"data"),
        })
        .send()
        .await
        .context("push no-token")?;
    assert_eq!(resp.status(), 401, "expect 401 without token");

    Ok(())
}

/// 401 Unauthorized with an invalid / expired token.
#[tokio::test]
async fn test_push_invalid_token_is_401() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    let fake_token = "a".repeat(128);
    let resp = client
        .post(server.url("/api/sync/push"))
        .bearer_auth(fake_token)
        .json(&PushRequest {
            encrypted_blob: B64.encode(b"data"),
        })
        .send()
        .await
        .context("push invalid token")?;
    assert_eq!(resp.status(), 401, "expect 401 with invalid token");

    Ok(())
}

/// Wrong passphrase in auth returns 401.
#[tokio::test]
async fn test_auth_wrong_passphrase_is_401() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    let resp = client
        .post(server.url("/api/challenge"))
        .json(&ChallengeRequest {
            public_key: TEST_PK_A,
        })
        .send()
        .await
        .context("challenge")?;
    assert_eq!(resp.status(), 200, "challenge status");
    let challenge: ChallengeResponse = resp.json().await.context("challenge json")?;
    let counter = solve_pow(&challenge.nonce, challenge.difficulty);

    let resp = client
        .post(server.url("/api/auth"))
        .json(&AuthRequest {
            public_key: TEST_PK_A,
            nonce: challenge.nonce,
            counter,
            passphrase: "WRONG_PASSPHRASE",
            device_name: "Test",
        })
        .send()
        .await
        .context("auth wrong pass")?;
    assert_eq!(resp.status(), 401, "expect 401 for wrong passphrase");

    Ok(())
}

/// 400 for invalid public_key format.
#[tokio::test]
async fn test_challenge_invalid_public_key_is_400() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    // Too short
    let resp = client
        .post(server.url("/api/challenge"))
        .json(&ChallengeRequest {
            public_key: "tooshort",
        })
        .send()
        .await
        .context("challenge short key")?;
    assert_eq!(resp.status(), 400, "short key → 400");

    // 64 chars but contains non-hex
    let resp = client
        .post(server.url("/api/challenge"))
        .json(&ChallengeRequest {
            public_key: "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ",
        })
        .send()
        .await
        .context("challenge non-hex key")?;
    assert_eq!(resp.status(), 400, "non-hex key → 400");

    Ok(())
}

/// Pull with no blobs returns an empty list.
#[tokio::test]
async fn test_pull_empty_returns_empty_list() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    let token = authenticate(&client, &server, TEST_PK_A, "testpass", "Empty Test").await?;

    let resp = client
        .get(server.url("/api/sync/pull?since=0"))
        .bearer_auth(&token)
        .send()
        .await
        .context("pull empty")?;
    assert_eq!(resp.status(), 200, "pull empty status");
    let pull: PullResponse = resp.json().await.context("pull json")?;
    assert!(pull.blobs.is_empty(), "no blobs pushed yet");
    assert_eq!(pull.latest_sequence, 0);

    Ok(())
}

/// Two different public keys have isolated blob namespaces.
#[tokio::test]
async fn test_two_accounts_isolated() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    // Account A pushes one blob
    let token_a = authenticate(&client, &server, TEST_PK_A, "testpass", "Device A").await?;
    let blob_a = B64.encode(b"secret-data-account-a");
    let resp = client
        .post(server.url("/api/sync/push"))
        .bearer_auth(&token_a)
        .json(&PushRequest {
            encrypted_blob: blob_a,
        })
        .send()
        .await
        .context("push a")?;
    assert_eq!(resp.status(), 200, "push a status");

    // Account B authenticates independently
    let token_b = authenticate(&client, &server, TEST_PK_B, "testpass", "Device B").await?;

    // B pulls — should see zero blobs (isolation check)
    let resp = client
        .get(server.url("/api/sync/pull?since=0"))
        .bearer_auth(&token_b)
        .send()
        .await
        .context("pull b")?;
    assert_eq!(resp.status(), 200, "pull b status");
    let pull_b: PullResponse = resp.json().await.context("pull b json")?;
    assert!(
        pull_b.blobs.is_empty(),
        "account B should see no blobs from account A"
    );

    // A's status shows 1 blob
    let resp = client
        .get(server.url("/api/sync/status"))
        .bearer_auth(&token_a)
        .send()
        .await
        .context("status a")?;
    assert_eq!(resp.status(), 200, "status a code");
    let status_a: StatusResponse = resp.json().await.context("status a json")?;
    assert_eq!(status_a.blob_count, 1);

    Ok(())
}

/// Health check endpoint is accessible without auth.
#[tokio::test]
async fn test_health_check_is_public() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    let resp = client
        .get(server.url("/api/health"))
        .send()
        .await
        .context("health")?;
    assert_eq!(resp.status(), 200, "health status");
    let body: serde_json::Value = resp.json().await.context("health json")?;
    assert_eq!(
        body.get("status").and_then(serde_json::Value::as_str),
        Some("ok")
    );

    Ok(())
}

/// Re-issuing a challenge for the same public key immediately works (prior one is deleted).
#[tokio::test]
async fn test_challenge_can_be_reissued_for_same_key() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    // Get two challenges in a row for the same key
    let resp1 = client
        .post(server.url("/api/challenge"))
        .json(&ChallengeRequest {
            public_key: TEST_PK_A,
        })
        .send()
        .await
        .context("challenge 1")?;
    assert_eq!(resp1.status(), 200, "challenge 1 status");
    let c1: ChallengeResponse = resp1.json().await.context("c1")?;

    let resp2 = client
        .post(server.url("/api/challenge"))
        .json(&ChallengeRequest {
            public_key: TEST_PK_A,
        })
        .send()
        .await
        .context("challenge 2")?;
    assert_eq!(resp2.status(), 200, "challenge 2 status");
    let c2: ChallengeResponse = resp2.json().await.context("c2")?;

    // Nonces should differ (random each time)
    assert_ne!(c1.nonce, c2.nonce, "each challenge has a unique nonce");

    // Only the second challenge is valid — authenticate with it
    let counter = solve_pow(&c2.nonce, c2.difficulty);
    let resp = client
        .post(server.url("/api/auth"))
        .json(&AuthRequest {
            public_key: TEST_PK_A,
            nonce: c2.nonce,
            counter,
            passphrase: "testpass",
            device_name: "ReIssue Test",
        })
        .send()
        .await
        .context("auth after reissue")?;
    assert_eq!(resp.status(), 200, "auth with second challenge succeeds");

    Ok(())
}

/// Sequence numbers increase monotonically per account.
#[tokio::test]
async fn test_sequence_numbers_monotonic() -> Result<()> {
    let server = TestServer::start("testpass", 4).await?;
    let client = Client::new();

    let token = authenticate(&client, &server, TEST_PK_A, "testpass", "Seq Test").await?;

    let mut prev_seq = 0i64;
    for i in 0..5u8 {
        let data = B64.encode(format!("blob-{i}").as_bytes());
        let resp = client
            .post(server.url("/api/sync/push"))
            .bearer_auth(&token)
            .json(&PushRequest {
                encrypted_blob: data,
            })
            .send()
            .await
            .context("push seq")?;
        assert_eq!(resp.status(), 200, "push seq {i} status");
        let push: PushResponse = resp.json().await.context("push seq json")?;
        assert!(
            push.sequence > prev_seq,
            "sequence must increase: {prev_seq} → {}",
            push.sequence
        );
        prev_seq = push.sequence;
    }
    assert_eq!(prev_seq, 5, "5 pushes → sequence 5");

    Ok(())
}
