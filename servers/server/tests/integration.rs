//! End-to-end integration tests for poly-server.
//!
//! These tests spin up a real server instance on a random free port,
//! run through the full protocol, then tear down the server.
//!
//! Run with:
//! ```
//! cargo test -p poly-server
//! ```
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::middleware;
use ed25519_dalek::{Signer, SigningKey};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener as TokioListener;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use poly_server::{AppState, Config, api, auth, db, ws};

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct TestServer {
    pub addr: String,
    pub client: Client,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        // Initialize tracing for debug output in tests.
        let _ = tracing_subscriber::fmt()
            .with_env_filter("poly_server=debug,warn")
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
            // tmp dropped here, cleaning up test DB.
        });

        // Give the server a moment to bind.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("client");

        Self {
            addr,
            client,
            _shutdown: tx,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a fresh Ed25519 keypair and return (signing_key, hex_public_key).
fn gen_keypair() -> (SigningKey, String) {
    // Generate 32 random bytes and use them as Ed25519 seed.
    use rand::RngExt;
    let mut rng = rand::rng();
    let seed: [u8; 32] = rng.random();
    let sk = SigningKey::from_bytes(&seed);
    let pk_hex = hex::encode(sk.verifying_key().to_bytes());
    (sk, pk_hex)
}

/// Sign up with a fresh keypair.
async fn signup(srv: &TestServer, username: &str) -> (SigningKey, Value) {
    let (sk, pk_hex) = gen_keypair();
    let email = format!("{username}@example.test");
    let resp = srv
        .client
        .post(srv.url("/auth/signup"))
        .json(&json!({
            "public_key": pk_hex,
            "username": username,
            "email": email,
            "display_name": username
        }))
        .send()
        .await
        .expect("signup request");
    let status = resp.status();
    let body: Value = resp.json().await.expect("signup json");
    assert!(status.is_success(), "signup failed: {status} — {body}");
    (sk, body)
}

/// Sign up another account reusing an existing keypair.
async fn signup_with_key(srv: &TestServer, sk: &SigningKey, username: &str) -> Value {
    let pk_hex = hex::encode(sk.verifying_key().to_bytes());
    let email = format!("{username}@example.test");
    let resp = srv
        .client
        .post(srv.url("/auth/signup"))
        .json(&json!({
            "public_key": pk_hex,
            "username": username,
            "email": email,
            "display_name": username
        }))
        .send()
        .await
        .expect("signup request");
    let status = resp.status();
    let body: Value = resp.json().await.expect("signup json");
    assert!(status.is_success(), "signup failed: {status} — {body}");
    body
}

/// Challenge-response signin. Returns the JWT token.
async fn signin(srv: &TestServer, sk: &SigningKey, user_id: Option<&str>) -> String {
    let pk_hex = hex::encode(sk.verifying_key().to_bytes());

    // Step 1: Request challenge.
    let challenge_resp: Value = srv
        .client
        .post(srv.url("/auth/challenge"))
        .json(&json!({ "public_key": pk_hex, "user_id": user_id }))
        .send()
        .await
        .expect("challenge request")
        .json()
        .await
        .expect("challenge json");
    let challenge_hex = challenge_resp["challenge"]
        .as_str()
        .expect("challenge field");

    // Step 2: Sign the challenge.
    let challenge_bytes = hex::decode(challenge_hex).expect("decode challenge");
    let signature = sk.sign(&challenge_bytes);
    let sig_hex = hex::encode(signature.to_bytes());

    // Step 3: Verify.
    let verify_resp: Value = srv
        .client
        .post(srv.url("/auth/verify"))
        .json(&json!({
            "public_key": pk_hex,
            "user_id": user_id,
            "challenge": challenge_hex,
            "signature": sig_hex,
        }))
        .send()
        .await
        .expect("verify request")
        .json()
        .await
        .expect("verify json");
    verify_resp["token"]
        .as_str()
        .expect("token field")
        .to_owned()
}

fn auth_header(token: &str) -> String {
    format!("Bearer {token}")
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_server_info() {
    let srv = TestServer::start().await;
    let body: Value = srv
        .client
        .get(srv.url("/server-info"))
        .send()
        .await
        .expect("request")
        .json()
        .await
        .expect("json");
    assert_eq!(body["name"], "Test Server");
    assert_eq!(body["invite_only"], false);
}

#[tokio::test]
async fn test_auth_flow() {
    let srv = TestServer::start().await;

    // Signup.
    let (sk, signup_resp) = signup(&srv, "alice").await;
    assert!(signup_resp["token"].is_string(), "signup returns token");

    // Duplicate username should fail.
    let (_, dup_pk) = gen_keypair();
    let dup = srv
        .client
        .post(srv.url("/auth/signup"))
        .json(&json!({
            "public_key": dup_pk,
            "username": "alice",
            "email": "alice-dup@example.test",
            "display_name": "a"
        }))
        .send()
        .await
        .expect("dup req");
    assert_eq!(dup.status().as_u16(), 409);

    let (_, dup_email_pk) = gen_keypair();
    let dup_email = srv
        .client
        .post(srv.url("/auth/signup"))
        .json(&json!({
            "public_key": dup_email_pk,
            "username": "alice-two",
            "email": "alice@example.test",
            "display_name": "Alice Two"
        }))
        .send()
        .await
        .expect("dup email req");
    assert_eq!(dup_email.status().as_u16(), 409);

    // Signin via challenge-response.
    let token = signin(&srv, &sk, None).await;
    assert!(!token.is_empty());

    // Wrong key should fail challenge (unregistered key → 404).
    let (_bad_sk, bad_pk) = gen_keypair();
    let bad = srv
        .client
        .post(srv.url("/auth/challenge"))
        .json(&json!({ "public_key": bad_pk }))
        .send()
        .await
        .expect("bad req");
    assert_eq!(bad.status().as_u16(), 404);

    // Authenticated request.
    let me: Value = srv
        .client
        .get(srv.url("/users/me"))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .expect("me req")
        .json()
        .await
        .expect("me json");
    assert_eq!(me["username"], "alice");

    // Signout.
    let so = srv
        .client
        .post(srv.url("/auth/signout"))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .expect("signout req");
    assert_eq!(so.status().as_u16(), 200);

    // Token should now be revoked.
    let after = srv
        .client
        .get(srv.url("/users/me"))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .expect("after req");
    assert_eq!(after.status().as_u16(), 401);
}

#[tokio::test]
async fn test_multi_account_identity_key_requires_selection() {
    let srv = TestServer::start().await;

    let (sk, first_signup) = signup(&srv, "alice_multi").await;
    let second_signup = signup_with_key(&srv, &sk, "alice_multi_two").await;

    let pk_hex = hex::encode(sk.verifying_key().to_bytes());
    let accounts: Value = srv
        .client
        .post(srv.url("/auth/accounts"))
        .json(&json!({ "public_key": pk_hex }))
        .send()
        .await
        .expect("accounts request")
        .json()
        .await
        .expect("accounts json");

    let listed = accounts["accounts"].as_array().expect("accounts array");
    assert_eq!(listed.len(), 2);

    let ambiguous = srv
        .client
        .post(srv.url("/auth/challenge"))
        .json(&json!({ "public_key": pk_hex }))
        .send()
        .await
        .expect("ambiguous challenge");
    assert_eq!(ambiguous.status().as_u16(), 400);

    let first_user_id = first_signup["user_id"].as_str().expect("first user id");
    let second_user_id = second_signup["user_id"].as_str().expect("second user id");
    assert_ne!(first_user_id, second_user_id);

    let token = signin(&srv, &sk, Some(second_user_id)).await;
    let me: Value = srv
        .client
        .get(srv.url("/users/me"))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .expect("me req")
        .json()
        .await
        .expect("me json");
    assert_eq!(me["username"], "alice_multi_two");
}

#[tokio::test]
async fn test_device_management() {
    let srv = TestServer::start().await;
    let (sk, _) = signup(&srv, "bob").await;
    let token1 = signin(&srv, &sk, None).await;
    let token2 = signin(&srv, &sk, None).await;

    // Both tokens work.
    for t in [&token1, &token2] {
        let r = srv
            .client
            .get(srv.url("/users/me"))
            .header("Authorization", auth_header(t))
            .send()
            .await
            .expect("me");
        assert_eq!(r.status().as_u16(), 200);
    }

    // List devices — should see 3 (signup + 2 signins).
    let devices: Value = srv
        .client
        .get(srv.url("/auth/devices"))
        .header("Authorization", auth_header(&token1))
        .send()
        .await
        .expect("devices req")
        .json()
        .await
        .expect("devices json");
    let arr = devices.as_array().expect("array");
    assert_eq!(arr.len(), 3);

    // Revoke device 2 from device 1.
    let device2_id = arr
        .iter()
        .find(|d| d["revoked"] == false)
        // any non-current device
        .and_then(|d| d["id"].as_str())
        .expect("device id");
    let rev = srv
        .client
        .delete(srv.url(&format!("/auth/devices/{device2_id}")))
        .header("Authorization", auth_header(&token1))
        .send()
        .await
        .expect("revoke req");
    assert_eq!(rev.status().as_u16(), 200);
}

#[tokio::test]
async fn test_server_and_channels() {
    let srv = TestServer::start().await;
    let (sk, _) = signup(&srv, "carol").await;
    let token = signin(&srv, &sk, None).await;
    let hdr = auth_header(&token);

    // Create a server.
    let server: Value = srv
        .client
        .post(srv.url("/servers"))
        .header("Authorization", &hdr)
        .json(&json!({ "name": "Test Guild", "icon_url": null }))
        .send()
        .await
        .expect("create server")
        .json()
        .await
        .expect("server json");
    let server_id = server["id"].as_str().expect("id");
    assert_eq!(server["name"], "Test Guild");

    // Create a channel.
    let channel: Value = srv
        .client
        .post(srv.url(&format!("/servers/{server_id}/channels")))
        .header("Authorization", &hdr)
        .json(&json!({ "name": "general", "kind": "text" }))
        .send()
        .await
        .expect("create channel")
        .json()
        .await
        .expect("channel json");
    let channel_id = channel["id"].as_str().expect("ch id");

    // Send a message.
    let msg: Value = srv
        .client
        .post(srv.url(&format!("/channels/{channel_id}/messages")))
        .header("Authorization", &hdr)
        .json(&json!({ "content": "hello world" }))
        .send()
        .await
        .expect("send msg")
        .json()
        .await
        .expect("msg json");
    assert_eq!(msg["content"], "hello world");
    let msg_id = msg["id"].as_str().expect("msg id");

    // Edit the message.
    let edited: Value = srv
        .client
        .patch(srv.url(&format!("/messages/{msg_id}")))
        .header("Authorization", &hdr)
        .json(&json!({ "content": "hello edited" }))
        .send()
        .await
        .expect("edit")
        .json()
        .await
        .expect("edit json");
    assert_eq!(edited["content"], "hello edited");
    assert!(edited["edited_at"].is_string());

    // Delete the message.
    let del = srv
        .client
        .delete(srv.url(&format!("/messages/{msg_id}")))
        .header("Authorization", &hdr)
        .send()
        .await
        .expect("delete");
    assert_eq!(del.status().as_u16(), 200);

    // Re-fetch — should show [deleted].
    let msgs: Value = srv
        .client
        .get(srv.url(&format!("/channels/{channel_id}/messages")))
        .header("Authorization", &hdr)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("list json");
    let first = &msgs[0];
    assert_eq!(first["content"], "[deleted]");
    assert_eq!(first["deleted"], true);
}

#[tokio::test]
async fn test_reactions() {
    let srv = TestServer::start().await;
    let (sk, _) = signup(&srv, "dan").await;
    let token = signin(&srv, &sk, None).await;
    let hdr = auth_header(&token);

    // Minimal setup.
    let server: Value = srv
        .client
        .post(srv.url("/servers"))
        .header("Authorization", &hdr)
        .json(&json!({ "name": "Reactions Test" }))
        .send()
        .await
        .expect("srv")
        .json()
        .await
        .expect("srv json");
    let sid = server["id"].as_str().expect("sid");
    let ch: Value = srv
        .client
        .post(srv.url(&format!("/servers/{sid}/channels")))
        .header("Authorization", &hdr)
        .json(&json!({ "name": "general", "kind": "text" }))
        .send()
        .await
        .expect("ch")
        .json()
        .await
        .expect("ch json");
    let cid = ch["id"].as_str().expect("cid");
    let msg: Value = srv
        .client
        .post(srv.url(&format!("/channels/{cid}/messages")))
        .header("Authorization", &hdr)
        .json(&json!({ "content": "react to this" }))
        .send()
        .await
        .expect("msg")
        .json()
        .await
        .expect("msg json");
    let mid = msg["id"].as_str().expect("mid");

    // Add reaction.
    let add = srv
        .client
        .post(srv.url(&format!("/messages/{mid}/reactions/👍")))
        .header("Authorization", &hdr)
        .send()
        .await
        .expect("add react");
    assert_eq!(add.status().as_u16(), 200);

    // Idempotent second add.
    let add2 = srv
        .client
        .post(srv.url(&format!("/messages/{mid}/reactions/👍")))
        .header("Authorization", &hdr)
        .send()
        .await
        .expect("add2");
    assert_eq!(add2.status().as_u16(), 200);

    // List.
    let list: Value = srv
        .client
        .get(srv.url(&format!("/messages/{mid}/reactions")))
        .header("Authorization", &hdr)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("list json");
    assert_eq!(list.as_array().expect("arr").len(), 1);

    // Remove.
    let rm = srv
        .client
        .delete(srv.url(&format!("/messages/{mid}/reactions/👍")))
        .header("Authorization", &hdr)
        .send()
        .await
        .expect("rm");
    assert_eq!(rm.status().as_u16(), 200);
}

#[tokio::test]
async fn test_direct_messages() {
    let srv = TestServer::start().await;
    let (eve_sk, _) = signup(&srv, "eve").await;
    let (frank_sk, _) = signup(&srv, "frank").await;
    let eve_token = signin(&srv, &eve_sk, None).await;
    let eve_hdr = auth_header(&eve_token);

    let frank_me: Value = {
        let ft = signin(&srv, &frank_sk, None).await;
        srv.client
            .get(srv.url("/users/me"))
            .header("Authorization", auth_header(&ft))
            .send()
            .await
            .expect("frank me")
            .json()
            .await
            .expect("frank me json")
    };
    let frank_id = frank_me["id"].as_str().expect("frank id");

    // Open DM.
    let dm: Value = srv
        .client
        .post(srv.url("/channels/@dms"))
        .header("Authorization", &eve_hdr)
        .json(&json!({ "user_id": frank_id }))
        .send()
        .await
        .expect("dm")
        .json()
        .await
        .expect("dm json");
    let dm_id = dm["id"].as_str().expect("dm id");

    // Idempotent — second open should return same channel.
    let dm2: Value = srv
        .client
        .post(srv.url("/channels/@dms"))
        .header("Authorization", &eve_hdr)
        .json(&json!({ "user_id": frank_id }))
        .send()
        .await
        .expect("dm2")
        .json()
        .await
        .expect("dm2 json");
    assert_eq!(dm2["id"], dm["id"]);

    // Send message in DM.
    let msg: Value = srv
        .client
        .post(srv.url(&format!("/channels/{dm_id}/messages")))
        .header("Authorization", &eve_hdr)
        .json(&json!({ "content": "hey frank!" }))
        .send()
        .await
        .expect("msg")
        .json()
        .await
        .expect("msg json");
    assert_eq!(msg["content"], "hey frank!");
}

#[tokio::test]
async fn test_file_upload_and_access() {
    let srv = TestServer::start().await;
    let (grace_sk, _) = signup(&srv, "grace").await;
    let (heidi_sk, _) = signup(&srv, "heidi").await;
    let grace_token = signin(&srv, &grace_sk, None).await;
    let grace_hdr = auth_header(&grace_token);
    let heidi_token = signin(&srv, &heidi_sk, None).await;
    let heidi_hdr = auth_header(&heidi_token);

    // Grace uploads a file.
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello file".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .expect("mime"),
    );
    let upload: Value = srv
        .client
        .post(srv.url("/attachments"))
        .header("Authorization", &grace_hdr)
        .multipart(form)
        .send()
        .await
        .expect("upload")
        .json()
        .await
        .expect("upload json");
    let att_id = upload["id"].as_str().expect("att id");

    // Grace can access her own orphan attachment.
    let get1 = srv
        .client
        .get(srv.url(&format!("/attachments/{att_id}")))
        .header("Authorization", &grace_hdr)
        .send()
        .await
        .expect("get1");
    assert_eq!(get1.status().as_u16(), 200);

    // Heidi cannot access it (orphan).
    let get2 = srv
        .client
        .get(srv.url(&format!("/attachments/{att_id}")))
        .header("Authorization", &heidi_hdr)
        .send()
        .await
        .expect("get2");
    assert_eq!(get2.status().as_u16(), 403);

    // Grace creates a server + channel and sends the attachment.
    let server: Value = srv
        .client
        .post(srv.url("/servers"))
        .header("Authorization", &grace_hdr)
        .json(&json!({ "name": "File Test" }))
        .send()
        .await
        .expect("srv")
        .json()
        .await
        .expect("srv json");
    let sid = server["id"].as_str().expect("sid");

    // Invite Heidi.
    let invite: Value = srv
        .client
        .post(srv.url(&format!("/servers/{sid}/invite")))
        .header("Authorization", &grace_hdr)
        .send()
        .await
        .expect("invite")
        .json()
        .await
        .expect("invite json");
    let code = invite["code"].as_str().expect("code");
    srv.client
        .post(srv.url(&format!("/servers/join/{code}")))
        .header("Authorization", &heidi_hdr)
        .send()
        .await
        .expect("join");

    let ch: Value = srv
        .client
        .post(srv.url(&format!("/servers/{sid}/channels")))
        .header("Authorization", &grace_hdr)
        .json(&json!({ "name": "pics", "kind": "text" }))
        .send()
        .await
        .expect("ch")
        .json()
        .await
        .expect("ch json");
    let cid = ch["id"].as_str().expect("cid");

    // Send message with attachment.
    srv.client
        .post(srv.url(&format!("/channels/{cid}/messages")))
        .header("Authorization", &grace_hdr)
        .json(&json!({ "content": "see attached", "attachments": [att_id] }))
        .send()
        .await
        .expect("msg");

    // Now Heidi (a server member) can access the attachment.
    let get3 = srv
        .client
        .get(srv.url(&format!("/attachments/{att_id}")))
        .header("Authorization", &heidi_hdr)
        .send()
        .await
        .expect("get3");
    assert_eq!(get3.status().as_u16(), 200);
}

#[tokio::test]
async fn test_friend_requests() {
    let srv = TestServer::start().await;
    let (ivan_sk, _) = signup(&srv, "ivan").await;
    let (judy_sk, _) = signup(&srv, "judy").await;
    let ivan_token = signin(&srv, &ivan_sk, None).await;
    let ivan_hdr = auth_header(&ivan_token);
    let judy_token = signin(&srv, &judy_sk, None).await;
    let judy_hdr = auth_header(&judy_token);

    // Ivan sends friend request.
    let fr: Value = srv
        .client
        .post(srv.url("/users/me/friends"))
        .header("Authorization", &ivan_hdr)
        .json(&json!({ "username": "judy" }))
        .send()
        .await
        .expect("fr")
        .json()
        .await
        .expect("fr json");
    let fr_id = fr["id"].as_str().expect("fr id");
    assert_eq!(fr["status"], "pending");

    // Judy accepts.
    let accepted: Value = srv
        .client
        .patch(srv.url(&format!("/users/me/friends/{fr_id}")))
        .header("Authorization", &judy_hdr)
        .json(&json!({ "status": "accepted" }))
        .send()
        .await
        .expect("accept")
        .json()
        .await
        .expect("accept json");
    assert_eq!(accepted["status"], "accepted");

    // Ivan sees Judy in friends list.
    let friends: Value = srv
        .client
        .get(srv.url("/users/me/friends"))
        .header("Authorization", &ivan_hdr)
        .send()
        .await
        .expect("friends")
        .json()
        .await
        .expect("friends json");
    let arr = friends.as_array().expect("arr");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["username"], "judy");
}

#[tokio::test]
async fn test_websocket_event_delivery() {
    use futures::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::{connect_async, tungstenite::Message as WsMsg};

    let srv = TestServer::start().await;
    let (kate_sk, _) = signup(&srv, "kate").await;
    let kate_token = signin(&srv, &kate_sk, None).await;
    let kate_hdr = auth_header(&kate_token);

    // Create server + channel.
    let server: Value = srv
        .client
        .post(srv.url("/servers"))
        .header("Authorization", &kate_hdr)
        .json(&json!({ "name": "WS Test" }))
        .send()
        .await
        .expect("srv")
        .json()
        .await
        .expect("srv json");
    let sid = server["id"].as_str().expect("sid");
    let ch: Value = srv
        .client
        .post(srv.url(&format!("/servers/{sid}/channels")))
        .header("Authorization", &kate_hdr)
        .json(&json!({ "name": "chat", "kind": "text" }))
        .send()
        .await
        .expect("ch")
        .json()
        .await
        .expect("ch json");
    let cid = ch["id"].as_str().expect("cid");

    // Connect WebSocket.
    let ws_url = format!("ws://{}/ws?token={}", srv.addr, kate_token);
    let (mut ws_stream, _) = connect_async(&ws_url).await.expect("ws connect");

    // Receive the initial Ping.
    let ping_msg = tokio::time::timeout(Duration::from_secs(2), ws_stream.next())
        .await
        .expect("ping timeout")
        .expect("ping item")
        .expect("ping msg");
    let ping_text = ping_msg.to_text().expect("ping text");
    let ping_val: Value = serde_json::from_str(ping_text).expect("ping json");
    assert_eq!(ping_val["event"], "ping");

    // Send a message via HTTP.
    srv.client
        .post(srv.url(&format!("/channels/{cid}/messages")))
        .header("Authorization", &kate_hdr)
        .json(&json!({ "content": "ws test message" }))
        .send()
        .await
        .expect("http msg");

    // Receive MessageCreated WS event.
    let event_msg = tokio::time::timeout(Duration::from_secs(2), ws_stream.next())
        .await
        .expect("event timeout")
        .expect("event item")
        .expect("event msg");
    let event_text = event_msg.to_text().expect("event text");
    let event: Value = serde_json::from_str(event_text).expect("event json");
    assert_eq!(event["event"], "message_created");
    assert_eq!(event["data"]["content"], "ws test message");

    // Clean disconnect.
    ws_stream.send(WsMsg::Close(None)).await.ok();
}
