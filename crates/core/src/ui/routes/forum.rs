//! Forum-domain route adapter components.
//!
//! Covers forum post thread view, create-forum-post, forum search, and the
//! forum comments feed.

use crate::ui::create_forum_post::{CreateForumPostPage, ForumSearchPage};
use crate::ui::account::ForumPostView;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Forum post thread view — `/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/posts/:post_id`.
///
/// Renders inside `ServerLayout` (sidebar visible). The parent `channel_id` is synced into
/// `AppState.nav.selected_channel` by `sync_route_to_app_state` so the sidebar stays highlighted.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ForumPostRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
    post_id: String,
) -> Element {
    rsx! {
        ForumPostView { channel_id, post_id }
    }
}

/// Create forum post — `/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/create-post`.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn CreateForumPostRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    rsx! {
        CreateForumPostPage { backend, instance_id, account_id, server_id, channel_id }
    }
}

/// Forum search — `/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/search`.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ForumSearchRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    rsx! {
        ForumSearchPage { backend, instance_id, account_id, server_id, channel_id }
    }
}

/// Forum comments feed — `/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/comments`.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ForumCommentsRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let _ = (backend, instance_id, account_id, server_id, channel_id);
    rsx! {
        div { class: "forum-view",
            div { class: "forum-empty",
                div { class: "forum-empty-icon", "💬" }
                p { "Comments feed — coming soon." }
            }
        }
    }
}
