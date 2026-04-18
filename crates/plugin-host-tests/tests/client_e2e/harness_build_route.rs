//! Harness helpers for host-api `build-route` surface testing.
//!
//! Skeletons only — bodies are `todo!()`. Filled in WP 1.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_plugin_host::PluginBackend;

/// Verify that the backend can call `host-api.build-route` and receive a valid URL string.
#[allow(dead_code)]
pub async fn plugin_builds_routes_via_host_api(backend: &PluginBackend) {
    todo!("WP 1: implement per plan")
}

/// Verify that supplying an unrecognized `route-kind` value returns a well-formed
/// `route-build-error` (not a panic or trap).
#[allow(dead_code)]
pub async fn invalid_route_kind_returns_error(backend: &PluginBackend) {
    todo!("WP 1: implement per plan")
}

/// Verify that every `navigate` action outcome produced by the backend contains a
/// route URL that passes the build-route validator.
#[allow(dead_code)]
pub async fn navigate_outcome_routes_are_valid(backend: &PluginBackend) {
    todo!("WP 1: implement per plan")
}
