//! Harness helpers for custom-block sanitization testing.
//!
//! Skeletons only — bodies are `todo!()`. Filled in WP 5.
//! `custom_block_scripts_stripped` and `custom_block_javascript_url_stripped`
//! are synchronous unit-style helpers that live here for co-location with
//! the async `custom_block_survives_sanitization`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_plugin_host::PluginBackend;

/// Ask the backend to render a custom-block payload and verify the sanitized output
/// survives a full round-trip (non-empty, no parse errors).
#[allow(dead_code)]
pub async fn custom_block_survives_sanitization(backend: &PluginBackend) {
    todo!("WP 5: implement per plan")
}

/// Verify that a raw HTML string containing `<script>` tags is stripped by the sanitizer.
#[allow(dead_code)]
pub fn custom_block_scripts_stripped(test: &str) {
    todo!("WP 5: implement per plan")
}

/// Verify that a raw HTML string containing `javascript:` URLs in `<a href>` attributes
/// is stripped by the sanitizer.
#[allow(dead_code)]
pub fn custom_block_javascript_url_stripped(test: &str) {
    todo!("WP 5: implement per plan")
}
