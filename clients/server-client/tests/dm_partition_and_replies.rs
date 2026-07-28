//! Regression tests for three `PolyServerBackend` mapping defects:
//!
//! 1. `get_dm_channels` emitted group DMs as 1:1 DMs, so a group chat appeared
//!    in BOTH the Groups list and the Direct Messages list.
//! 2. `map_message` dropped `WireMessage::reply_to_id`, so a reply refetched
//!    from the server rendered as a standalone message.
//! 3. `get_server_roles` returned `NotSupported` while
//!    `backend_capabilities().has_roles` was `true`, so the Roles tab rendered
//!    and then failed on open.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use axum::middleware;
use axum::Router;
use rand::RngExt;
use tokio::net::TcpListener as TokioListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use poly_client::{AuthCredentials, DmsAndGroupsBackend, IsBackend, MessageQuery, ModerationBackend};
use poly_server::{api, auth, db, ws, AppState, Config};
use poly_server_client::PolyServerBackend;

// ---------------------------------------------------------------------------
// Test harness (mirrors tests/overview.rs)
// ---------------------------------------------------------------------------

struct TestServer {
    addr: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let _logging = tracing_subscriber::fmt()
            .with_env_filter("warn")
            .with_test_writer()
            .try_init();

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);
        let addr = format!("127.0.0.1:{port}");

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
        let _served = tokio::spawn(async move {
            axum::serve(tcp, app)
                .with_graceful_shutdown(async {
                    let _closed = rx.await;
                })
                .await
                .expect("serve");
            drop(tmp);
        });
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

fn random_key() -> [u8; 32] {
    rand::rng().random()
}

/// Sign a fresh account up and return the backend plus the server-assigned
/// user id.
async fn signed_in(srv: &TestServer, username: &str) -> (PolyServerBackend, String) {
    let mut backend = PolyServerBackend::new(&srv.base_url(), random_key());
    let session = backend
        .authenticate(AuthCredentials::PolyServer {
            server_url: srv.base_url(),
            private_key_bytes: random_key().to_vec(),
            username: Some(username.to_string()),
            email: Some(format!("{username}@example.test")),
            display_name: Some(username.to_string()),
            selected_user_id: None,
            is_signup: true,
        })
        .await
        .expect("authenticate");
    let user_id = session.user.id;
    (backend, user_id)
}

// ---------------------------------------------------------------------------
// 1. DM / group partition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn group_dm_is_not_also_listed_as_a_direct_message() {
    let srv = TestServer::start().await;

    let (alice, _alice_id) = signed_in(&srv, "alice_part").await;
    let (_bob, bob_id) = signed_in(&srv, "bob_part").await;
    let (_carol, carol_id) = signed_in(&srv, "carol_part").await;

    // A 1:1 DM (2 participants) and a group DM (3 participants).
    let dm = alice.http().create_dm(&bob_id).await.expect("create_dm");
    let group = alice
        .http()
        .create_group_dm("Trio", &[bob_id.clone(), carol_id])
        .await
        .expect("create_group_dm");

    let dms = alice.get_dm_channels().await.expect("get_dm_channels");
    let groups = alice.get_groups().await.expect("get_groups");

    assert!(
        groups.iter().any(|g| g.id == group.id),
        "the 3-party chat must appear under Groups"
    );
    assert!(
        !dms.iter().any(|d| d.id == group.id),
        "the 3-party chat must NOT also appear under Direct Messages: {:?}",
        dms.iter().map(|d| d.id.as_str()).collect::<Vec<_>>()
    );
    assert!(
        dms.iter().any(|d| d.id == dm.id),
        "the 1:1 DM must still appear under Direct Messages"
    );
    assert!(
        !groups.iter().any(|g| g.id == dm.id),
        "the 1:1 DM must NOT appear under Groups"
    );
}

// ---------------------------------------------------------------------------
// 2. reply_to round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refetched_reply_keeps_its_parent_reference() {
    let srv = TestServer::start().await;
    let (alice, _alice_id) = signed_in(&srv, "alice_reply").await;

    let server = alice
        .http()
        .create_server("Reply Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("server id");
    let channel = alice
        .http()
        .create_channel(&server_id, "replies", "text", None)
        .await
        .expect("create_channel");

    let original = alice
        .http()
        .send_message(&channel.id, "original", None, None)
        .await
        .expect("send original");
    let _reply = alice
        .http()
        .send_message(&channel.id, "a reply", Some(&original.id), None)
        .await
        .expect("send reply");

    let msgs = alice
        .get_messages(
            &channel.id,
            MessageQuery {
                limit: Some(50),
                ..MessageQuery::default()
            },
        )
        .await
        .expect("get_messages");

    let reply = msgs
        .iter()
        .find(|m| matches!(&m.content, poly_client::MessageContent::Text(t) if t == "a reply"))
        .expect("reply is in the channel history");
    let preview = reply
        .reply_to
        .as_ref()
        .expect("a refetched reply must carry a reply_to preview");
    assert_eq!(
        preview.message_id, original.id,
        "reply preview must point at the original message"
    );

    let plain = msgs
        .iter()
        .find(|m| matches!(&m.content, poly_client::MessageContent::Text(t) if t == "original"))
        .expect("original is in the channel history");
    assert!(
        plain.reply_to.is_none(),
        "a non-reply must not gain a reply_to preview"
    );
}

// ---------------------------------------------------------------------------
// 3. Roles capability honours its own flag
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advertised_roles_capability_is_actually_answerable() {
    let srv = TestServer::start().await;
    let (alice, _alice_id) = signed_in(&srv, "alice_roles").await;

    let server = alice
        .http()
        .create_server("Role Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("server id");

    assert!(
        alice.backend_capabilities().has_roles,
        "precondition: poly-server advertises has_roles"
    );

    let roles = alice
        .get_server_roles(&server_id)
        .await
        .expect("has_roles == true must imply get_server_roles answers");

    let ids: Vec<_> = roles.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["owner", "admin", "moderator", "member"]);

    let owner = roles.first().expect("owner rung");
    assert!(owner.permissions.manage_server, "owner may manage the server");
    let member = roles.last().expect("member rung");
    assert!(
        !member.permissions.kick_members,
        "plain members may not kick"
    );
    // Positions must be a strictly descending ladder as returned.
    let positions: Vec<_> = roles.iter().map(|r| r.position).collect();
    assert_eq!(positions, vec![3, 2, 1, 0]);
}
