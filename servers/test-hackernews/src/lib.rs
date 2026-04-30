#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]
//! Mock Hacker News Firebase API server for Poly testing.
//!
//! Implements the subset of the HN Firebase REST API that `poly-hackernews`
//! calls:
//!
//! - `GET /v0/topstories.json`
//! - `GET /v0/newstories.json`
//! - `GET /v0/beststories.json`
//! - `GET /v0/askstories.json`
//! - `GET /v0/showstories.json`
//! - `GET /v0/jobstories.json`
//! - `GET /v0/item/{id}.json`

use axum::middleware;
use axum::Router;
use axum::routing::get;
use poly_test_common::{
    handle_inspect_last_headers, header_inspect_middleware, health_handler, HeaderInspectBuffer,
    TestServerBase,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

// ---------------------------------------------------------------------------
// Seed data
// ---------------------------------------------------------------------------

/// Pre-built seed story items. Each is a minimal HN "story" object.
fn seed_items() -> Vec<Value> {
    vec![
        json!({
            "id": 1001,
            "type": "story",
            "by": "pg",
            "time": 1700000001,
            "title": "Show HN: A new kind of internet",
            "url": "https://example.com/new-internet",
            "score": 512,
            "descendants": 87,
            "kids": [2001, 2002]
        }),
        json!({
            "id": 1002,
            "type": "story",
            "by": "dang",
            "time": 1700000002,
            "title": "Ask HN: What are you working on? (April 2026)",
            "text": "Monthly thread. Share what you&#x27;re building.",
            "score": 340,
            "descendants": 201,
            "kids": [2003]
        }),
        json!({
            "id": 1003,
            "type": "story",
            "by": "todsacerdoti",
            "time": 1700000003,
            "title": "The unreasonable effectiveness of simple code",
            "url": "https://example.com/simple-code",
            "score": 298,
            "descendants": 54,
            "kids": []
        }),
        json!({
            "id": 1004,
            "type": "story",
            "by": "tptacek",
            "time": 1700000004,
            "title": "Hacking the Gibson: a retrospective",
            "url": "https://example.com/gibson",
            "score": 189,
            "descendants": 33,
            "kids": []
        }),
        json!({
            "id": 1005,
            "type": "job",
            "by": "acme",
            "time": 1700000005,
            "title": "Acme Corp is hiring Rust engineers (remote)",
            "url": "https://acme.example.com/jobs",
            "score": 1,
            "kids": []
        }),
    ]
}

/// Pre-built seed comment items.
fn seed_comments() -> Vec<Value> {
    vec![
        json!({
            "id": 2001,
            "type": "comment",
            "by": "patio11",
            "time": 1700000010,
            "text": "This is genuinely interesting. The latency implications alone are worth exploring.",
            "parent": 1001,
            "kids": []
        }),
        json!({
            "id": 2002,
            "type": "comment",
            "by": "tptacek",
            "time": 1700000020,
            "text": "I&#x27;ve seen similar approaches fail because of CAP theorem issues. How do you handle partition tolerance?",
            "parent": 1001,
            "kids": []
        }),
        json!({
            "id": 2003,
            "type": "comment",
            "by": "pg",
            "time": 1700000030,
            "text": "Working on a new Lisp dialect that compiles to WebAssembly. Early days but promising.",
            "parent": 1002,
            "kids": []
        }),
    ]
}

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

/// Shared in-memory state for the mock HN server.
#[derive(Clone)]
pub struct HnState {
    /// All story and comment items, keyed by ID.
    pub items: Arc<dashmap::DashMap<u64, Value>>,
    /// Feed lists, keyed by feed name (e.g. "top", "new", etc.).
    pub feeds: Arc<dashmap::DashMap<String, Vec<u64>>>,
    /// Ring buffer of recent inbound request headers (Phase E inspection endpoint).
    pub inspect: Arc<HeaderInspectBuffer>,
}

impl HnState {
    /// Create an empty state.
    pub fn new() -> Self {
        Self {
            items: Arc::new(dashmap::DashMap::new()),
            feeds: Arc::new(dashmap::DashMap::new()),
            inspect: Arc::new(HeaderInspectBuffer::new()),
        }
    }

    /// Populate seed data.
    pub fn seed(&self) {
        let items = seed_items();
        let comments = seed_comments();

        // Index all items
        for item in &items {
            if let Some(id) = item.get("id").and_then(serde_json::Value::as_u64) {
                self.items.insert(id, item.clone());
            }
        }
        for comment in &comments {
            if let Some(id) = comment.get("id").and_then(serde_json::Value::as_u64) {
                self.items.insert(id, comment.clone());
            }
        }

        // Build feed lists from seeded stories
        let story_ids: Vec<u64> = items
            .iter()
            .filter(|i| i.get("type").and_then(serde_json::Value::as_str) != Some("job"))
            .filter_map(|i| i.get("id").and_then(serde_json::Value::as_u64))
            .collect();
        let job_ids: Vec<u64> = items
            .iter()
            .filter(|i| i.get("type").and_then(serde_json::Value::as_str) == Some("job"))
            .filter_map(|i| i.get("id").and_then(serde_json::Value::as_u64))
            .collect();

        self.feeds.insert("top".to_string(), story_ids.clone());
        self.feeds.insert("new".to_string(), story_ids.clone());
        self.feeds.insert("best".to_string(), story_ids.clone());
        self.feeds.insert("ask".to_string(), story_ids.clone());
        self.feeds.insert("show".to_string(), story_ids.clone());
        self.feeds.insert("jobs".to_string(), job_ids);
    }
}

impl Default for HnState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn feed_handler(
    axum::extract::Path(feed): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<Arc<HnState>>,
) -> axum::response::Json<Value> {
    let ids = state
        .feeds
        .get(&feed)
        .map(|v| v.clone())
        .unwrap_or_default();
    axum::response::Json(json!(ids))
}

async fn item_handler(
    axum::extract::Path(id_json): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<Arc<HnState>>,
) -> axum::response::Json<Value> {
    // id_json is e.g. "1001.json" — strip the ".json" suffix
    let id_str = id_json.trim_end_matches(".json");
    let id: u64 = match id_str.parse() {
        Ok(n) => n,
        Err(_) => return axum::response::Json(Value::Null),
    };
    let item = state.items.get(&id).map(|v| v.clone()).unwrap_or(Value::Null);
    axum::response::Json(item)
}

async fn user_handler(
    axum::extract::Path(user_json): axum::extract::Path<String>,
) -> axum::response::Json<Value> {
    let username = user_json.trim_end_matches(".json");
    // Return a minimal HN user profile
    axum::response::Json(json!({
        "id": username,
        "created": 1_000_000_u64,
        "karma": 100,
        "about": null,
        "submitted": []
    }))
}

async fn test_auth_token_handler() -> axum::response::Json<Value> {
    // HN requires no authentication — return an empty guest token.
    axum::response::Json(json!({ "token": "" }))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the axum router for the mock HN server.
pub fn router(state: Arc<HnState>) -> Router {
    let inspect = Arc::clone(&state.inspect);
    Router::new()
        .route(
            "/health",
            get(|| async { health_handler("hackernews").await }),
        )
        // Feed lists
        .route("/v0/topstories.json", get({
            let s = state.clone();
            move || {
                let s = s.clone();
                async move { feed_handler(axum::extract::Path("top".to_string()), axum::extract::State(s)).await }
            }
        }))
        .route("/v0/newstories.json", get({
            let s = state.clone();
            move || {
                let s = s.clone();
                async move { feed_handler(axum::extract::Path("new".to_string()), axum::extract::State(s)).await }
            }
        }))
        .route("/v0/beststories.json", get({
            let s = state.clone();
            move || {
                let s = s.clone();
                async move { feed_handler(axum::extract::Path("best".to_string()), axum::extract::State(s)).await }
            }
        }))
        .route("/v0/askstories.json", get({
            let s = state.clone();
            move || {
                let s = s.clone();
                async move { feed_handler(axum::extract::Path("ask".to_string()), axum::extract::State(s)).await }
            }
        }))
        .route("/v0/showstories.json", get({
            let s = state.clone();
            move || {
                let s = s.clone();
                async move { feed_handler(axum::extract::Path("show".to_string()), axum::extract::State(s)).await }
            }
        }))
        .route("/v0/jobstories.json", get({
            let s = state.clone();
            move || {
                let s = s.clone();
                async move { feed_handler(axum::extract::Path("jobs".to_string()), axum::extract::State(s)).await }
            }
        }))
        // Individual items — route captures "1234.json", handler strips ".json"
        .route("/v0/item/{id_json}", get(item_handler))
        // User profiles
        .route("/v0/user/{user_json}", get(user_handler))
        // Test-only bypass: guest auth token
        .route("/test/auth/token", axum::routing::post(test_auth_token_handler))
        // Inspection endpoints (Phase E)
        .route(
            "/test/inspect/last-headers",
            get(handle_inspect_last_headers).with_state(Arc::clone(&inspect)),
        )
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            Arc::clone(&inspect),
            header_inspect_middleware,
        ))
        .layer(CorsLayer::very_permissive())
}

// ---------------------------------------------------------------------------
// In-process test server helper
// ---------------------------------------------------------------------------

/// A running in-process test server.
pub struct TestHnServer {
    /// Base URL at which the server is listening (e.g. `http://127.0.0.1:51234`).
    pub base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestHnServer {
    /// Start the mock HN server on a random free port, with seed data.
    pub async fn start() -> Self {
        let state = Arc::new(HnState::new());
        state.seed();

        let base = TestServerBase::bind(0).await.expect("bind free port");
        let base_url = base.base_url();

        let app = router(state);
        tokio::spawn(async move {
            axum::serve(base.listener, app)
                .with_graceful_shutdown(async {
                    let _ = base.shutdown_rx.await;
                })
                .await
                .expect("serve mock HN");
        });

        // Give the server a moment to start accepting connections.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        Self {
            base_url,
            _shutdown: base.shutdown_tx,
        }
    }
}
