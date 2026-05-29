//! poly-backup-server — Encrypted settings backup/sync server for Poly.
//!
//! An Axum-based REST API server that stores encrypted settings blobs
//! identified by Ed25519 public keys. The server never sees plaintext data.
//!
//! ## Core modules
//! - [`auth`] — PoW challenge generation, passphrase verification, session tokens
//! - [`sync`] — Push/pull encrypted blobs with monotonic sequence numbers
//! - [`web`]  — Admin HTML UI (Tailwind + Alpine.js) with session auth + PoW login
//! - [`db`]   — SurrealKV init + schema
//! - [`config`] — Env-var-based configuration
//! - [`error`] — Unified `AppError` / `Result` types
//!
//! ## OpenAPI spec
//! Available at `/swagger-ui` (Swagger UI) and `/api-docs/openapi.json`.

// needless_for_each fires inside utoipa's OpenApi proc-macro expansion — not our code.
#![allow(clippy::needless_for_each)]

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod info;
pub mod sync;
pub mod web;

use std::sync::Arc;

use axum::{Json, Router, response::Html, routing::get};
use utoipa::OpenApi;

pub use config::Config;
pub use db::{Db, init as init_db};
pub use web::AdminState;

// ── AppState ───────────────────────────────────────────────────────────────────

/// Shared application state threaded through all Axum handlers.
///
/// All fields are cheap to clone (`Arc`-wrapped where necessary).
#[derive(Clone)]
pub struct AppState {
    /// Embedded SurrealKV database handle.
    pub db: Db,
    /// Server configuration (immutable after startup).
    pub config: Arc<Config>,
    /// Admin UI state: sessions, PoW challenges, rate limiter.
    pub admin: Arc<AdminState>,
}

// ── OpenAPI / Swagger ─────────────────────────────────────────────────────────

/// Top-level OpenAPI 3.1 document.
///
/// All endpoint schemas and paths are collected here via `#[utoipa::path]` macros
/// on each handler. When adding a new endpoint:
/// 1. Add `#[utoipa::path(...)]` to the handler in its module.
/// 2. Add the handler function path to `paths(...)` below.
/// 3. Add any new request/response structs to `components(schemas(...))`.
/// 4. Run `cargo doc` to verify the generated spec.
// needless_for_each fires inside OpenApi derive-generated code — not in our hand-written code.
#[allow(clippy::needless_for_each)]
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Poly Backup Server API",
        version = "0.1.0",
        description = "Encrypted settings sync for Poly (PolyGlot Messenger). \
            All blobs are opaque to the server — the server only stores ciphertext.",
        contact(name = "Poly Project", url = "https://github.com/user/poly"),
        license(name = "MIT OR Apache-2.0"),
    ),
    paths(
        info::server_info,
        auth::request_challenge,
        auth::authenticate,
        sync::push,
        sync::pull,
        sync::status,
    ),
    components(
        schemas(
            info::ServerInfoResponse,
            auth::ChallengeRequest,
            auth::ChallengeResponse,
            auth::AuthRequest,
            auth::AuthResponse,
            sync::PushRequest,
            sync::PushResponse,
            sync::PullQuery,
            sync::BlobEntry,
            sync::PullResponse,
            sync::SyncStatusResponse,
        )
    ),
    tags(
        (name = "info", description = "Public server metadata"),
        (name = "auth", description = "PoW challenge + passphrase authentication"),
        (name = "sync", description = "Push/pull encrypted settings blobs"),
    ),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;

/// Adds the `BearerAuth` security scheme to the generated OpenAPI spec.
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "BearerAuth",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

// ── Health check ──────────────────────────────────────────────────────────────

/// `GET /api/health` — liveness probe.
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// `GET /api-docs/openapi.json` — machine-readable OpenAPI 3.1 spec.
async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// `GET /swagger-ui` — browser-friendly API documentation via Swagger UI CDN.
async fn swagger_ui() -> Html<&'static str> {
    Html(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Poly Backup Server — API Docs</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    SwaggerUIBundle({
      url: '/api-docs/openapi.json',
      dom_id: '#swagger-ui',
      presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
      layout: 'BaseLayout',
      deepLinking: true,
    });
  </script>
</body>
</html>"#,
    )
}

// ── Router assembly ───────────────────────────────────────────────────────────

/// Assemble the complete Axum router.
///
/// Mount order:
/// 1. Swagger UI at `/swagger-ui`
/// 2. OpenAPI spec at `/api-docs/openapi.json`
/// 3. API routes at `/api/...`
/// 4. Admin UI and admin API at `/` and `/admin/...`
pub fn create_app(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/api/health", get(health_check))
        .route("/api/info", get(info::server_info))
        .route(
            "/api/challenge",
            axum::routing::post(auth::request_challenge),
        )
        .route("/api/auth", axum::routing::post(auth::authenticate))
        .route("/api/sync/push", axum::routing::post(sync::push))
        .route("/api/sync/pull", get(sync::pull))
        .route("/api/sync/status", get(sync::status))
        .with_state(state.clone());

    let admin_routes = web::admin_router().with_state(state);

    Router::new()
        .route("/swagger-ui", get(swagger_ui))
        .route("/api-docs/openapi.json", get(openapi_json))
        .merge(api_routes)
        .merge(admin_routes)
}
