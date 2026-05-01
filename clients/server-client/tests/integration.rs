//! End-to-end integration tests for poly-server-client.
//!
//! Spins up a real poly-server instance and exercises the full client library:
//! signup, signin, server/channel CRUD, messaging, friend requests, DMs,
//! and WebSocket real-time events.
//!
//! Run with:
//! ```
//! cargo test -p poly-server-client
//! ```
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::middleware;
use rand::RngExt;
use tokio::net::TcpListener as TokioListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use poly_server::{AppState, Config, api, auth, db, ws};
use poly_server_client::http::{PolyServerConfig, PolyServerHttpClient};
use poly_server_client::ws::PolyServerWsClient;

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct TestServer {
    addr: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        // Initialize tracing (once).
        let _ = tracing_subscriber::fmt()
            .with_env_filter("poly_server=debug,poly_server_client=debug,warn")
            .with_test_writer()
            .try_init();

        // Find a free port.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);
        let addr = format!("127.0.0.1:{port}");

        // Temp dir for DB + uploads.
        let tmp = tempfile::tempdir().expect("tmpdir");
        let db_path = tmp.path().join("testdb").to_string_lossy().to_string();
        let uploads_dir = tmp.path().join("uploads").to_string_lossy().to_string();

        let config = Arc::new(Config {
            bind_addr: addr.clone(),
            db_path,
            surreal_url: "ws://localhost:8000".into(),
            surreal_user: "root".into(),
            surreal_pass: "root".into(),
            server_name: "Test Server".into(),
            invite_only: false,
            jwt_secret: "test-secret".into(),
            jwt_expiry_secs: 3600,
            uploads_dir,
        });

        let db_obj: Arc<db::Db> = Arc::new(db::init(&config).await.expect("db init"));
        let ws_state = Arc::new(ws::WsState::new());
        let state = AppState {
            db: db_obj,
            config,
            ws: ws_state,
        };

        let protected = api::router()
            .merge(auth::routes::protected_router())
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                auth::auth_middleware,
            ));

        let app: Router = Router::new()
            .merge(auth::routes::public_router())
            .merge(protected)
            .merge(ws::router())
            .layer(TraceLayer::new_for_http())
            .layer(CorsLayer::permissive())
            .with_state(state);

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let tcp = TokioListener::bind(&addr).await.expect("listen");

        tokio::spawn(async move {
            axum::serve(tcp, app)
                .with_graceful_shutdown(async {
                    rx.await.ok();
                })
                .await
                .expect("serve");
            // tmp dropped here → cleans test DB.
        });

        // Give the server a moment to bind.
        tokio::time::sleep(Duration::from_millis(50)).await;

        Self {
            addr,
            _shutdown: tx,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// Generate a fresh 32-byte Ed25519 seed.
fn random_key() -> [u8; 32] {
    let mut rng = rand::rng();
    rng.random()
}

/// Build a `PolyServerHttpClient` for the given server with a fresh keypair.
fn make_client(srv: &TestServer) -> PolyServerHttpClient {
    let config = PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: random_key(),
    };
    PolyServerHttpClient::new(&config)
}

/// Build a client with a specific key.
fn make_client_with_key(srv: &TestServer, key: [u8; 32]) -> PolyServerHttpClient {
    let config = PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: key,
    };
    PolyServerHttpClient::new(&config)
}

fn test_email(username: &str) -> String {
    format!("{username}@example.test")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_server_info() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);

    let info = client.server_info().await.expect("server_info");
    assert_eq!(info.name, "Test Server");
    assert!(!info.invite_only);
}

#[tokio::test]
async fn test_signup_and_signin() {
    let srv = TestServer::start().await;
    let key = random_key();
    let client = make_client_with_key(&srv, key);

    // Sign up.
    let auth1 = client
        .signup("alice", &test_email("alice"), Some("Alice"))
        .await
        .expect("signup");
    assert!(!auth1.token.is_empty());
    assert!(!auth1.user_id.is_empty());
    assert!(client.is_authenticated().await);

    // Fetch profile.
    let me = client.get_me().await.expect("get_me");
    assert_eq!(me.username, "alice");
    assert_eq!(me.display_name, "Alice");

    // Sign out.
    client.signout().await.expect("signout");
    assert!(!client.is_authenticated().await);

    // Sign back in with the same key via challenge-response.
    let auth2 = client.signin(None).await.expect("signin");
    assert!(!auth2.token.is_empty());
    assert_eq!(auth2.user_id, auth1.user_id);
    assert!(client.is_authenticated().await);
}

#[tokio::test]
async fn test_list_accounts_and_sign_in_selected_account() {
    let srv = TestServer::start().await;
    let key = random_key();

    let first = make_client_with_key(&srv, key);
    let auth1 = first
        .signup(
            "alice_multi",
            &test_email("alice_multi"),
            Some("Alice Multi"),
        )
        .await
        .expect("first signup");
    first.signout().await.expect("first signout");

    let second = make_client_with_key(&srv, key);
    let auth2 = second
        .signup(
            "alice_multi_two",
            &test_email("alice_multi_two"),
            Some("Alice Multi Two"),
        )
        .await
        .expect("second signup");
    second.signout().await.expect("second signout");

    let chooser = make_client_with_key(&srv, key);
    let accounts = chooser.list_accounts().await.expect("list accounts");
    assert_eq!(accounts.len(), 2);
    assert!(
        accounts
            .iter()
            .any(|account| account.user_id == auth1.user_id)
    );
    assert!(
        accounts
            .iter()
            .any(|account| account.user_id == auth2.user_id)
    );

    let selected = chooser
        .signin(Some(&auth2.user_id))
        .await
        .expect("signin selected");
    assert_eq!(selected.user_id, auth2.user_id);
}

#[tokio::test]
async fn test_create_server_and_channels() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("bob", &test_email("bob"), None)
        .await
        .expect("signup");

    // Create a server.
    let server = client
        .create_server("My Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("server should have id");
    assert_eq!(server.name, "My Guild");

    // List servers — should contain the one we created.
    let servers = client.get_servers().await.expect("get_servers");
    assert!(
        servers.iter().any(|s| s.name == "My Guild"),
        "Created server not found in get_servers result: {servers:?}",
    );

    // Create channels.
    let ch1 = client
        .create_channel(&server_id, "general", "text", None)
        .await
        .expect("create_channel text");
    assert_eq!(ch1.name, "general");
    assert!(!ch1.id.is_empty());

    let ch2 = client
        .create_channel(&server_id, "voice-lobby", "voice", None)
        .await
        .expect("create_channel voice");
    assert_eq!(ch2.name, "voice-lobby");

    // List channels.
    let channels = client.get_channels(&server_id).await.expect("get_channels");
    assert!(channels.len() >= 2);
    assert!(channels.iter().any(|c| c.name == "general"));
    assert!(channels.iter().any(|c| c.name == "voice-lobby"));
}

#[tokio::test]
async fn test_send_and_list_messages() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("charlie", &test_email("charlie"), None)
        .await
        .expect("signup");

    // Create server + channel.
    let server = client
        .create_server("Chat Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");
    let channel = client
        .create_channel(&server_id, "chat", "text", None)
        .await
        .expect("create_channel");

    // Send messages.
    let msg1 = client
        .send_message(&channel.id, "Hello, world!", None, None)
        .await
        .expect("send_message 1");
    assert_eq!(msg1.content, "Hello, world!");
    assert!(!msg1.id.is_empty());
    assert_eq!(msg1.channel_id, channel.id);

    let msg2 = client
        .send_message(&channel.id, "Second message", None, None)
        .await
        .expect("send_message 2");
    assert_eq!(msg2.content, "Second message");

    // List messages — server returns newest first (reverse chronological).
    let messages = client
        .get_messages(&channel.id, Some(50), None)
        .await
        .expect("get_messages");
    assert_eq!(messages.len(), 2);

    // Both should be present (order may be newest-first).
    let contents: Vec<&str> = messages.iter().map(|m| m.content.as_str()).collect();
    assert!(
        contents.contains(&"Hello, world!"),
        "Missing 'Hello, world!' in {contents:?}",
    );
    assert!(
        contents.contains(&"Second message"),
        "Missing 'Second message' in {contents:?}",
    );
}

#[tokio::test]
async fn test_edit_and_delete_message() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("dave", &test_email("dave"), None)
        .await
        .expect("signup");

    let server = client
        .create_server("Edit Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");
    let channel = client
        .create_channel(&server_id, "edits", "text", None)
        .await
        .expect("create_channel");

    // Send a message.
    let msg = client
        .send_message(&channel.id, "original", None, None)
        .await
        .expect("send_message");
    assert!(msg.edited_at.is_none());

    // Edit it.
    let edited = client
        .edit_message(&msg.id, "edited content")
        .await
        .expect("edit_message");
    assert_eq!(edited.content, "edited content");
    assert!(edited.edited_at.is_some());

    // Soft-delete it.
    client
        .delete_message(&msg.id)
        .await
        .expect("delete_message");

    // Verify it's marked as deleted.
    let messages = client
        .get_messages(&channel.id, Some(50), None)
        .await
        .expect("get_messages");
    let deleted_msg = messages.iter().find(|m| m.id == msg.id).expect("find msg");
    assert!(deleted_msg.deleted);
}

#[tokio::test]
async fn test_friend_request_and_dm() {
    let srv = TestServer::start().await;

    // Sign up two users.
    let key_alice = random_key();
    let alice = make_client_with_key(&srv, key_alice);
    let _alice_auth = alice
        .signup("alice_fr", &test_email("alice_fr"), Some("Alice"))
        .await
        .expect("signup alice");

    let key_bob = random_key();
    let bob = make_client_with_key(&srv, key_bob);
    let bob_auth = bob
        .signup("bob_fr", &test_email("bob_fr"), Some("Bob"))
        .await
        .expect("signup bob");

    // Alice sends friend request to Bob (by username).
    let fr = alice
        .send_friend_request("bob_fr")
        .await
        .expect("send_friend_request");
    assert_eq!(
        fr.status,
        poly_server_client::models::FriendRequestStatus::Pending
    );
    let fr_id = fr.id.expect("friend request should have id");

    // Bob accepts using the friend request ID from Alice's response.
    bob.respond_friend_request(&fr_id, "accepted")
        .await
        .expect("accept friend request");

    // Verify friendship: Bob's friends list should now contain Alice.
    let bob_friends = bob.get_friends().await.expect("get_friends");
    assert!(
        !bob_friends.is_empty(),
        "Bob should have at least one friend after accepting",
    );
    assert!(
        bob_friends.iter().any(|f| f.username == "alice_fr"),
        "Alice should be in Bob's friends list: {bob_friends:?}",
    );

    // Alice creates a DM with Bob.
    let dm = alice.create_dm(&bob_auth.user_id).await.expect("create_dm");
    assert!(!dm.id.is_empty());

    // Alice sends a DM message.
    let dm_msg = alice
        .send_message(&dm.id, "Hey Bob!", None, None)
        .await
        .expect("send DM message");
    assert_eq!(dm_msg.content, "Hey Bob!");

    // Bob reads the DM.
    let bob_dms = bob.get_dm_channels().await.expect("get_dm_channels");
    assert!(
        !bob_dms.is_empty(),
        "Bob should see at least one DM channel"
    );

    let bob_dm_id = &bob_dms.first().unwrap().id;
    let bob_msgs = bob
        .get_messages(bob_dm_id, Some(50), None)
        .await
        .expect("get DM messages");
    assert_eq!(bob_msgs.len(), 1);
    assert_eq!(bob_msgs.first().unwrap().content, "Hey Bob!");
}

#[tokio::test]
async fn test_invite_and_join_server() {
    let srv = TestServer::start().await;

    // Alice creates a server.
    let alice = make_client(&srv);
    alice
        .signup("alice_inv", &test_email("alice_inv"), None)
        .await
        .expect("signup");
    let server = alice
        .create_server("Invite Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");

    // Alice creates an invite.
    let invite = alice
        .create_invite(&server_id, None, None)
        .await
        .expect("create_invite");
    assert!(!invite.code.is_empty());

    // Bob joins via invite.
    let bob = make_client(&srv);
    bob.signup("bob_inv", &test_email("bob_inv"), None)
        .await
        .expect("signup");
    bob.join_server(&invite.code).await.expect("join_server");

    // Bob should now see the server in his list.
    let bob_servers = bob.get_servers().await.expect("get_servers");
    assert!(
        bob_servers.iter().any(|s| s.name == "Invite Guild"),
        "Bob should see the server after joining: {bob_servers:?}",
    );
}

#[tokio::test]
async fn test_websocket_events() {
    let srv = TestServer::start().await;

    // Sign up and create a server + channel.
    let key = random_key();
    let client = make_client_with_key(&srv, key);
    let _auth = client
        .signup("eve_ws", &test_email("eve_ws"), None)
        .await
        .expect("signup");

    let server = client
        .create_server("WS Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");
    let channel = client
        .create_channel(&server_id, "events", "text", None)
        .await
        .expect("create_channel");

    // Set up WS client.
    let mut ws_client = PolyServerWsClient::new(&srv.base_url(), client.session_lock());
    let mut rx = ws_client.subscribe();
    ws_client.connect();

    // Give WS time to connect.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Send a message — should trigger a WS event.
    let sent = client
        .send_message(&channel.id, "WS test message", None, None)
        .await
        .expect("send_message");

    // Wait for the MessageCreated event (skip Pings and other events).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let event = tokio::time::timeout(remaining, rx.recv())
            .await
            .expect("Timeout waiting for WS event")
            .expect("recv error");

        match event {
            poly_server_client::models::ServerEvent::MessageCreated(payload) => {
                assert_eq!(payload.content, "WS test message");
                assert_eq!(payload.channel_id, channel.id);
                assert_eq!(payload.id, sent.id);
                break; // Success!
            }
            poly_server_client::models::ServerEvent::Ping => {
                continue; // Skip keepalive pings.
            }
            other => unreachable!("Unexpected event before MessageCreated: {:?}", other),
        }
    }

    ws_client.disconnect();
}

#[tokio::test]
async fn test_reactions() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("frank_rx", &test_email("frank_rx"), None)
        .await
        .expect("signup");

    let server = client
        .create_server("React Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");
    let channel = client
        .create_channel(&server_id, "reactions", "text", None)
        .await
        .expect("create_channel");

    let msg = client
        .send_message(&channel.id, "React to this!", None, None)
        .await
        .expect("send_message");

    // Add a reaction.
    client
        .add_reaction(&msg.id, "👍")
        .await
        .expect("add_reaction");

    // List reactions.
    let reactions = client.get_reactions(&msg.id).await.expect("get_reactions");
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions.first().unwrap().emoji, "👍");

    // Remove the reaction.
    client
        .remove_reaction(&msg.id, "👍")
        .await
        .expect("remove_reaction");

    let reactions = client.get_reactions(&msg.id).await.expect("get_reactions");
    assert!(reactions.is_empty());
}

#[tokio::test]
async fn test_server_detail() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("gina_detail", &test_email("gina_detail"), None)
        .await
        .expect("signup");

    let server = client
        .create_server("Detail Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");

    // Create a channel.
    client
        .create_channel(&server_id, "lobby", "text", None)
        .await
        .expect("create_channel");

    // Get full server detail.
    let detail = client.get_server(&server_id).await.expect("get_server");
    assert_eq!(detail.server.name, "Detail Guild");
    assert!(!detail.members.is_empty()); // At least the creator (raw JSON).
    assert!(!detail.channels.is_empty()); // The "lobby" channel.
}

#[tokio::test]
async fn test_devices() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("henry_dev", &test_email("henry_dev"), None)
        .await
        .expect("signup");

    let devices = client.get_devices().await.expect("get_devices");
    // At least one device (the current session).
    assert!(!devices.is_empty());
}

// ── New tests for Phase 2 additions ─────────────────────────────────────────

#[tokio::test]
async fn test_reply_message() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("irene_reply", &test_email("irene_reply"), None)
        .await
        .expect("signup");

    let server = client
        .create_server("Reply Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");
    let channel = client
        .create_channel(&server_id, "replies", "text", None)
        .await
        .expect("create_channel");

    // Send the original message.
    let original = client
        .send_message(&channel.id, "This is the original", None, None)
        .await
        .expect("send original");

    // Send a reply referencing the original.
    let reply = client
        .send_message(&channel.id, "This is a reply", Some(&original.id), None)
        .await
        .expect("send reply");
    assert_eq!(reply.content, "This is a reply");
    // The reply should record reply_to_id pointing to the original.
    assert_eq!(
        reply.reply_to_id.as_deref(),
        Some(original.id.as_str()),
        "reply_to_id should be set to the original message ID"
    );
}

#[tokio::test]
async fn test_update_and_delete_server() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("jake_server", &test_email("jake_server"), None)
        .await
        .expect("signup");

    // Create a server.
    let server = client
        .create_server("Old Name")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");

    // Update it.
    let updated = client
        .update_server(&server_id, Some("New Name"), None)
        .await
        .expect("update_server");
    assert_eq!(updated.name, "New Name");

    // Delete it.
    client
        .delete_server(&server_id)
        .await
        .expect("delete_server");

    // The server should no longer appear in the list.
    let servers = client.get_servers().await.expect("get_servers");
    assert!(
        !servers.iter().any(|s| s.name == "New Name"),
        "Deleted server should not appear in get_servers"
    );
}

#[tokio::test]
async fn test_kick_member() {
    let srv = TestServer::start().await;

    // Alice creates a server.
    let alice = make_client(&srv);
    alice
        .signup("alice_kick", &test_email("alice_kick"), None)
        .await
        .expect("signup");
    let server = alice
        .create_server("Kick Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");

    // Alice creates an invite.
    let invite = alice
        .create_invite(&server_id, None, None)
        .await
        .expect("create_invite");

    // Bob joins.
    let bob = make_client(&srv);
    let bob_auth = bob
        .signup("bob_kick", &test_email("bob_kick"), None)
        .await
        .expect("signup bob");
    bob.join_server(&invite.code).await.expect("join_server");

    // Bob should be a member.
    let bob_servers = bob.get_servers().await.expect("get_servers");
    assert!(
        bob_servers.iter().any(|s| s.name == "Kick Guild"),
        "Bob should be in the server before kick"
    );

    // Alice kicks Bob.
    alice
        .kick_member(&server_id, &bob_auth.user_id)
        .await
        .expect("kick_member");

    // Bob's server list should no longer contain the guild.
    let bob_servers_after = bob.get_servers().await.expect("get_servers after kick");
    assert!(
        !bob_servers_after.iter().any(|s| s.name == "Kick Guild"),
        "Bob should not see the server after being kicked"
    );
}

#[tokio::test]
async fn test_update_and_delete_channel() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("kate_ch", &test_email("kate_ch"), None)
        .await
        .expect("signup");

    let server = client
        .create_server("Channel Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");
    let ch = client
        .create_channel(&server_id, "old-name", "text", None)
        .await
        .expect("create_channel");

    // Update channel name.
    let updated = client
        .update_channel(&ch.id, Some("new-name"), None, None)
        .await
        .expect("update_channel");
    assert_eq!(updated.name, "new-name");

    // Delete channel.
    client.delete_channel(&ch.id).await.expect("delete_channel");

    // Channel should no longer appear.
    let channels = client.get_channels(&server_id).await.expect("get_channels");
    assert!(
        !channels.iter().any(|c| c.name == "new-name"),
        "Deleted channel should not appear in list"
    );
}

#[tokio::test]
async fn test_category_crud() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);
    client
        .signup("leo_cat", &test_email("leo_cat"), None)
        .await
        .expect("signup");

    let server = client
        .create_server("Category Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("id");

    // Create a category.
    let cat = client
        .create_category(&server_id, "My Category", Some(0))
        .await
        .expect("create_category");
    assert_eq!(cat.name, "My Category");
    let cat_id = cat.id;

    // Create a channel inside the category.
    let ch = client
        .create_channel(&server_id, "cat-channel", "text", Some(&cat_id))
        .await
        .expect("create_channel in category");
    assert_eq!(ch.name, "cat-channel");
    // Note: the server's channel-creation response doesn't always return category_id
    // in the same response. Verify via server detail instead.
    let detail = client
        .get_server(&server_id)
        .await
        .expect("get_server for category");
    let found = detail
        .channels
        .iter()
        .any(|c| c.name == "cat-channel" && c.category_id.as_deref() == Some(cat_id.as_str()));
    assert!(
        found,
        "cat-channel should be in category {cat_id}: {detail:?}"
    );

    // Update category name.
    let updated_cat = client
        .update_category(&cat_id, Some("Renamed Category"), None)
        .await
        .expect("update_category");
    assert_eq!(updated_cat.name, "Renamed Category");

    // Delete category.
    client
        .delete_category(&cat_id)
        .await
        .expect("delete_category");
}

#[tokio::test]
async fn test_remove_friend() {
    let srv = TestServer::start().await;

    let alice = make_client(&srv);
    alice
        .signup("alice_unfriend", &test_email("alice_unfriend"), None)
        .await
        .expect("signup");

    let bob = make_client(&srv);
    let bob_auth = bob
        .signup("bob_unfriend", &test_email("bob_unfriend"), None)
        .await
        .expect("signup bob");

    // Alice sends a friend request.
    let fr = alice
        .send_friend_request("bob_unfriend")
        .await
        .expect("send_friend_request");
    let fr_id = fr.id.expect("friend request id");

    // Bob accepts.
    bob.respond_friend_request(&fr_id, "accepted")
        .await
        .expect("accept");

    // Verify friendship is active.
    let alice_friends_before = alice.get_friends().await.expect("get_friends before");
    assert!(
        !alice_friends_before.is_empty(),
        "Alice should have Bob as friend"
    );

    // Alice removes the friendship — the endpoint takes the OTHER user's ID, not the request ID.
    alice
        .remove_friend(&bob_auth.user_id)
        .await
        .expect("remove_friend");

    // Alice's friends list should be empty.
    let alice_friends = alice.get_friends().await.expect("get_friends");
    assert!(
        alice_friends.is_empty(),
        "Alice should have no friends after removal"
    );
}

// ---------------------------------------------------------------------------
// Pack C.2 — settings storage round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_settings_storage_round_trip() {
    use poly_client::{ClientBackend, SettingsScope};
    let client = poly_server_client::PolyServerBackend::new("http://localhost:9333", [0u8; 32]);
    client
        .set_setting_value(SettingsScope::PerServer, "srv1", "nickname", "poly-nick")
        .await
        .expect("set_setting_value should succeed");
    let got = client
        .get_setting_value(SettingsScope::PerServer, "srv1", "nickname")
        .await
        .expect("get_setting_value should succeed");
    assert_eq!(got, "poly-nick");
}
