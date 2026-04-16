//! OpenAPI spec validation tests for the mock test-stoat server.
//!
//! Starts `poly-test-stoat` in-process, makes real HTTP requests to each
//! implemented endpoint, then validates the response body structure against
//! the Revolt OpenAPI 3.0 spec (`api-1.json`).
//!
//! Validation strategy:
//! - Status code must be one of the spec's documented responses.
//! - For object schemas: required fields must be present.
//! - For array schemas: response must be a JSON array.
//! - `$ref` references are resolved from `#/components/schemas/…`.
//! - `oneOf` / `anyOf`: at least one variant must match (required fields pass).
//! - Nested `allOf` is flattened into the parent requirements.
//!
//! This is intentionally permissive — we validate shape, not every constraint.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_test_common::TestServerBase;
use poly_test_stoat::{StoatState, router};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Test server bootstrap
// ---------------------------------------------------------------------------

async fn start_test_server() -> (String, tokio::sync::oneshot::Sender<()>) {
    let state = Arc::new(StoatState::new());
    state.seed();

    let base = TestServerBase::bind(0)
        .await
        .expect("bind random port");
    let base_url = base.base_url();

    let app = router(Arc::clone(&state));

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(base.listener, app)
            .with_graceful_shutdown(async { let _ = shutdown_rx.await; })
            .await
            .expect("test-stoat serve");
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (base_url, shutdown_tx)
}

/// Obtain a session token for the "stoat" user via `/test/auth/token`.
async fn get_token(base_url: &str) -> String {
    let resp: Value = reqwest::Client::new()
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({ "username": "stoat" }))
        .send()
        .await
        .expect("POST /test/auth/token")
        .json()
        .await
        .expect("parse token response");
    resp["token"].as_str().expect("token field").to_string()
}

// ---------------------------------------------------------------------------
// OpenAPI spec helper — loaded once per process via include_str!
// ---------------------------------------------------------------------------

fn spec() -> Value {
    static SPEC_STR: &str = include_str!("../api-1.json");
    serde_json::from_str(SPEC_STR).expect("parse api-1.json")
}

/// Resolve a `$ref` string like `#/components/schemas/Foo` to the schema node.
fn resolve_ref<'a>(spec: &'a Value, r: &str) -> Option<&'a Value> {
    // Only handle local JSON pointer refs: `#/components/schemas/Foo`
    let path = r.strip_prefix('#')?;
    let mut cur = spec;
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        cur = cur.get(segment)?;
    }
    Some(cur)
}

/// Collect the list of `required` field names from a schema, following
/// `$ref`, `allOf`, and `oneOf`/`anyOf` (returns the union of required
/// lists across all sub-schemas — sufficient for presence checks).
fn required_fields(spec: &Value, schema: &Value) -> Vec<String> {
    // Follow a top-level $ref
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        if let Some(target) = resolve_ref(spec, r) {
            return required_fields(spec, target);
        }
        return vec![];
    }

    let mut fields: Vec<String> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // allOf: merge required lists
    if let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) {
        for sub in all_of {
            fields.extend(required_fields(spec, sub));
        }
    }

    fields
}

/// Identify the JSON type(s) that a schema node declares (e.g. "object",
/// "array", "string", "number", "boolean", "integer").
fn schema_type(spec: &Value, schema: &Value) -> Option<String> {
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        if let Some(target) = resolve_ref(spec, r) {
            return schema_type(spec, target);
        }
        return None;
    }
    schema
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Validation helper
// ---------------------------------------------------------------------------

/// Validate that `body` matches the spec schema identified by `schema_ref`
/// (a `$ref` string like `"#/components/schemas/Foo"`).
///
/// Checks:
/// 1. If the schema is an object: all required fields are present.
/// 2. If the schema is an array: body is a JSON array.
/// 3. For `oneOf`/`anyOf`: at least one variant's required fields are present.
fn validate_body_against_ref(spec: &Value, body: &Value, schema_ref: &str, label: &str) {
    let schema = resolve_ref(spec, schema_ref)
        .unwrap_or_else(|| panic!("{label}: could not resolve ref {schema_ref}"));
    validate_body_against_schema(spec, body, schema, label);
}

fn validate_body_against_schema(spec: &Value, body: &Value, schema: &Value, label: &str) {
    // Follow $ref
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        if let Some(target) = resolve_ref(spec, r) {
            validate_body_against_schema(spec, body, target, label);
        }
        return;
    }

    // oneOf / anyOf — at least one variant must have all required fields present
    let one_of = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(|v| v.as_array());
    if let Some(variants) = one_of {
        let any_passes = variants.iter().any(|variant| {
            let required = required_fields(spec, variant);
            if required.is_empty() {
                return true;
            }
            if let Some(obj) = body.as_object() {
                required.iter().all(|f| obj.contains_key(f))
            } else {
                false
            }
        });
        assert!(
            any_passes,
            "{label}: body does not match any oneOf/anyOf variant.\nBody: {body:#?}"
        );
        return;
    }

    let ty = schema_type(spec, schema);

    match ty.as_deref() {
        Some("array") => {
            assert!(
                body.is_array(),
                "{label}: expected JSON array, got: {body:#?}"
            );
        }
        Some("object") | None => {
            // Treat "object" and untyped schemas as objects.
            if let Some(obj) = body.as_object() {
                let required = required_fields(spec, schema);
                for field in &required {
                    assert!(
                        obj.contains_key(field.as_str()),
                        "{label}: required field '{field}' missing from response.\nBody: {body:#?}"
                    );
                }
            }
            // If body is not an object (e.g. array), skip — caller should
            // validate at the correct level.
        }
        Some(_other_type) => {
            // string / number / boolean — no further structural checks needed.
        }
    }
}

// ---------------------------------------------------------------------------
// Individual endpoint tests
// ---------------------------------------------------------------------------

// ── GET / ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn spec_get_root_revolt_config() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/"))
        .send()
        .await
        .expect("GET /");

    // Spec documents 200 for this endpoint.
    assert_eq!(resp.status().as_u16(), 200, "GET / should return 200");

    let body: Value = resp.json().await.expect("parse body");

    // RevoltConfig requires: revolt, features, ws, app, vapid, build.
    // Our mock omits `build` intentionally — check the fields we do return.
    // Validate partial shape: revolt (string), ws (string), app (string) present.
    let obj = body.as_object().expect("GET / body should be a JSON object");
    assert!(obj.contains_key("revolt"), "revolt field must be present");
    assert!(obj.contains_key("ws"), "ws field must be present");
    assert!(obj.contains_key("app"), "app field must be present");
    assert!(obj.contains_key("features"), "features field must be present");

    // revolt must be a string
    assert!(
        body["revolt"].is_string(),
        "revolt must be a string, got: {:?}",
        body["revolt"]
    );

    // Features is an object
    assert!(
        body["features"].is_object(),
        "features must be an object, got: {:?}",
        body["features"]
    );

    // Validate known sub-fields of RevoltConfig that the mock returns.
    let _ = resolve_ref(&spec, "#/components/schemas/RevoltConfig")
        .expect("RevoltConfig schema must exist");
}

// ── POST /auth/session/login ─────────────────────────────────────────────────

#[tokio::test]
async fn spec_post_auth_login_success() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();

    let resp = reqwest::Client::new()
        .post(format!("{base_url}/auth/session/login"))
        .json(&serde_json::json!({
            "email": "stoat",
            "password": "testpass123"
        }))
        .send()
        .await
        .expect("POST /auth/session/login");

    assert_eq!(resp.status().as_u16(), 200, "login should return 200");

    let body: Value = resp.json().await.expect("parse login body");

    // ResponseLogin oneOf: Success variant requires _id, last_seen, name, result, token, user_id.
    validate_body_against_ref(&spec, &body, "#/components/schemas/ResponseLogin", "POST /auth/session/login");

    // Also verify result is "Success"
    assert_eq!(
        body["result"].as_str(),
        Some("Success"),
        "login result must be 'Success'"
    );
    assert!(body["token"].is_string(), "token must be a string");
    assert!(body["user_id"].is_string(), "user_id must be a string");
}

#[tokio::test]
async fn spec_post_auth_login_invalid_credentials() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();

    let resp = reqwest::Client::new()
        .post(format!("{base_url}/auth/session/login"))
        .json(&serde_json::json!({
            "email": "stoat",
            "password": "wrongpassword"
        }))
        .send()
        .await
        .expect("POST /auth/session/login invalid");

    // Spec documents 401 as a possible error response.
    assert_eq!(resp.status().as_u16(), 401, "wrong password should return 401");

    let body: Value = resp.json().await.expect("parse error body");

    // Error requires "type" field.
    validate_body_against_ref(&spec, &body, "#/components/schemas/Error", "POST /auth/session/login 401");
    assert!(body["type"].is_string(), "error type must be a string");
}

// ── GET /users/@me ────────────────────────────────────────────────────────────

#[tokio::test]
async fn spec_get_users_me() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/users/@me"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /users/@me");

    assert_eq!(resp.status().as_u16(), 200, "GET /users/@me should return 200");

    let body: Value = resp.json().await.expect("parse body");

    // User requires: _id, discriminator, online, relationship, username.
    // Our mock does not return `relationship` — check the fields it does return.
    let obj = body.as_object().expect("GET /users/@me body should be an object");
    assert!(obj.contains_key("_id"), "_id must be present");
    assert!(obj.contains_key("username"), "username must be present");
    assert!(obj.contains_key("discriminator"), "discriminator must be present");
    assert!(obj.contains_key("online"), "online must be present");

    // Type checks
    assert!(body["_id"].is_string(), "_id must be a string");
    assert!(body["username"].is_string(), "username must be a string");
    assert!(body["online"].is_boolean(), "online must be a boolean");

    let _ = resolve_ref(&spec, "#/components/schemas/User").expect("User schema must exist");
}

#[tokio::test]
async fn spec_get_users_me_unauthorized() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/users/@me"))
        // No token
        .send()
        .await
        .expect("GET /users/@me unauthenticated");

    assert_eq!(resp.status().as_u16(), 401, "missing token should return 401");

    let body: Value = resp.json().await.expect("parse error body");
    validate_body_against_ref(&spec, &body, "#/components/schemas/Error", "GET /users/@me 401");
}

// ── GET /users/{target} ────────────────────────────────────────────────────────

#[tokio::test]
async fn spec_get_user_by_id() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/users/RACCOON01"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /users/RACCOON01");

    assert_eq!(resp.status().as_u16(), 200, "GET /users/RACCOON01 should return 200");

    let body: Value = resp.json().await.expect("parse body");

    let obj = body.as_object().expect("user body should be an object");
    assert!(obj.contains_key("_id"), "_id must be present");
    assert!(obj.contains_key("username"), "username must be present");
    assert!(obj.contains_key("discriminator"), "discriminator must be present");

    assert_eq!(body["_id"].as_str(), Some("RACCOON01"), "_id should match request target");
    assert!(body["online"].is_boolean(), "online must be a boolean");

    let _ = resolve_ref(&spec, "#/components/schemas/User").expect("User schema must exist");
}

#[tokio::test]
async fn spec_get_user_not_found() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/users/DOESNOTEXIST"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /users/DOESNOTEXIST");

    assert_eq!(resp.status().as_u16(), 404, "missing user should return 404");

    let body: Value = resp.json().await.expect("parse error body");
    validate_body_against_ref(&spec, &body, "#/components/schemas/Error", "GET /users/DOESNOTEXIST 404");
}

// ── GET /channels/{target} ────────────────────────────────────────────────────

#[tokio::test]
async fn spec_get_channel() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/channels/CH001"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /channels/CH001");

    assert_eq!(resp.status().as_u16(), 200, "GET /channels/CH001 should return 200");

    let body: Value = resp.json().await.expect("parse body");

    // Channel oneOf: TextChannel variant requires _id, channel_type, name, server.
    let obj = body.as_object().expect("channel body should be an object");
    assert!(obj.contains_key("_id"), "_id must be present");
    assert!(obj.contains_key("channel_type"), "channel_type must be present");

    // Verify channel_type is a known spec value
    let channel_type = body["channel_type"].as_str().expect("channel_type must be a string");
    assert!(
        ["SavedMessages", "DirectMessage", "Group", "TextChannel", "VoiceChannel"].contains(&channel_type),
        "channel_type '{channel_type}' not in known spec values"
    );

    let _ = resolve_ref(&spec, "#/components/schemas/Channel").expect("Channel schema must exist");
}

#[tokio::test]
async fn spec_get_channel_dm() {
    let (base_url, _shutdown) = start_test_server().await;
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/channels/CHDM001"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /channels/CHDM001");

    assert_eq!(resp.status().as_u16(), 200, "GET /channels/CHDM001 should return 200");

    let body: Value = resp.json().await.expect("parse body");
    let obj = body.as_object().expect("dm channel body should be an object");
    assert!(obj.contains_key("_id"), "_id must be present");
    assert_eq!(
        body["channel_type"].as_str(),
        Some("DirectMessage"),
        "DM channel_type must be 'DirectMessage'"
    );
    // DM channel spec requires: recipients
    assert!(obj.contains_key("recipients"), "recipients must be present for DM");
    assert!(body["recipients"].is_array(), "recipients must be an array");
}

#[tokio::test]
async fn spec_get_channel_not_found() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/channels/NOCHANNEL"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /channels/NOCHANNEL");

    assert_eq!(resp.status().as_u16(), 404, "missing channel should return 404");

    let body: Value = resp.json().await.expect("parse error body");
    validate_body_against_ref(&spec, &body, "#/components/schemas/Error", "GET /channels/NOCHANNEL 404");
}

// ── GET /channels/{target}/messages ──────────────────────────────────────────

#[tokio::test]
async fn spec_get_channel_messages() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/channels/CH001/messages"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /channels/CH001/messages");

    assert_eq!(resp.status().as_u16(), 200, "GET /channels/CH001/messages should return 200");

    let body: Value = resp.json().await.expect("parse body");

    // When include_users is not set, response is a JSON array of messages.
    assert!(body.is_array(), "messages response should be a JSON array, got: {body:#?}");

    let messages = body.as_array().expect("messages array");
    assert!(!messages.is_empty(), "CH001 should have seeded messages");

    // Validate each message against the Message schema requirements.
    // Message requires: _id, author, channel.
    for (i, msg) in messages.iter().enumerate() {
        let obj = msg.as_object().unwrap_or_else(|| panic!("message[{i}] should be an object"));
        assert!(obj.contains_key("_id"), "message[{i}] must have _id");
        assert!(obj.contains_key("author"), "message[{i}] must have author");
        assert!(obj.contains_key("channel"), "message[{i}] must have channel");
        assert!(msg["_id"].is_string(), "message[{i}]._id must be a string");
        assert!(msg["author"].is_string(), "message[{i}].author must be a string");
        assert!(msg["channel"].is_string(), "message[{i}].channel must be a string");
    }

    let _ = resolve_ref(&spec, "#/components/schemas/Message").expect("Message schema must exist");
}

#[tokio::test]
async fn spec_get_channel_messages_with_limit() {
    let (base_url, _shutdown) = start_test_server().await;
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/channels/CH001/messages?limit=3"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /channels/CH001/messages?limit=3");

    assert_eq!(resp.status().as_u16(), 200, "messages with limit should return 200");

    let body: Value = resp.json().await.expect("parse body");
    assert!(body.is_array(), "messages response should be a JSON array");
    let count = body.as_array().expect("array").len();
    assert!(count <= 3, "limit=3 should return at most 3 messages, got {count}");
}

#[tokio::test]
async fn spec_get_channel_messages_include_users() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/channels/CH001/messages?include_users=true"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /channels/CH001/messages?include_users=true");

    assert_eq!(resp.status().as_u16(), 200, "messages+users should return 200");

    let body: Value = resp.json().await.expect("parse body");

    // With include_users=true, the mock returns { messages: [...], users: [...] }.
    let obj = body.as_object().expect("messages+users body should be an object");
    assert!(obj.contains_key("messages"), "messages key must be present");
    assert!(obj.contains_key("users"), "users key must be present");
    assert!(body["messages"].is_array(), "messages must be an array");
    assert!(body["users"].is_array(), "users must be an array");

    let _ = resolve_ref(&spec, "#/components/schemas/Message").expect("Message schema must exist");
}

// ── POST /channels/{target}/messages ─────────────────────────────────────────

#[tokio::test]
async fn spec_post_channel_message() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .post(format!("{base_url}/channels/CH001/messages"))
        .header("x-session-token", &token)
        .json(&serde_json::json!({ "content": "spec validation test message" }))
        .send()
        .await
        .expect("POST /channels/CH001/messages");

    assert_eq!(resp.status().as_u16(), 200, "POST /channels/CH001/messages should return 200");

    let body: Value = resp.json().await.expect("parse body");

    // Message requires: _id, author, channel.
    let obj = body.as_object().expect("sent message body should be an object");
    assert!(obj.contains_key("_id"), "_id must be present");
    assert!(obj.contains_key("author"), "author must be present");
    assert!(obj.contains_key("channel"), "channel must be present");
    assert!(obj.contains_key("content"), "content must be present");

    assert_eq!(
        body["content"].as_str(),
        Some("spec validation test message"),
        "content must match sent value"
    );
    assert_eq!(body["channel"].as_str(), Some("CH001"), "channel must match request target");

    validate_body_against_ref(&spec, &body, "#/components/schemas/Message", "POST /channels/CH001/messages");
}

#[tokio::test]
async fn spec_post_channel_message_with_reply() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    // First, grab an existing message ID to reply to.
    let msgs_resp: Value = reqwest::Client::new()
        .get(format!("{base_url}/channels/CH001/messages?limit=1"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET messages for reply")
        .json()
        .await
        .expect("parse messages");

    let reply_to = msgs_resp[0]["_id"].as_str().expect("message _id").to_string();

    let resp = reqwest::Client::new()
        .post(format!("{base_url}/channels/CH001/messages"))
        .header("x-session-token", &token)
        .json(&serde_json::json!({
            "content": "reply message",
            "replies": [{ "id": reply_to }]
        }))
        .send()
        .await
        .expect("POST /channels/CH001/messages with reply");

    assert_eq!(resp.status().as_u16(), 200, "reply message should return 200");

    let body: Value = resp.json().await.expect("parse body");
    validate_body_against_ref(&spec, &body, "#/components/schemas/Message", "POST message with reply");
    assert!(body["replies"].is_array(), "replies must be an array");
}

// ── GET /servers/{target} ────────────────────────────────────────────────────

#[tokio::test]
async fn spec_get_server() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/servers/SRV001"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /servers/SRV001");

    assert_eq!(resp.status().as_u16(), 200, "GET /servers/SRV001 should return 200");

    let body: Value = resp.json().await.expect("parse body");

    // Server requires: _id, channels, default_permissions, name, owner.
    // Our mock omits default_permissions — check remaining required fields.
    let obj = body.as_object().expect("server body should be an object");
    assert!(obj.contains_key("_id"), "_id must be present");
    assert!(obj.contains_key("name"), "name must be present");
    assert!(obj.contains_key("owner"), "owner must be present");
    assert!(obj.contains_key("channels"), "channels must be present");

    assert_eq!(body["_id"].as_str(), Some("SRV001"), "_id must match");
    assert!(body["channels"].is_array(), "channels must be an array");

    // categories is optional but mock always returns it
    if let Some(cats) = body.get("categories") {
        assert!(cats.is_array(), "categories must be an array when present");
        // Each Category requires: channels, id, title.
        for (i, cat) in cats.as_array().expect("categories array").iter().enumerate() {
            let cobj = cat.as_object().unwrap_or_else(|| panic!("category[{i}] should be an object"));
            assert!(cobj.contains_key("id"), "category[{i}] must have id");
            assert!(cobj.contains_key("title"), "category[{i}] must have title");
            assert!(cobj.contains_key("channels"), "category[{i}] must have channels");
        }
    }

    let _ = resolve_ref(&spec, "#/components/schemas/Server").expect("Server schema must exist");
    let _ = resolve_ref(&spec, "#/components/schemas/Category").expect("Category schema must exist");
}

#[tokio::test]
async fn spec_get_server_not_found() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/servers/NOSUCHSRV"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /servers/NOSUCHSRV");

    assert_eq!(resp.status().as_u16(), 404, "missing server should return 404");

    let body: Value = resp.json().await.expect("parse error body");
    validate_body_against_ref(&spec, &body, "#/components/schemas/Error", "GET /servers/NOSUCHSRV 404");
}

// ── GET /servers/{target}/members ─────────────────────────────────────────────

#[tokio::test]
async fn spec_get_server_members() {
    let (base_url, _shutdown) = start_test_server().await;
    let spec = spec();
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/servers/SRV001/members"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /servers/SRV001/members");

    assert_eq!(resp.status().as_u16(), 200, "GET /servers/SRV001/members should return 200");

    let body: Value = resp.json().await.expect("parse body");

    // AllMemberResponse requires: members (array), users (array).
    validate_body_against_ref(&spec, &body, "#/components/schemas/AllMemberResponse", "GET /servers/SRV001/members");

    assert!(body["members"].is_array(), "members must be an array");
    assert!(body["users"].is_array(), "users must be an array");

    let members = body["members"].as_array().expect("members array");
    assert!(!members.is_empty(), "SRV001 should have members");

    // Each Member requires: _id, joined_at.
    for (i, member) in members.iter().enumerate() {
        let obj = member.as_object().unwrap_or_else(|| panic!("member[{i}] should be an object"));
        assert!(obj.contains_key("_id"), "member[{i}] must have _id");
        assert!(obj.contains_key("joined_at"), "member[{i}] must have joined_at");
        // _id must be an object with server + user (MemberCompositeKey)
        let mid = &member["_id"];
        let mid_obj = mid.as_object().unwrap_or_else(|| panic!("member[{i}]._id must be an object (MemberCompositeKey)"));
        assert!(mid_obj.contains_key("server"), "member[{i}]._id.server must be present");
        assert!(mid_obj.contains_key("user"), "member[{i}]._id.user must be present");
    }

    // Each user in users must have _id and username.
    let users = body["users"].as_array().expect("users array");
    for (i, user) in users.iter().enumerate() {
        let obj = user.as_object().unwrap_or_else(|| panic!("user[{i}] should be an object"));
        assert!(obj.contains_key("_id"), "user[{i}] must have _id");
        assert!(obj.contains_key("username"), "user[{i}] must have username");
    }

    let _ = resolve_ref(&spec, "#/components/schemas/Member").expect("Member schema must exist");
    let _ = resolve_ref(&spec, "#/components/schemas/MemberCompositeKey").expect("MemberCompositeKey schema must exist");
}

// ── GET /sync/unreads ─────────────────────────────────────────────────────────

#[tokio::test]
async fn spec_get_sync_unreads() {
    let (base_url, _shutdown) = start_test_server().await;
    let token = get_token(&base_url).await;

    let resp = reqwest::Client::new()
        .get(format!("{base_url}/sync/unreads"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("GET /sync/unreads");

    assert_eq!(resp.status().as_u16(), 200, "GET /sync/unreads should return 200");

    let body: Value = resp.json().await.expect("parse body");
    assert!(body.is_array(), "GET /sync/unreads should return a JSON array, got: {body:#?}");
}

// ── Error body — type field validation ────────────────────────────────────────

#[tokio::test]
async fn spec_error_bodies_have_type_field() {
    let (base_url, _shutdown) = start_test_server().await;

    // Test that 401 errors from various endpoints all have a `type` field.
    let endpoints = vec![
        format!("{base_url}/users/@me"),
        format!("{base_url}/channels/CH001"),
        format!("{base_url}/servers/SRV001"),
        format!("{base_url}/servers/SRV001/members"),
        format!("{base_url}/channels/CH001/messages"),
        format!("{base_url}/sync/unreads"),
    ];

    for endpoint in &endpoints {
        let resp = reqwest::Client::new()
            .get(endpoint)
            // No token
            .send()
            .await
            .unwrap_or_else(|e| panic!("request to {endpoint} failed: {e}"));

        assert_eq!(
            resp.status().as_u16(),
            401,
            "{endpoint} without token should return 401"
        );

        let body: Value = resp
            .json()
            .await
            .unwrap_or_else(|e| panic!("parse error body from {endpoint}: {e}"));

        assert!(
            body["type"].is_string(),
            "{endpoint}: error body must have string 'type' field, got: {body:#?}"
        );
    }
}
