//! Harness helpers for channel-view surface testing (forum, feed, issue-tracker, custom-block).
//!
//! Skeletons only — bodies are `todo!()`. Filled in WP 5.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_client::{ClientBackend, ClientError, ViewBody};
use poly_plugin_host::PluginBackend;

/// Verify that the view descriptor returned for the given channel is structurally well-formed.
///
/// Pack A follow-up: a backend that declares a non-chat view for `ch_id`
/// must return a descriptor whose required fields are all populated:
///
/// - `body` picks a known engine (list/card/tree/split).
/// - If a `toolbar` is present, every option in it has a non-empty
///   `id` and `label_key`.
/// - If a `header` is present and sets a title, the title key is
///   non-empty.
/// - Body-engine spec invariants: list `page_size > 0`, tree
///   `root_page_size > 0` and `max_depth >= 1`.
///
/// Backends that declare **no** non-chat view for the given channel
/// return `Err(NotSupported(_))` / `Err(NotFound(_))`. That path is
/// accepted and exits quietly — the assertion only fires when a
/// descriptor is actually produced.
#[allow(dead_code)]
pub async fn channel_view_descriptor_well_formed(backend: &PluginBackend, ch_id: &str) {
    let result = backend.get_channel_view(ch_id).await;
    let desc = match result {
        Ok(d) => d,
        Err(ClientError::NotSupported(_)) | Err(ClientError::NotFound(_)) => return,
        Err(e) => panic!("unexpected error from get_channel_view({ch_id}): {e:?}"),
    };

    if let Some(toolbar) = &desc.toolbar {
        for o in toolbar
            .sort_options
            .iter()
            .chain(toolbar.filter_options.iter())
            .chain(toolbar.tabs.iter())
        {
            assert!(!o.id.is_empty(), "toolbar option id must be non-empty");
            assert!(
                !o.label_key.is_empty(),
                "toolbar option label_key must be non-empty (option id={})",
                o.id
            );
        }
    }

    if let Some(header) = &desc.header {
        if let Some(title_key) = &header.title_key {
            assert!(
                !title_key.is_empty(),
                "header.title_key, when Some, must be non-empty"
            );
        }
        if let Some(subtitle_key) = &header.subtitle_key {
            assert!(
                !subtitle_key.is_empty(),
                "header.subtitle_key, when Some, must be non-empty"
            );
        }
    }

    match &desc.body {
        ViewBody::ListBody(spec) => {
            assert!(
                spec.page_size > 0,
                "ListSpec::page_size must be > 0, got {}",
                spec.page_size
            );
            assert!(
                !spec.row_template.primary_field.is_empty(),
                "ListSpec.row_template.primary_field must be non-empty"
            );
        }
        ViewBody::CardBody(spec) => {
            assert!(
                !spec.primary_field.is_empty(),
                "CardSpec::primary_field must be non-empty"
            );
        }
        ViewBody::TreeBody(spec) => {
            assert!(
                spec.root_page_size > 0,
                "TreeSpec::root_page_size must be > 0, got {}",
                spec.root_page_size
            );
            assert!(
                spec.max_depth >= 1,
                "TreeSpec::max_depth must be >= 1, got {}",
                spec.max_depth
            );
        }
        ViewBody::SplitBody(spec) => {
            assert!(
                spec.list_side.page_size > 0,
                "SplitSpec::list_side.page_size must be > 0, got {}",
                spec.list_side.page_size
            );
        }
    }
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
