//! Banner round-trip tests for the poly-server-client.
//!
//! Spins up a real poly-server instance, creates a server, then exercises
//! `update_server_banner` (PATCH /servers/:id with `banner_url`) and
//! verifies the change is visible in a subsequent `get_servers` call.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

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

// ---------------------------------------------------------------------------
// Test harness (shared with integration.rs)
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

fn make_client(srv: &TestServer) -> PolyServerHttpClient {
    let mut rng = rand::rng();
    let key: [u8; 32] = rng.random();
    PolyServerHttpClient::new(PolyServerConfig {
        base_url: srv.base_url(),
        private_key_bytes: key,
    })
}

fn test_email(u: &str) -> String {
    format!("{u}@banner.test")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Set a banner URL via PATCH and verify it persists in the server list.
#[tokio::test]
async fn set_banner_url_persists() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);

    client
        .signup("alice_banner", &test_email("alice_banner"), None)
        .await
        .expect("signup");

    let server = client
        .create_server("Banner Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("server id");

    client
        .update_server_banner(&server_id, Some("https://example.com/guild-banner.png"))
        .await
        .expect("update_server_banner should succeed");

    // Re-read and verify.
    let servers = client.get_servers().await.expect("get_servers");
    let found = servers
        .iter()
        .find(|s| s.id.as_deref() == Some(&*server_id))
        .expect("server not found");
    assert_eq!(
        found.banner_url.as_deref(),
        Some("https://example.com/guild-banner.png"),
        "banner_url should be set"
    );
}

/// Clear a banner by passing `None`.
#[tokio::test]
async fn clear_banner_url() {
    let srv = TestServer::start().await;
    let client = make_client(&srv);

    client
        .signup("bob_banner", &test_email("bob_banner"), None)
        .await
        .expect("signup");

    let server = client
        .create_server("Clear Banner Guild")
        .await
        .expect("create_server");
    let server_id = server.id.expect("server id");

    client
        .update_server_banner(&server_id, Some("https://example.com/temp-banner.png"))
        .await
        .expect("set banner");

    client
        .update_server_banner(&server_id, None)
        .await
        .expect("clear banner");

    let servers = client.get_servers().await.expect("get_servers");
    let found = servers
        .iter()
        .find(|s| s.id.as_deref() == Some(&*server_id))
        .expect("server not found");
    assert!(
        found.banner_url.is_none(),
        "banner_url should be None after clearing, got: {:?}",
        found.banner_url
    );
}
