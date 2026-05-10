//! Tests for `get_account_overview_view` and overview-path `get_view_rows`.
//!
//! Spins up a real poly-server instance via the embedded test harness, creates
//! an account with two servers, then exercises both trait methods through
//! `PolyServerBackend` (the `ClientBackend` impl).

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
use poly_server_client::PolyServerBackend;

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct TestServer {
    addr: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let _ = tracing_subscriber::fmt()
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
        let state = AppState { db: db_obj, config, ws: ws_state };

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
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .expect("serve");
            drop(tmp);
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        Self { addr, _shutdown: tx }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

fn random_key() -> [u8; 32] {
    rand::rng().random()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `get_account_overview_view` must return a `CardGrid` with `CardBody`.
#[tokio::test]
async fn overview_view_descriptor_is_card_grid() {
    // No server connection needed — the descriptor is built locally.
    let backend = PolyServerBackend::new("http://127.0.0.1:9999", random_key());
    let desc = backend
        .get_account_overview_view()
        .await
        .expect("get_account_overview_view");

    assert_eq!(desc.kind, ViewKind::CardGrid, "kind must be CardGrid");
    assert!(
        matches!(desc.body, ViewBody::CardBody(_)),
        "body must be CardBody, got: {:?}",
        desc.body
    );
    assert!(
        desc.header.is_some(),
        "header should be present with title/subtitle keys"
    );
    let header = desc.header.unwrap();
    assert!(
        header.title_key.is_some(),
        "title_key must be set"
    );
    assert!(
        header.subtitle_key.is_some(),
        "subtitle_key must be set"
    );
}

/// `get_view_rows` with `channel_id=""` (overview sentinel) returns one row
/// per joined server. Each row has `primary_text == server.name` and a
/// `meta_text` containing member / unread / mention counts.
#[tokio::test]
async fn overview_view_rows_contain_joined_servers() {
    let srv = TestServer::start().await;

    let mut backend = PolyServerBackend::new(&srv.base_url(), random_key());
    backend
        .authenticate(AuthCredentials::PolyServer {
            server_url: srv.base_url(),
            private_key_bytes: random_key().to_vec(),
            username: Some("overview_user".to_string()),
            email: Some("overview_user@example.test".to_string()),
            display_name: Some("Overview User".to_string()),
            selected_user_id: None,
            is_signup: true,
        })
        .await
        .expect("authenticate");

    // Create two servers.
    let http = backend.http();
    let s1 = http.create_server("Alpha Guild").await.expect("create s1");
    let s2 = http.create_server("Beta Guild").await.expect("create s2");
    let s1_id = s1.id.as_deref().expect("s1 id");
    let s2_id = s2.id.as_deref().expect("s2 id");

    // Add a channel to each so they're non-trivial.
    http.create_channel(s1_id, "general", "text", None)
        .await
        .expect("ch s1");
    http.create_channel(s2_id, "general", "text", None)
        .await
        .expect("ch s2");

    // Call get_view_rows with the overview sentinel (empty channel_id).
    let page = backend
        .get_view_rows("", None, None, None, None)
        .await
        .expect("get_view_rows overview");

    // Must have at least the two servers we created.
    assert!(
        page.rows.len() >= 2,
        "Expected >= 2 rows, got {}: {:?}",
        page.rows.len(),
        page.rows.iter().map(|r| &r.primary_text).collect::<Vec<_>>()
    );

    let names: Vec<&str> = page.rows.iter().map(|r| r.primary_text.as_str()).collect();
    assert!(names.contains(&"Alpha Guild"), "Alpha Guild missing: {names:?}");
    assert!(names.contains(&"Beta Guild"), "Beta Guild missing: {names:?}");

    // Each row must have meta_text containing "members".
    for row in &page.rows {
        let meta = row.meta_text.as_deref().unwrap_or("");
        assert!(
            meta.contains("members"),
            "meta_text for '{}' must contain 'members', got: {meta:?}",
            row.primary_text
        );
        assert!(
            meta.contains("unread"),
            "meta_text for '{}' must contain 'unread', got: {meta:?}",
            row.primary_text
        );
        assert!(
            meta.contains("mentions"),
            "meta_text for '{}' must contain 'mentions', got: {meta:?}",
            row.primary_text
        );
    }

    // `get_view_rows` with a non-empty channel_id must return NotSupported.
    let err = backend
        .get_view_rows("some-channel-id", None, None, None, None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, poly_client::ClientError::NotSupported(_)),
        "non-empty channel_id should return NotSupported, got: {err:?}"
    );
}

/// The overview `get_view_rows` returns an empty list when the account has
/// not joined any servers (freshly created account).
#[tokio::test]
async fn overview_view_rows_empty_when_no_servers() {
    let srv = TestServer::start().await;

    let mut backend = PolyServerBackend::new(&srv.base_url(), random_key());
    backend
        .authenticate(AuthCredentials::PolyServer {
            server_url: srv.base_url(),
            private_key_bytes: random_key().to_vec(),
            username: Some("no_servers_user".to_string()),
            email: Some("no_servers_user@example.test".to_string()),
            display_name: None,
            selected_user_id: None,
            is_signup: true,
        })
        .await
        .expect("authenticate");

    let page = backend
        .get_view_rows("", None, None, None, None)
        .await
        .expect("get_view_rows overview");

    assert!(
        page.rows.is_empty(),
        "Newly created account should have 0 server rows, got: {:?}",
        page.rows.iter().map(|r| &r.primary_text).collect::<Vec<_>>()
    );
    assert!(page.next_cursor.is_none(), "next_cursor should be None");
}
