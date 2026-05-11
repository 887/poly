//! Phase B.7 — `X-Super-Properties` consistency + anti-ban guard tests.
//!
//! These tests fail-loud if:
//! - `X-Super-Properties` would be sent empty (the WASM silent-empty bug).
//! - `User-Agent` contains "DiscordBot" on a user-token request.
//! - The HTTP header and gateway IDENTIFY properties diverge.
//! - The `client_build_number` in the header doesn't match the `BuildInfo`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use poly_client::{AuthCredentials, IsBackend};
use poly_discord::build_info::{BuildInfo, LATEST_KNOWN_STABLE_BUILD};
use poly_discord::super_properties::SuperProperties;
use poly_test_discord::{DiscordState, router};
use tokio::net::TcpListener;

// ── Helpers ──────────────────────────────────────────────────────────────────

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let base_url = format!("http://127.0.0.1:{port}");

        let state = Arc::new(DiscordState::new());
        state.seed();
        *state.gateway_url.write().await = format!("ws://127.0.0.1:{port}/gateway/ws");

        let app = router(Arc::clone(&state));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    rx.await.ok();
                })
                .await
                .ok();
        });
        Self { base_url, _shutdown: tx }
    }

    async fn token_for(&self, username: &str) -> String {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": username }))
            .send()
            .await
            .expect("POST /test/auth/token")
            .json()
            .await
            .expect("parse token response");
        resp["token"].as_str().expect("token field").to_string()
    }

    async fn captured_headers(&self) -> Vec<serde_json::Value> {
        let body: serde_json::Value = reqwest::Client::new()
            .get(format!("{}/test/inspect/last-headers", self.base_url))
            .send()
            .await
            .expect("GET /test/inspect/last-headers")
            .json()
            .await
            .expect("parse inspect response");
        body.as_array().expect("array").clone()
    }
}

fn sample_build() -> BuildInfo {
    BuildInfo {
        build_number: LATEST_KNOWN_STABLE_BUILD,
        version_hash: "3eb5b4a".to_string(),
        scraped_at: 1_000_000,
    }
}

// ── Pure unit tests (no network) ─────────────────────────────────────────────

/// B.7 guard 1: User-Agent must never contain "DiscordBot" on a user-token path.
#[test]
fn ua_must_not_contain_discordbot() {
    let props = SuperProperties::for_platform(&sample_build(), "en-US");
    assert!(
        !props.browser_user_agent.contains("DiscordBot"),
        "User-Agent contains 'DiscordBot' — this is the #1 ban-bait signal. Got: {}",
        props.browser_user_agent
    );
}

/// B.7 guard 2: X-Super-Properties must not be empty.
#[test]
fn x_super_properties_not_empty() {
    let props = SuperProperties::for_platform(&sample_build(), "en-US");
    let header = props.to_header_value();
    assert!(
        !header.is_empty(),
        "X-Super-Properties is empty — WASM builds were sending this before the fix"
    );
}

/// B.7 guard 3: base64-decoded header parses to valid JSON with required fields.
#[test]
fn x_super_properties_valid_json_with_required_fields() {
    use base64::Engine as _;
    let props = SuperProperties::for_platform(&sample_build(), "en-US");
    let encoded = props.to_header_value();

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .expect("X-Super-Properties must be valid base64");

    let json: serde_json::Value =
        serde_json::from_slice(&decoded).expect("decoded X-Super-Properties must be valid JSON");

    for field in &[
        "os",
        "browser",
        "device",
        "system_locale",
        "browser_user_agent",
        "browser_version",
        "os_version",
        "referrer",
        "referring_domain",
        "referrer_current",
        "referring_domain_current",
        "release_channel",
        "client_build_number",
        "client_event_source",
    ] {
        assert!(
            json.get(field).is_some(),
            "Required field '{field}' is missing from X-Super-Properties JSON"
        );
    }

    assert!(
        json["client_event_source"].is_null(),
        "client_event_source must be JSON null, got: {}",
        json["client_event_source"]
    );
}

/// B.7 guard 4: client_build_number in header equals BuildInfo.build_number.
#[test]
fn build_number_consistent_http_vs_build_info() {
    use base64::Engine as _;
    let build = sample_build();
    let props = SuperProperties::for_platform(&build, "en-US");
    let encoded = props.to_header_value();

    let decoded =
        base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
    let bn = json["client_build_number"].as_u64().expect("client_build_number must be integer");

    assert_eq!(
        bn,
        u64::from(build.build_number),
        "client_build_number in X-Super-Properties ({bn}) must equal BuildInfo.build_number ({})",
        build.build_number
    );
}

/// Phase C.4 guard: gateway IDENTIFY properties JSON is byte-equal to
/// the JSON inside the base64-decoded X-Super-Properties.
#[test]
fn identify_properties_matches_http_header_json() {
    use base64::Engine as _;
    let build = sample_build();
    let props = SuperProperties::for_platform(&build, "en-US");

    // Decode the HTTP header.
    let encoded = props.to_header_value();
    let decoded =
        base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()).unwrap();
    let header_json: serde_json::Value = serde_json::from_slice(&decoded).unwrap();

    // Get the IDENTIFY properties (no base64, raw JSON object).
    let identify_json = props.to_identify_properties();

    assert_eq!(
        header_json, identify_json,
        "Gateway IDENTIFY properties must be byte-equal to X-Super-Properties JSON.\n\
         HTTP header JSON: {header_json:#?}\n\
         IDENTIFY JSON:    {identify_json:#?}"
    );
}

/// Phase B.5 guard: UA override propagates consistently (no DiscordBot leak).
#[test]
fn ua_override_never_leaks_discordbot() {
    let build = sample_build();
    let mut props = SuperProperties::for_platform(&build, "en-US");
    props.apply_ua_override("Mozilla/5.0 (Custom Client)");
    assert!(
        !props.browser_user_agent.contains("DiscordBot"),
        "UA override introduced 'DiscordBot'. Got: {}",
        props.browser_user_agent
    );
}

// ── Integration tests (require network — test server) ────────────────────────

/// Verify that the wire `X-Super-Properties` header is non-empty and valid.
#[tokio::test]
async fn wire_x_super_properties_non_empty_and_valid() {
    use base64::Engine as _;

    let srv = TestServer::start().await;
    let token = srv.token_for("koala").await;
    let mut client = poly_discord::DiscordClient::with_base_url(srv.base_url.clone());
    client
        .authenticate(poly_client::AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let _ = client.get_servers().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = srv.captured_headers().await;
    let found = entries.iter().any(|e| {
        let xsp = e["headers"]["x-super-properties"].as_str().unwrap_or("");
        if xsp.is_empty() {
            return false;
        }
        // Must be valid base64.
        let decoded = base64::engine::general_purpose::STANDARD.decode(xsp.as_bytes());
        decoded.is_ok()
    });

    assert!(
        found,
        "Expected non-empty valid-base64 X-Super-Properties on wire. Entries: {entries:#?}"
    );
}

/// Verify that the wire User-Agent does NOT contain "DiscordBot".
#[tokio::test]
async fn wire_ua_must_not_contain_discordbot() {
    let srv = TestServer::start().await;
    let token = srv.token_for("koala").await;
    let mut client = poly_discord::DiscordClient::with_base_url(srv.base_url.clone());
    client
        .authenticate(poly_client::AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let _ = client.get_servers().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let entries = srv.captured_headers().await;
    let bad = entries.iter().find(|e| {
        e["headers"]["user-agent"]
            .as_str()
            .map(|ua| ua.contains("DiscordBot"))
            .unwrap_or(false)
    });

    assert!(
        bad.is_none(),
        "Found 'DiscordBot' in wire User-Agent — ban-bait! Entry: {bad:#?}"
    );
}
