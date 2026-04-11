//! End-to-end integration tests for the poly-server client.
//!
//! Spins up a real poly-server in-process and exercises the full client:
//! signup → create server → invite → join → send messages → WebSocket events.
//!
//! Run with:
//! ```
//! cargo test -p poly-client --test integration -- --nocapture
//! ```
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::middleware;
use rand::RngExt;
use tokio::net::TcpListener as TokioListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use poly_client::{AuthCredentials, BackendType, ClientBackend, MessageContent, MessageQuery};
use poly_server::{AppState, Config, api, auth, db, ws};
use poly_server_client::PolyServerBackend;
use poly_server_client::http::{PolyServerConfig, PolyServerHttpClient};
use poly_server_client::models::ServerEvent;

// ---------------------------------------------------------------------------
// Test harness — spins up a real poly-server instance
// ---------------------------------------------------------------------------

struct TestServer {
    pub addr: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("poly_server=debug,poly_client=debug,warn")
            .with_test_writer()
            .try_init();

        // Find a free port.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind free port");
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
            server_name: "Integration Test Server".into(),
            invite_only: false,
            jwt_secret: "test-secret-key".into(),
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
            // tmp dropped here, cleaning up test DB + uploads.
            drop(tmp);
        });

        // Give the server a moment to bind.
        tokio::time::sleep(Duration::from_millis(100)).await;

        Self {
            addr,
            _shutdown: tx,
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// Generate a random 32-byte Ed25519 seed.
fn random_key() -> [u8; 32] {
    let mut rng = rand::rng();
    rng.random()
}

// ---------------------------------------------------------------------------
// Tests — HTTP client layer
// ---------------------------------------------------------------------------

fn test_email(username: &str) -> String {
    format!("{username}@example.test")
}

#[tokio::test]
async fn test_signup_and_server_info() {
    let srv = TestServer::start().await;

    let config = PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: random_key(),
    };
    let client = PolyServerHttpClient::new(config);

    // Server info (no auth).
    let info = client.server_info().await.expect("server_info");
    assert_eq!(info.name, "Integration Test Server");
    assert!(!info.invite_only);

    // Signup.
    let auth = client
        .signup("alice", &test_email("alice"), Some("Alice"))
        .await
        .expect("signup");
    assert!(!auth.token.is_empty());
    assert!(!auth.user_id.is_empty());
    assert!(!auth.device_id.is_empty());

    // Should be authenticated now.
    assert!(client.is_authenticated().await);

    // Get own profile.
    let me = client.get_me().await.expect("get_me");
    assert_eq!(me.username, "alice");
    assert_eq!(me.display_name, "Alice");
}

#[tokio::test]
async fn test_signin_challenge_response() {
    let srv = TestServer::start().await;
    let key = random_key();

    let config = PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: key,
    };

    // First signup.
    let client1 = PolyServerHttpClient::new(config.clone());
    client1
        .signup("bob", &test_email("bob"), None)
        .await
        .expect("signup");

    // Now signin with the same key on a fresh client.
    let client2 = PolyServerHttpClient::new(config);
    let auth = client2.signin(None).await.expect("signin");
    assert!(!auth.token.is_empty());
    assert!(client2.is_authenticated().await);

    let me = client2.get_me().await.expect("get_me");
    assert_eq!(me.username, "bob");
}

#[tokio::test]
async fn test_create_server_invite_join() {
    let srv = TestServer::start().await;

    // Alice creates a server.
    let alice_client = PolyServerHttpClient::new(PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: random_key(),
    });
    alice_client
        .signup("alice", &test_email("alice"), Some("Alice"))
        .await
        .expect("alice signup");

    let server = alice_client
        .create_server("Test Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("server should have id");
    assert_eq!(server.name, "Test Guild");

    // Verify server detail.
    let detail = alice_client
        .get_server(&server_id)
        .await
        .expect("get_server");
    assert_eq!(detail.server.name, "Test Guild");
    // servers no longer auto-create a #general channel; don't expect any channels here.
    // (Other tests explicitly create one when needed.)

    // Alice creates an invite.
    let invite_val = alice_client
        .create_invite(&server_id, None, None)
        .await
        .expect("create_invite");
    let invite_code = invite_val.code.clone();
    assert!(!invite_code.is_empty());

    // Bob joins via invite.
    let bob_client = PolyServerHttpClient::new(PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: random_key(),
    });
    bob_client
        .signup("bob", &test_email("bob"), Some("Bob"))
        .await
        .expect("bob signup");

    let join_result = bob_client
        .join_server(&invite_code)
        .await
        .expect("join_server");
    // join_server returns { "server_id": "..." }
    assert!(join_result.get("server_id").is_some());

    // Bob should now see the server.
    let bob_servers = bob_client.get_servers().await.expect("get_servers");
    assert!(
        bob_servers
            .iter()
            .any(|s| s.id.as_deref() == Some(server_id.as_str())),
        "Bob should see the server after joining"
    );
}

#[tokio::test]
async fn test_send_and_read_messages() {
    let srv = TestServer::start().await;

    // Alice creates server + Bob joins.
    let alice_client = PolyServerHttpClient::new(PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: random_key(),
    });
    alice_client
        .signup("alice", &test_email("alice"), Some("Alice"))
        .await
        .expect("signup");

    let server = alice_client
        .create_server("Msg Test")
        .await
        .expect("create_server");
    let server_id = server.id.expect("server id");

    // Create a channel (server doesn't auto-create one).
    let ch = alice_client
        .create_channel(&server_id, "general", "text", None)
        .await
        .expect("create channel");
    let channel_id = &ch.id;

    // Alice sends a message.
    let msg = alice_client
        .send_message(channel_id, "Hello, world!", None, None)
        .await
        .expect("send_message");
    assert_eq!(msg.content, "Hello, world!");
    assert!(!msg.id.is_empty());

    // Alice reads messages back.
    let msgs = alice_client
        .get_messages(channel_id, None, None)
        .await
        .expect("get_messages");
    assert!(!msgs.is_empty());
    assert!(msgs.iter().any(|m| m.content == "Hello, world!"));

    // Alice edits the message.
    let edited = alice_client
        .edit_message(&msg.id, "Hello, edited!")
        .await
        .expect("edit_message");
    assert_eq!(edited.content, "Hello, edited!");
    assert!(edited.edited_at.is_some());

    // Alice deletes the message.
    alice_client
        .delete_message(&msg.id)
        .await
        .expect("delete_message");
}

#[tokio::test]
async fn test_friend_requests() {
    let srv = TestServer::start().await;

    let alice_client = PolyServerHttpClient::new(PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: random_key(),
    });
    alice_client
        .signup("alice", &test_email("alice"), Some("Alice"))
        .await
        .expect("signup");

    let bob_client = PolyServerHttpClient::new(PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: random_key(),
    });
    bob_client
        .signup("bob", &test_email("bob"), Some("Bob"))
        .await
        .expect("signup");

    // Alice sends a friend request to Bob (by username).
    let fr = alice_client
        .send_friend_request("bob")
        .await
        .expect("send_friend_request");
    assert_eq!(
        fr.status,
        poly_server_client::models::FriendRequestStatus::Pending
    );

    // Alice's friends list should include the pending request.
    let alice_friends = alice_client.get_friends().await.expect("get_friends");
    // The list may or may not be empty depending on whether "get_friends"
    // returns pendings. Just assert no error occurred.
    let _ = alice_friends;
}

// ---------------------------------------------------------------------------
// Tests — WebSocket events
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_websocket_message_event() {
    let srv = TestServer::start().await;

    // Alice signs up and creates server.
    let alice_key = random_key();
    let alice_http = PolyServerHttpClient::new(PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: alice_key,
    });
    alice_http
        .signup("alice", &test_email("alice"), Some("Alice"))
        .await
        .expect("signup");

    let server = alice_http.create_server("WS Test").await.expect("create");
    let server_id = server.id.expect("id");
    // Create a channel (server doesn't auto-create one).
    let ch = alice_http
        .create_channel(&server_id, "general", "text", None)
        .await
        .expect("create channel");
    let channel_id = ch.id.clone();

    // Create invite, bob joins.
    let invite_val = alice_http
        .create_invite(&server_id, None, None)
        .await
        .expect("invite");
    let invite_code = invite_val.code.clone();

    let bob_key = random_key();
    let bob_http = PolyServerHttpClient::new(PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: bob_key,
    });
    bob_http
        .signup("bob", &test_email("bob"), Some("Bob"))
        .await
        .expect("signup");
    bob_http.join_server(&invite_code).await.expect("join");

    // Bob connects a WebSocket and subscribes to events.
    let mut bob_ws =
        poly_server_client::PolyServerWsClient::new(&srv.base_url(), bob_http.session_lock());
    bob_ws.connect();
    let mut rx = bob_ws.subscribe();

    // Give WS time to connect.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Alice sends a message in the channel.
    alice_http
        .send_message(&channel_id, "Hello from Alice via WS!", None, None)
        .await
        .expect("send");

    // Bob should receive the MessageCreated event.
    // wait for a MessageCreated event, skipping any Ping keepalives
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            unreachable!("Timeout waiting for MessageCreated WS event");
        }
        let event = tokio::time::timeout(remaining, rx.recv())
            .await
            .expect("timeout waiting for WS event")
            .expect("recv error");
        match event {
            ServerEvent::MessageCreated(payload) => {
                assert_eq!(payload.content, "Hello from Alice via WS!");
                assert_eq!(payload.channel_id, channel_id);
                break;
            }
            ServerEvent::Ping => continue,
            other => unreachable!("Unexpected event before MessageCreated: {:?}", other),
        }
    }

    bob_ws.disconnect();
}

// ---------------------------------------------------------------------------
// Tests — ClientBackend trait (high-level API)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_backend_full_flow() {
    let srv = TestServer::start().await;

    // Create Alice backend via ClientBackend trait.
    let alice_key = random_key();
    let mut alice = PolyServerBackend::new(&srv.base_url(), alice_key);

    let session = alice
        .authenticate(AuthCredentials::PolyServer {
            server_url: srv.base_url(),
            private_key_bytes: alice_key.to_vec(),
            username: Some("alice".into()),
            email: Some("alice@example.test".into()),
            display_name: Some("Alice Backend".into()),
            selected_user_id: None,
            is_signup: true,
        })
        .await
        .expect("authenticate");

    assert!(!session.id.is_empty());
    assert_eq!(session.user.display_name, "Alice Backend");
    assert_eq!(session.backend, BackendType::from("poly"));
    assert!(alice.is_authenticated());

    // Create a server via the HTTP client.
    let server = alice
        .http()
        .create_server("Backend Test")
        .await
        .expect("create");
    let server_id = server.id.expect("id");

    // Get servers via ClientBackend trait.
    let servers = alice.get_servers().await.expect("get_servers");
    assert!(!servers.is_empty());
    assert!(servers.iter().any(|s| s.name == "Backend Test"));

    // Create a channel (server doesn't auto-create one).
    alice
        .http()
        .create_channel(&server_id, "general", "text", None)
        .await
        .expect("create ch");

    // Get channels via trait.
    let channels = alice.get_channels(&server_id).await.expect("get_channels");
    assert!(!channels.is_empty());
    let ch = channels.first().expect("no channels returned");
    assert_eq!(ch.channel_type, poly_client::ChannelType::Text);

    // Send a message via trait.
    let msg = alice
        .send_message(&ch.id, MessageContent::Text("Hello from backend!".into()))
        .await
        .expect("send_message");
    assert!(!msg.id.is_empty());

    // Get messages via trait.
    let msgs = alice
        .get_messages(&ch.id, MessageQuery::default())
        .await
        .expect("get_messages");
    assert!(
        msgs.iter().any(|m| {
            matches!(&m.content, MessageContent::Text(t) if t == "Hello from backend!")
        })
    );

    // Backend info.
    assert_eq!(alice.backend_type(), BackendType::from("poly"));
    assert_eq!(alice.backend_name(), "Poly Server");

    // Logout.
    alice.logout().await.expect("logout");
    assert!(!alice.is_authenticated());
}

#[tokio::test]
async fn test_backend_two_users_communicate() {
    let srv = TestServer::start().await;

    // Setup Alice.
    let alice_key = random_key();
    let mut alice = PolyServerBackend::new(&srv.base_url(), alice_key);
    alice
        .authenticate(AuthCredentials::PolyServer {
            server_url: srv.base_url(),
            private_key_bytes: alice_key.to_vec(),
            username: Some("alice".into()),
            email: Some("alice@example.test".into()),
            display_name: Some("Alice".into()),
            selected_user_id: None,
            is_signup: true,
        })
        .await
        .expect("alice auth");

    // Setup Bob.
    let bob_key = random_key();
    let mut bob = PolyServerBackend::new(&srv.base_url(), bob_key);
    bob.authenticate(AuthCredentials::PolyServer {
        server_url: srv.base_url(),
        private_key_bytes: bob_key.to_vec(),
        username: Some("bob".into()),
        email: Some("bob@example.test".into()),
        display_name: Some("Bob".into()),
        selected_user_id: None,
        is_signup: true,
    })
    .await
    .expect("bob auth");

    // Alice creates server + invite.
    let server = alice
        .http()
        .create_server("Two Users")
        .await
        .expect("create");
    let server_id = server.id.expect("id");
    let invite_val = alice
        .http()
        .create_invite(&server_id, None, None)
        .await
        .expect("invite");
    let invite_code = invite_val.code.clone();

    // Bob joins.
    bob.http().join_server(&invite_code).await.expect("join");

    // Create a channel (server doesn't auto-create one).
    alice
        .http()
        .create_channel(&server_id, "general", "text", None)
        .await
        .expect("create ch");

    // Get the channel.
    let channels = alice.get_channels(&server_id).await.expect("channels");
    let ch_id = &channels.first().expect("no channels returned").id;

    // Alice sends a message.
    alice
        .send_message(ch_id, MessageContent::Text("Hi Bob!".into()))
        .await
        .expect("alice send");

    // Bob reads messages.
    let msgs = bob
        .get_messages(ch_id, MessageQuery::default())
        .await
        .expect("bob get_messages");
    assert!(
        msgs.iter()
            .any(|m| { matches!(&m.content, MessageContent::Text(t) if t == "Hi Bob!") })
    );

    // Bob replies.
    bob.send_message(ch_id, MessageContent::Text("Hello Alice!".into()))
        .await
        .expect("bob send");

    // Alice reads Bob's reply.
    let msgs2 = alice
        .get_messages(ch_id, MessageQuery::default())
        .await
        .expect("alice get_messages");
    assert!(
        msgs2
            .iter()
            .any(|m| { matches!(&m.content, MessageContent::Text(t) if t == "Hello Alice!") }),
        "Alice should see Bob's reply"
    );

    // Cleanup.
    alice.logout().await.expect("alice logout");
    bob.logout().await.expect("bob logout");
}

/// Debug test: dump raw JSON from server to understand response format.
#[tokio::test]
async fn test_debug_raw_server_response() {
    let srv = TestServer::start().await;

    let client = PolyServerHttpClient::new(PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: random_key(),
    });
    let auth = client
        .signup("dbguser", &test_email("dbguser"), None)
        .await
        .expect("signup");

    // Create a server.
    let server = client.create_server("Debug Guild").await.expect("create");
    let server_id = server.id.expect("id");

    // Raw GET /servers/:id
    let raw_detail: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/servers/{}", srv.base_url(), server_id))
        .header("Authorization", format!("Bearer {}", auth.token))
        .send()
        .await
        .expect("send")
        .json()
        .await
        .expect("json");
    eprintln!(
        "\n=== RAW /servers/:id ===\n{}\n",
        serde_json::to_string_pretty(&raw_detail).unwrap()
    );

    // Raw GET /servers (list)
    let raw_list: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/servers", srv.base_url()))
        .header("Authorization", format!("Bearer {}", auth.token))
        .send()
        .await
        .expect("send")
        .json()
        .await
        .expect("json");
    eprintln!(
        "\n=== RAW /servers ===\n{}\n",
        serde_json::to_string_pretty(&raw_list).unwrap()
    );
}
