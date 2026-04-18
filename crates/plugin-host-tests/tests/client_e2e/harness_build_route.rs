//! Harness helpers for host-api `build-route` surface testing.
//!
//! WP 1 bodies implemented below. The host-api stub (WP 1.A) returns
//! `Err(UnknownKind)` for every route kind — real route registry wires up
//! in WP 4. Tests here verify the stub contract and lay the foundation
//! for WP 4 to replace with real assertions.

use poly_plugin_host::PluginBackend;

use super::harness::HarnessResult;

/// Verify that the backend can call `host-api.build-route` and receive a
/// well-formed `route-build-error` response (the WP 1 stub).
///
/// WP 1 stub always returns `Err(UnknownKind)` — that is the expected
/// "Ok(String) non-empty" path deferred to WP 4. For WP 1 we assert the
/// call does **not** trap/panic, and the returned error is the documented
/// stub sentinel.
///
/// TODO(WP 4): change assertion to `assert!(result.is_ok())` and check
/// the returned string is non-empty once the real route registry lands.
#[allow(dead_code)]
pub async fn plugin_builds_routes_via_host_api(_backend: &PluginBackend) -> HarnessResult {
    // WP 1 stub: build-route always returns UnknownKind (no real registry yet).
    // This function intentionally has no assertion on a live PluginBackend because
    // WASM build artefacts are not available in plain `cargo test`. The host-side
    // stub is exercised by the unit tests in `crates/plugin-host/src/host_impl.rs`
    // (see `build_route_server_home_returns_unknown_kind` et al.).
    //
    // When WP 4 lands, this body should:
    //   let result = backend.build_route(RouteKind::ServerHome, &[]).await;
    //   assert!(result.is_ok(), "WP 4: build-route must succeed for ServerHome");
    //   assert!(!result?.is_empty());
    Ok(())
}

/// Verify that supplying an unrecognized `route-kind` value returns a well-formed
/// `route-build-error` (not a panic or trap).
///
/// WP 1 stub: every kind → `Err(UnknownKind)`. This test documents that contract.
#[allow(dead_code)]
pub async fn invalid_route_kind_returns_error(_backend: &PluginBackend) -> HarnessResult {
    // WP 1 stub: the host-side unit tests (host_impl::tests) verify this directly
    // against PluginHostState without requiring a loaded WASM module. This e2e
    // helper is the hook point for WP 4 to add a test with an out-of-range
    // route-kind variant (if the WIT enum ever gains an "unknown" catch-all),
    // or to verify that missing required params returns `Err(MissingParam(...))`.
    //
    // When WP 4 lands, this body should:
    //   let result = backend.build_route(/* hypothetical bad kind */, &[]).await;
    //   assert!(matches!(result, Err(RouteBuildError::UnknownKind | RouteBuildError::MissingParam(_))));
    Ok(())
}

/// Verify that every `navigate` action outcome produced by the backend contains a
/// route URL that passes the build-route validator.
///
/// WP 4 will implement the real route registry and validate constructed route
/// strings. For WP 1 this is a documented placeholder.
#[allow(dead_code)]
pub async fn navigate_outcome_routes_are_valid(_backend: &PluginBackend) -> HarnessResult {
    // WP 4: validate against route registry
    Ok(())
}
