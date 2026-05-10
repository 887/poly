//! Integration tests for poly-matrix against the poly-test-matrix mock server.
//!
//! Each test spins up the mock Matrix homeserver in-process, authenticates a
//! client, and exercises the full `ClientBackend` trait surface.
//!
//! Run with:
//! ```
//! cargo test -p poly-matrix --features native
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;
use tokio::net::TcpListener;

use futures::StreamExt;

use poly_matrix::MatrixClient;
use poly_test_matrix::{MatrixState, router};

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind free port");
        let addr = listener.local_addr().expect("local_addr");
        let base_url = format!("http://{addr}");

        let state = Arc::new(MatrixState::new());
        state.seed();

        let app = router(state);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                })
                .await
                .expect("serve");
        });

        // Give the server a moment to start accepting connections.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        Self {
            base_url,
            _shutdown: shutdown_tx,
        }
    }
}

/// Obtain an access token for `username` (display name) from the test auth
/// helper endpoint.  Returns `(user_id, access_token)`.
async fn get_test_token(base_url: &str, username: &str) -> (String, String) {
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({ "username": username }))
        .send()
        .await
        .expect("POST /test/auth/token")
        .json()
        .await
        .expect("parse token response");

    let user_id = resp["user_id"].as_str().expect("user_id in response").to_string();
    let token = resp["access_token"].as_str().expect("access_token in response").to_string();
    (user_id, token)
}

/// Build a `MatrixClient` pointed at the test server.
fn make_client(base_url: &str) -> MatrixClient {
    MatrixClient::with_homeserver(base_url).expect("valid homeserver URL")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_authenticate_with_token() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    assert!(!client.is_authenticated());

    let session = client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    assert!(client.is_authenticated());
    assert!(!session.id.is_empty());
    assert_eq!(session.user.id, "@owl:localhost");
    assert_eq!(session.backend, BackendType::from("matrix"));
}

#[tokio::test]
async fn test_authenticate_with_password() {
    let srv = TestServer::start().await;

    let mut client = make_client(&srv.base_url);

    let session = client
        .authenticate(AuthCredentials::EmailPassword {
            email: "owl".to_string(),
            password: "testpass123".to_string(),
        })
        .await
        .expect("password authenticate");

    assert!(client.is_authenticated());
    assert_eq!(session.user.id, "@owl:localhost");
    assert_eq!(session.backend, BackendType::from("matrix"));
}

#[tokio::test]
async fn test_logout() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");
    assert!(client.is_authenticated());

    client.logout().await.expect("logout");
    assert!(!client.is_authenticated());
}

#[tokio::test]
async fn test_get_servers() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let servers = client.get_servers().await.expect("get_servers");
    assert!(!servers.is_empty(), "Expected at least one space");
    assert!(
        servers.iter().any(|s| s.name == "The Hollow Tree"),
        "Expected 'The Hollow Tree' space: {servers:?}"
    );
    assert!(
        servers.iter().any(|s| s.name == "Neon Reef"),
        "Expected 'Neon Reef' space: {servers:?}"
    );
    for server in &servers {
        assert_eq!(server.backend, BackendType::from("matrix"));
    }
}

#[tokio::test]
async fn test_get_server() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let server = client
        .get_server("!space1:localhost")
        .await
        .expect("get_server");

    assert_eq!(server.id, "!space1:localhost");
    assert_eq!(server.name, "The Hollow Tree");
    assert_eq!(server.backend, BackendType::from("matrix"));
}

#[tokio::test]
async fn test_get_channels() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let channels = client
        .get_channels("!space1:localhost")
        .await
        .expect("get_channels");

    assert!(!channels.is_empty(), "Expected channels in space1");
    // The space hierarchy should include general, random, announcements
    let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
    assert!(
        names.contains(&"general"),
        "Expected 'general' channel: {names:?}"
    );
}

#[tokio::test]
async fn test_get_channel() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let channel = client
        .get_channel("!general1:localhost")
        .await
        .expect("get_channel");

    assert_eq!(channel.id, "!general1:localhost");
    assert_eq!(channel.name, "general");
}

#[tokio::test]
async fn test_get_messages() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let messages = client
        .get_messages(
            "!general1:localhost",
            MessageQuery {
                before: None,
                after: None,
                around: None,
                limit: Some(50),
            },
        )
        .await
        .expect("get_messages");

    // The seeded general1 channel has 5 messages
    assert!(!messages.is_empty(), "Expected seeded messages");
    for msg in &messages {
        assert!(!msg.id.is_empty());
        assert_eq!(msg.author.backend, BackendType::from("matrix"));
        assert!(matches!(&msg.content, MessageContent::Text(_)));
    }
}

#[tokio::test]
async fn test_send_message() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let msg = client
        .send_message(
            "!general1:localhost",
            MessageContent::Text("Hello from integration test!".to_string()),
        )
        .await
        .expect("send_message");

    assert!(!msg.id.is_empty());
    assert!(matches!(&msg.content, MessageContent::Text(t) if t == "Hello from integration test!"));
    assert_eq!(msg.author.id, "@owl:localhost");
}

#[tokio::test]
async fn test_send_then_read_message() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let body = "Send-then-read test message";

    client
        .send_message(
            "!random1:localhost",
            MessageContent::Text(body.to_string()),
        )
        .await
        .expect("send_message");

    let messages = client
        .get_messages(
            "!random1:localhost",
            MessageQuery {
                before: None,
                after: None,
                around: None,
                limit: Some(50),
            },
        )
        .await
        .expect("get_messages after send");

    let found = messages
        .iter()
        .any(|m| matches!(&m.content, MessageContent::Text(t) if t == body));

    assert!(
        found,
        "Sent message not found in get_messages result: {messages:?}"
    );
}

#[tokio::test]
async fn test_get_user() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let user = client
        .get_user("@axolotl:localhost")
        .await
        .expect("get_user");

    assert_eq!(user.id, "@axolotl:localhost");
    assert_eq!(user.display_name, "Axolotl");
    assert_eq!(user.backend, BackendType::from("matrix"));
}

#[tokio::test]
async fn test_backend_type_and_name() {
    let client = make_client("http://localhost:12345");
    assert_eq!(client.backend_type(), BackendType::from("matrix"));
    assert_eq!(client.backend_name(), "Matrix");
}

#[tokio::test]
async fn test_presence_stubs() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    // get_presence returns Offline (stub)
    let presence = client
        .get_presence("@axolotl:localhost")
        .await
        .expect("get_presence");
    assert_eq!(presence, poly_client::PresenceStatus::Offline);

    // set_presence returns Ok (stub)
    client
        .set_presence(poly_client::PresenceStatus::Online)
        .await
        .expect("set_presence");
}

#[tokio::test]
async fn test_send_reply_message() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    // First send a message to reply to.
    let original = client
        .send_message(
            "!general1:localhost",
            MessageContent::Text("Original message".to_string()),
        )
        .await
        .expect("send original");

    let reply = client
        .send_reply_message(
            "!general1:localhost",
            &original.id,
            MessageContent::Text("This is a reply".to_string()),
        )
        .await
        .expect("send reply");

    assert!(!reply.id.is_empty());
    assert!(
        matches!(&reply.content, MessageContent::Text(t) if t == "This is a reply")
    );
}

#[tokio::test]
async fn test_get_channel_members() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let members = client
        .get_channel_members("!general1:localhost")
        .await
        .expect("get_channel_members");

    assert!(!members.is_empty(), "Expected members in general1");
    let ids: Vec<&str> = members.iter().map(|m| m.id.as_str()).collect();
    assert!(
        ids.contains(&"@owl:localhost"),
        "Expected owl in members: {ids:?}"
    );
    assert!(
        ids.contains(&"@axolotl:localhost"),
        "Expected axolotl in members: {ids:?}"
    );
}

// ---------------------------------------------------------------------------
// Sync / notification tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sync_initial() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let resp: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/_matrix/client/v3/sync", srv.base_url))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("GET /sync")
        .json()
        .await
        .expect("parse sync response");

    // Response must have next_batch and rooms.join
    assert!(
        resp["next_batch"].is_string(),
        "Expected next_batch token, got: {resp}"
    );

    let join = resp["rooms"]["join"].as_object().expect("rooms.join must be an object");

    // owl is in !general1:localhost — it must appear in the initial sync
    assert!(
        join.contains_key("!general1:localhost"),
        "Expected !general1:localhost in rooms.join, got keys: {:?}",
        join.keys().collect::<Vec<_>>()
    );

    // The timeline for !general1:localhost should have seeded events
    let timeline_events = resp["rooms"]["join"]["!general1:localhost"]["timeline"]["events"]
        .as_array()
        .expect("timeline.events must be an array");

    assert!(
        !timeline_events.is_empty(),
        "Expected seeded events in !general1:localhost timeline"
    );
}

#[tokio::test]
async fn test_sync_incremental_after_send() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let http = reqwest::Client::new();

    // Initial sync — capture prev_batch for !general1:localhost (room-local timeline length).
    // The test server uses room-local timeline index for slicing in incremental syncs,
    // so prev_batch (= timeline length at that point) is the correct `since` for this room.
    let initial: serde_json::Value = http
        .get(format!("{}/_matrix/client/v3/sync", srv.base_url))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("initial GET /sync")
        .json()
        .await
        .expect("parse initial sync");

    // prev_batch is the room-local timeline length at time of initial sync.
    let prev_batch = initial["rooms"]["join"]["!general1:localhost"]["timeline"]["prev_batch"]
        .as_str()
        .expect("prev_batch for !general1:localhost in initial sync")
        .to_string();

    // Send a new message to !general1:localhost
    let txn_id = "txn-incremental-test-1";
    http.put(format!(
        "{}/_matrix/client/v3/rooms/!general1:localhost/send/m.room.message/{txn_id}",
        srv.base_url
    ))
    .header("Authorization", format!("Bearer {token}"))
    .json(&serde_json::json!({ "msgtype": "m.text", "body": "test incremental sync" }))
    .send()
    .await
    .expect("PUT send message")
    .error_for_status()
    .expect("send message should succeed");

    // Incremental sync using prev_batch as `since` — the test server slices per-room
    // timelines from `since` as a room-local index, so this returns exactly the new event.
    let incremental_url = format!("{}/_matrix/client/v3/sync?since={prev_batch}", srv.base_url);
    let incremental: serde_json::Value = http
        .get(&incremental_url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("incremental GET /sync")
        .json()
        .await
        .expect("parse incremental sync");

    assert!(
        incremental["next_batch"].is_string(),
        "Expected next_batch in incremental sync"
    );

    // The new message must appear in the incremental timeline for !general1:localhost
    let inc_events = incremental["rooms"]["join"]["!general1:localhost"]["timeline"]["events"]
        .as_array()
        .expect("incremental timeline.events must be an array");

    let found = inc_events.iter().any(|ev| {
        ev["content"]["body"].as_str() == Some("test incremental sync")
    });
    assert!(
        found,
        "New message 'test incremental sync' not found in incremental sync events: {inc_events:?}"
    );
}

#[tokio::test]
async fn test_sync_dm_room_included() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let resp: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/_matrix/client/v3/sync", srv.base_url))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("GET /sync")
        .json()
        .await
        .expect("parse sync response");

    let join = resp["rooms"]["join"].as_object().expect("rooms.join must be an object");

    // owl is in !dm1:localhost (DM with axolotl)
    assert!(
        join.contains_key("!dm1:localhost"),
        "Expected !dm1:localhost in rooms.join, got keys: {:?}",
        join.keys().collect::<Vec<_>>()
    );

    // DM room must have seeded timeline events
    let dm_events = resp["rooms"]["join"]["!dm1:localhost"]["timeline"]["events"]
        .as_array()
        .expect("DM timeline.events must be an array");

    assert!(
        !dm_events.is_empty(),
        "Expected seeded messages in !dm1:localhost"
    );
}

#[tokio::test]
async fn test_sync_notification_counts() {
    let srv = TestServer::start().await;
    // Authenticate as axolotl
    let (_user_id, token) = get_test_token(&srv.base_url, "Axolotl").await;

    let response = reqwest::Client::new()
        .get(format!("{}/_matrix/client/v3/sync", srv.base_url))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("GET /sync");

    assert_eq!(
        response.status(),
        200,
        "sync should return 200 for axolotl"
    );

    let body: serde_json::Value = response.json().await.expect("parse sync response");

    // Must be parseable and have next_batch
    assert!(
        body["next_batch"].is_string(),
        "Expected next_batch in sync response for axolotl"
    );

    // rooms.join must be an object (may be empty or contain rooms axolotl is in)
    assert!(
        body["rooms"]["join"].is_object(),
        "rooms.join must be an object"
    );
}

#[tokio::test]
async fn test_matrix_client_backend_event_stream_returns_stream() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    // event_stream() must not panic — box it and drop it immediately
    let _stream = client.event_stream();
    drop(_stream);
}

#[tokio::test]
async fn test_event_stream_receives_sent_message() {
    let srv = TestServer::start().await;

    // Authenticate two clients: Axolotl listens, Owl sends.
    let (_axolotl_id, axolotl_token) = get_test_token(&srv.base_url, "Axolotl").await;
    let (_owl_id, owl_token) = get_test_token(&srv.base_url, "Owl").await;

    let mut axolotl = make_client(&srv.base_url);
    axolotl
        .authenticate(AuthCredentials::Token(axolotl_token))
        .await
        .expect("authenticate axolotl");

    let mut owl = make_client(&srv.base_url);
    owl.authenticate(AuthCredentials::Token(owl_token))
        .await
        .expect("authenticate owl");

    // Send a message as Owl to !general1:localhost before starting the stream,
    // so it will be part of the first (or a subsequent) sync response.
    let body = "event-stream integration test message";
    owl.send_message(
        "!general1:localhost",
        MessageContent::Text(body.to_string()),
    )
    .await
    .expect("owl send_message");

    // Obtain Axolotl's event stream after the message has been sent.
    // The initial sync (since=0) will include all timeline events — both seeded
    // and the one Owl just sent.  Scan every event for a matching MessageReceived.
    let mut stream = axolotl.event_stream();

    // Read from the stream with a 5-second timeout, skipping non-matching events
    // (seeded messages etc.) until a MessageReceived for !general1:localhost with
    // the right body arrives.
    let found = loop {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            stream.next(),
        )
        .await;

        match result {
            Err(_elapsed) => {
                // Timed out waiting for the event.
                break false;
            }
            Ok(None) => {
                // Stream ended.
                break false;
            }
            Ok(Some(ClientEvent::MessageReceived { channel_id, message })) => {
                if channel_id == "!general1:localhost"
                    && let MessageContent::Text(ref text) = message.content
                    && text == body
                {
                    break true;
                }
                // Different event (seeded or wrong channel) — keep scanning.
            }
            Ok(Some(_other)) => {
                // Different event type — keep scanning.
            }
        }
    };

    assert!(
        found,
        "Did not receive MessageReceived for !general1:localhost with body '{body}' within 2 s"
    );
}

// ---------------------------------------------------------------------------
// Pack C.2 — settings storage round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_settings_storage_round_trip() {
    
    let client = poly_matrix::MatrixClient::new();
    client
        .set_setting_value(SettingsScope::PerServer, "room1", "display-name", "matrix-nick")
        .await
        .expect("set_setting_value should succeed");
    let got = client
        .get_setting_value(SettingsScope::PerServer, "room1", "display-name")
        .await
        .expect("get_setting_value should succeed");
    assert_eq!(got, "matrix-nick");
}

#[tokio::test]
async fn test_get_account_overview_view_returns_card_grid() {
    use poly_client::{ViewBody, ViewKind};

    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let view = client
        .get_account_overview_view()
        .await
        .expect("get_account_overview_view");

    assert_eq!(view.kind, ViewKind::CardGrid, "overview must use CardGrid");
    assert!(
        matches!(view.body, ViewBody::CardBody(_)),
        "overview body must be CardBody, got: {:?}",
        view.body
    );
    let header = view.header.expect("overview must have a header");
    assert_eq!(
        header.title_key.as_deref(),
        Some("plugin-matrix-overview-title"),
        "title key must match FTL key"
    );
}

#[tokio::test]
async fn test_get_view_rows_account_overview_lists_rooms() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let page = client
        .get_view_rows("account-overview", None, None, None, None)
        .await
        .expect("get_view_rows account-overview");

    assert!(!page.rows.is_empty(), "must return at least one row");

    // Every row must have a non-empty primary_text (room name / alias).
    for row in &page.rows {
        assert!(!row.primary_text.is_empty(), "primary_text must not be empty: {row:?}");
    }

    // meta_text must contain "members" keyword.
    for row in &page.rows {
        let meta = row.meta_text.as_deref().unwrap_or("");
        assert!(meta.contains("members"), "meta_text must contain 'members': {meta}");
    }

    // At least one of the seeded spaces/rooms should appear by name.
    let names: Vec<&str> = page.rows.iter().map(|r| r.primary_text.as_str()).collect();
    assert!(
        names.iter().any(|n| *n == "The Hollow Tree" || *n == "Neon Reef" || *n == "general"),
        "expected a known room name in overview rows: {names:?}"
    );

    // No next cursor for a complete room list.
    assert!(page.next_cursor.is_none(), "overview must not paginate");
}

#[tokio::test]
async fn test_get_view_rows_non_overview_returns_not_supported() {
    let srv = TestServer::start().await;
    let (_user_id, token) = get_test_token(&srv.base_url, "Owl").await;

    let mut client = make_client(&srv.base_url);
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    let result = client
        .get_view_rows("!general1:localhost", None, None, None, None)
        .await;

    assert!(
        matches!(result, Err(poly_client::ClientError::NotSupported(_))),
        "non-overview channel_id must return NotSupported, got: {result:?}"
    );
}
