//! Integration tests for the web-shell sandbox redirect shim (Phase C).
//!
//! Tests the server-side `/sandbox/<id>` route that OAuth providers redirect
//! to. The WASM-side `WebSandbox` impl (apps/web/src/sandbox.rs) cannot be
//! tested here without a real browser; these tests cover the axum handler.
//!
//! See `docs/plans/plan-host-sandbox-impl.md` Phase C, task C.6.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::{Router, body::to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for `oneshot`

/// Build a minimal router with just the sandbox shim route, backed by a
/// real HostState pointing at a temp SQLite file.
fn make_test_router() -> Router {
    use poly_host::HostState;
    use tempfile::tempdir;

    let dir = tempdir().expect("tempdir");
    let db = dir.path().join("test.sqlite3");
    // Keep the tempdir alive for the duration — leak it intentionally in
    // tests (process exits anyway).
    std::mem::forget(dir);
    let state = HostState::open(&db).expect("open HostState");
    poly_host::router(state)
}

/// GET /sandbox/<id> with no query string — the shim still serves the
/// postMessage script because the full `location.href` (including any query
/// string the OAuth provider adds) is read client-side in JS.
#[tokio::test]
async fn shim_returns_html_with_postmessage_script() {
    let app = make_test_router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/sandbox/abc123def456")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"), "content-type should be text/html, got: {ct}");

    let body = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();

    // The shim must embed the sandbox id in a JSON-safe string.
    assert!(html.contains("sandbox-captured"), "missing 'sandbox-captured' type tag");
    assert!(html.contains("abc123def456"), "missing sandbox id in HTML");
    assert!(html.contains("postMessage"), "missing postMessage call");
    assert!(html.contains("window.close()"), "missing window.close()");
    assert!(html.contains("window.opener"), "missing window.opener check");
}

/// GET /sandbox/<id>?token=abc captures the query string server-side only if
/// the page reads `location.href` — which the shim JS does. Verify the page
/// renders correctly when the redirect includes query params.
#[tokio::test]
async fn shim_renders_correctly_with_query_params() {
    let app = make_test_router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/sandbox/mysandboxid?code=oauth_code&state=xyz")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    // The sandbox id appears in the JS; the query params are captured
    // client-side via `window.location.href`.
    assert!(html.contains("mysandboxid"));
}

/// Invalid sandbox ids (path traversal chars, empty) must return 400.
#[tokio::test]
async fn shim_rejects_invalid_ids() {
    let app = make_test_router();

    // Slash in id (path traversal attempt — axum would route it differently
    // but we test the validation for double-encoded slashes).
    // Try an id with a dot which is not allowed by our validator.
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/sandbox/abc..def")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// `GET /sandbox/<id>` sets `Cache-Control: no-store` so intermediate caches
/// never serve a stale shim page to a different sandbox session.
#[tokio::test]
async fn shim_sets_no_store_cache_header() {
    let app = make_test_router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/sandbox/testid1234")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let cc = resp
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(cc.contains("no-store"), "expected no-store, got: {cc}");
}

/// Verify that `advertised_host_caps()` from `poly-host-sandbox` with the
/// `web` feature enabled returns `SandboxBrowser` (C.5).
#[test]
fn web_shell_advertises_sandbox_browser_cap() {
    use poly_client::HostCap;
    use poly_host_sandbox::advertised_host_caps;

    let caps = advertised_host_caps();
    assert!(
        caps.contains(&HostCap::SandboxBrowser),
        "apps/web must advertise SandboxBrowser; got: {caps:?}"
    );
}
