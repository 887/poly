//! Harness helpers for channel-view surface testing (forum, feed, issue-tracker, custom-block).
//!
//! Skeletons only — bodies are `todo!()`. Filled in WP 5.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_plugin_host::PluginBackend;

/// Verify that the view descriptor returned for the given channel is structurally well-formed.
#[allow(dead_code)]
pub async fn channel_view_descriptor_well_formed(backend: &PluginBackend, ch_id: &str) {
    todo!("WP 5: implement per plan")
}

/// Fetch the first page of rows, then the next page using the returned cursor, and assert
/// that pagination is consistent (no duplicate IDs, cursors change).
#[allow(dead_code)]
pub async fn view_rows_paginate(backend: &PluginBackend, ch_id: &str) {
    todo!("WP 5: implement per plan")
}

/// Verify that the cursor returned by a view rows response is a valid structured value
/// (can be serialized and deserialized round-trip).
#[allow(dead_code)]
pub async fn view_cursor_is_structured(backend: &PluginBackend, ch_id: &str) {
    todo!("WP 5: implement per plan")
}

/// Fetch a view detail row and verify it returns a well-formed custom-block payload.
#[allow(dead_code)]
pub async fn view_detail_returns_custom_block(
    backend: &PluginBackend,
    ch_id: &str,
    row_id: &str,
) {
    todo!("WP 5: implement per plan")
}
