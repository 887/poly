//! Discord-style forum channel view.
//!
//! Rendered when the user opens a `ChannelType::Forum` channel on a backend
//! that is NOT itself a forum-layout backend (i.e. Discord / generic backends
//! that carry individual forum channels inside a normal server).
//!
//! ## Layout
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │ Channel name  ·  sort toggle (Latest / Creation)  · [+ New] │  ← ForumHeader
//! ├──────────────────────────────────────────────────────────────┤
//! │ [tag1] [tag2] [tag3] …  [× clear]                           │  ← ForumTagBar
//! ├──────────────────────────────────────────────────────────────┤
//! │ (list rows)  OR  (gallery grid)                             │  ← ForumPostList / ForumPostGallery
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Sub-components (all in this file, each ≤ 150 lines of RSX)
//!
//! | Name | Responsibility |
//! |---|---|
//! | `DiscordForumView` | top-level: fetches posts, owns sort + tag filter signals |
//! | `ForumHeader` | channel name, sort toggle, New Post button |
//! | `ForumTagBar` | tag pill row + clear button |
//! | `ForumPostList` | list-layout row per post |
//! | `ForumPostRow` | single row (emoji, title, chips, counts, timestamp) |
//! | `ForumPostGallery` | gallery grid for media-style channels |
//! | `ForumPostCard` | single gallery card |
//! | `NewPostModal` | compose dialog (title + tags + body + submit) |

use crate::state::BatchedSignal;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::state::{AppState, ChatData};
use crate::ui::routes::Route;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use poly_client::{Channel, ForumPost, ForumSortOrder, ForumTag};
use poly_ui_macros::{context_menu, ui_action};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn fmt_ts(ts: DateTime<Utc>) -> String {
    let local = ts.with_timezone(&chrono::Local);
    let now = chrono::Local::now();
    let diff = now.signed_duration_since(local);
    let secs = diff.num_seconds();
    if secs < 60 {
        return "just now".to_string();
    }
    let m = diff.num_minutes();
    if m < 60 {
        return format!("{m}m ago");
    }
    let h = diff.num_hours();
    if h < 24 {
        return format!("{h}h ago");
    }
    let d = diff.num_days();
    if d < 7 {
        return format!("{d}d ago");
    }
    local.format("%b %-d, %Y").to_string()
}

/// Return `true` when the channel's tags suggest a media-style forum
/// (i.e. at least one tag carries an emoji).  Discord's `default_forum_layout`
/// field isn't carried in our model yet, so this acts as a heuristic
/// placeholder until that field lands.
fn is_media_channel(ch: &Channel) -> bool {
    ch.forum_tags
        .as_ref()
        .is_some_and(|tags| tags.iter().any(|t| t.emoji.is_some()))
}

// ─────────────────────────────────────────────────────────────────────────────
// DiscordForumView — top-level component
// ─────────────────────────────────────────────────────────────────────────────

/// Discord-style forum post list rendered when a `ChannelType::Forum` channel
/// is opened inside a non-forum-layout server (e.g. Discord, generic backends).
#[ui_action(None)]
#[context_menu(None)]
#[component]
pub fn DiscordForumView() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    // Resolve channel + account from nav/chat_data (same pattern as ForumView).
    let account_id = app_state
        .read()
        .nav
        .active_account_id
        .cloned()
        .unwrap_or_default();
    let channel_id = {
        let s = app_state.read();
        s.nav
            .selected_channel
            .cloned()
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| {
                let cd = chat_data.read();
                cd.current_channel
                    .as_ref()
                    .map(|ch| ch.id.clone())
                    .unwrap_or_default()
            })
    };

    if channel_id.is_empty() || account_id.is_empty() {
        return rsx! {
            div { class: "discord-forum-view-empty", "No channel selected" }
        };
    }

    // Owned signals — sort order, active tag filter set, modal visibility.
    let sort = use_signal(|| ForumSortOrder::LatestActivity);
    let selected_tags: Signal<Vec<String>> = use_signal(Vec::new);
    let show_modal = use_signal(|| false);

    // Fetch forum tags from the channel metadata.
    let forum_tags: Vec<ForumTag> = chat_data
        .read()
        .channels
        .iter()
        .find(|ch| ch.id == channel_id)
        .and_then(|ch| ch.forum_tags.clone())
        .unwrap_or_default();

    let gallery_mode = chat_data
        .read()
        .channels
        .iter()
        .find(|ch| ch.id == channel_id)
        .is_some_and(is_media_channel);

    let channel_name = chat_data
        .read()
        .channels
        .iter()
        .find(|ch| ch.id == channel_id)
        .map(|ch| ch.name.clone())
        .unwrap_or_default();

    // Load forum posts; re-fetches when channel_id, account_id, or sort changes.
    let posts_res = {
        let cid = channel_id.clone();
        let aid = account_id.clone();
        use_resource(move || {
            let cid = cid.clone();
            let aid = aid.clone();
            let cur_sort = *sort.read();
            async move {
                let backend = client_manager.read().get_backend(&aid)?;
                let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("discord_forum_view: backend read timed out");
                        return None;
                    }
                };
                match guard.get_forum_posts(&cid, cur_sort, Some(50)).await {
                    Ok(posts) => Some(posts),
                    Err(err) => {
                        tracing::debug!(
                            "DiscordForumView: get_forum_posts failed: {err:?}"
                        );
                        Some(vec![])
                    }
                }
            }
        })
    };

    // Snapshot of currently loaded posts (empty while loading).
    let all_posts: Vec<ForumPost> = posts_res
        .read_unchecked()
        .as_ref()
        .and_then(|opt| opt.clone())
        .unwrap_or_default();

    let loading = posts_res.read_unchecked().is_none();

    // Client-side tag filter.
    let visible_posts: Vec<ForumPost> = {
        let active = selected_tags.read();
        if active.is_empty() {
            all_posts.clone()
        } else {
            all_posts
                .iter()
                .filter(|p| {
                    p.applied_tags
                        .iter()
                        .any(|tag_id| active.contains(tag_id))
                })
                .cloned()
                .collect()
        }
    };

    // Navigation helpers.
    let backend = app_state
        .read()
        .nav
        .active_backend
        .cloned()
        .map(|b| b.slug().to_string())
        .unwrap_or_else(|| "demo".to_string());
    let instance_id = app_state
        .read()
        .nav
        .active_instance_id
        .cloned()
        .unwrap_or_default();
    let server_id = chat_data
        .read()
        .current_server
        .as_ref()
        .map(|s| s.id.clone())
        .unwrap_or_default();

    rsx! {
        div { class: "discord-forum-view",
            ForumHeader {
                channel_name: channel_name.clone(),
                sort,
                show_modal,
            }
            if !forum_tags.is_empty() {
                ForumTagBar {
                    tags: forum_tags.clone(),
                    selected_tags,
                }
            }
            div { class: "discord-forum-body",
                if loading {
                    div { class: "discord-forum-loading", "Loading posts…" }
                } else if visible_posts.is_empty() {
                    div { class: "discord-forum-empty",
                        "No posts yet. Be the first to post!"
                    }
                } else if gallery_mode {
                    ForumPostGallery {
                        posts: visible_posts.clone(),
                        tags: forum_tags.clone(),
                        backend: backend.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                        server_id: server_id.clone(),
                    }
                } else {
                    ForumPostList {
                        posts: visible_posts.clone(),
                        tags: forum_tags.clone(),
                        backend: backend.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                        server_id: server_id.clone(),
                    }
                }
            }
            if *show_modal.read() {
                NewPostModal {
                    forum_channel_id: channel_id.clone(),
                    account_id: account_id.clone(),
                    tags: forum_tags.clone(),
                    show_modal,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ForumHeader — channel name, sort toggle, New Post button
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(None)]
#[component]
fn ForumHeader(
    channel_name: String,
    sort: Signal<ForumSortOrder>,
    show_modal: Signal<bool>,
) -> Element {
    let cur = *sort.read();
    rsx! {
        div { class: "discord-forum-header",
            div { class: "discord-forum-header-left",
                span { class: "discord-forum-channel-name", "📋 {channel_name}" }
            }
            div { class: "discord-forum-header-right",
                div { class: "discord-forum-sort-group",
                    button {
                        class: if cur == ForumSortOrder::LatestActivity {
                            "discord-forum-sort-btn active"
                        } else {
                            "discord-forum-sort-btn"
                        },
                        onclick: move |_| sort.set(ForumSortOrder::LatestActivity),
                        "Latest Activity"
                    }
                    button {
                        class: if cur == ForumSortOrder::CreationDate {
                            "discord-forum-sort-btn active"
                        } else {
                            "discord-forum-sort-btn"
                        },
                        onclick: move |_| sort.set(ForumSortOrder::CreationDate),
                        "Creation Date"
                    }
                }
                button {
                    class: "discord-forum-new-post-btn",
                    onclick: move |_| show_modal.set(true),
                    "+ New Post"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ForumTagBar — tag pills + clear button
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(None)]
#[component]
fn ForumTagBar(tags: Vec<ForumTag>, selected_tags: Signal<Vec<String>>) -> Element {
    let active = selected_tags.read().clone();
    let has_active = !active.is_empty();
    rsx! {
        div { class: "discord-forum-tag-bar",
            for tag in tags {
                {
                    let tid = tag.id.clone();
                    let is_active = active.contains(&tid);
                    let label = if let Some(ref emoji) = tag.emoji {
                        format!("{emoji} {}", tag.name)
                    } else {
                        tag.name.clone()
                    };
                    rsx! {
                        button {
                            key: "{tid}",
                            class: if is_active { "discord-forum-tag-pill active" } else { "discord-forum-tag-pill" },
                            onclick: move |_| {
                                let mut active = selected_tags.write();
                                if active.contains(&tid) {
                                    active.retain(|id| *id != tid);
                                } else {
                                    active.push(tid.clone());
                                }
                            },
                            "{label}"
                        }
                    }
                }
            }
            if has_active {
                button {
                    class: "discord-forum-tag-clear",
                    onclick: move |_| selected_tags.write().clear(),
                    "× clear"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ForumPostList — list layout
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(None)]
#[component]
fn ForumPostList(
    posts: Vec<ForumPost>,
    tags: Vec<ForumTag>,
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    rsx! {
        div { class: "discord-forum-post-list",
            for post in posts {
                ForumPostRow {
                    key: "{post.thread.thread_id}",
                    post: post.clone(),
                    tags: tags.clone(),
                    backend: backend.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                    server_id: server_id.clone(),
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ForumPostRow — single list row
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(None)]
#[component]
fn ForumPostRow(
    post: ForumPost,
    tags: Vec<ForumTag>,
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    let thread_id = post.thread.thread_id.clone();
    let msg_count = post.thread.message_count;
    let member_count = post.thread.member_count;

    // Resolve tag display objects for applied tags on this post.
    let applied: Vec<&ForumTag> = post
        .applied_tags
        .iter()
        .filter_map(|tid| tags.iter().find(|t| &t.id == tid))
        .collect();

    // Lead emoji from first applied tag.
    let lead_emoji = applied
        .first()
        .and_then(|t| t.emoji.as_deref())
        .unwrap_or("💬");

    // Thread name (= post title) is stored in thread_id until title fetch lands.
    let title = post.thread.thread_id.clone();
    let parent_id = post.thread.parent_channel_id.clone();

    let nav_target = Route::ServerChat {
        backend: backend.clone(),
        instance_id: instance_id.clone(),
        account_id: account_id.clone(),
        server_id: server_id.clone(),
        channel_id: thread_id.clone(),
    };

    rsx! {
        Link {
            class: "discord-forum-post-row",
            to: nav_target,
            span { class: "discord-forum-post-lead-emoji", "{lead_emoji}" }
            div { class: "discord-forum-post-row-body",
                span { class: "discord-forum-post-title", "{title}" }
                div { class: "discord-forum-post-meta",
                    for tag in &applied {
                        {
                            let label = if let Some(ref e) = tag.emoji {
                                format!("{e} {}", tag.name)
                            } else {
                                tag.name.clone()
                            };
                            rsx! {
                                span {
                                    key: "{tag.id}",
                                    class: "discord-forum-post-tag-chip",
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "discord-forum-post-row-stats",
                span { class: "discord-forum-post-stat", title: "Messages",
                    "💬 {msg_count}"
                }
                span { class: "discord-forum-post-stat", title: "Members",
                    "👥 {member_count}"
                }
                // parent_id is the channel backing the post — proxy for last activity.
                span {
                    class: "discord-forum-post-parent-id",
                    style: "display:none",
                    "{parent_id}"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ForumPostGallery — gallery (grid) layout
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(None)]
#[component]
fn ForumPostGallery(
    posts: Vec<ForumPost>,
    tags: Vec<ForumTag>,
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    rsx! {
        div { class: "discord-forum-gallery-grid",
            for post in posts {
                ForumPostCard {
                    key: "{post.thread.thread_id}",
                    post: post.clone(),
                    tags: tags.clone(),
                    backend: backend.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                    server_id: server_id.clone(),
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ForumPostCard — single gallery card
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(None)]
#[component]
fn ForumPostCard(
    post: ForumPost,
    tags: Vec<ForumTag>,
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    let thread_id = post.thread.thread_id.clone();
    let title = post.thread.thread_id.clone();
    let msg_count = post.thread.message_count;

    let applied: Vec<&ForumTag> = post
        .applied_tags
        .iter()
        .filter_map(|tid| tags.iter().find(|t| &t.id == tid))
        .collect();

    let lead_emoji = applied
        .first()
        .and_then(|t| t.emoji.as_deref())
        .unwrap_or("🖼️");

    let nav_target = Route::ServerChat {
        backend: backend.clone(),
        instance_id: instance_id.clone(),
        account_id: account_id.clone(),
        server_id: server_id.clone(),
        channel_id: thread_id.clone(),
    };

    rsx! {
        Link {
            class: "discord-forum-gallery-card",
            to: nav_target,
            div { class: "discord-forum-card-thumbnail",
                span { class: "discord-forum-card-emoji", "{lead_emoji}" }
            }
            div { class: "discord-forum-card-body",
                span { class: "discord-forum-card-title", "{title}" }
                div { class: "discord-forum-card-tags",
                    for tag in &applied {
                        {
                            let label = if let Some(ref e) = tag.emoji {
                                format!("{e} {}", tag.name)
                            } else {
                                tag.name.clone()
                            };
                            rsx! {
                                span {
                                    key: "{tag.id}",
                                    class: "discord-forum-card-tag-chip",
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                span { class: "discord-forum-card-count", "💬 {msg_count}" }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NewPostModal — compose dialog
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(None)]
#[component]
fn NewPostModal(
    forum_channel_id: String,
    account_id: String,
    tags: Vec<ForumTag>,
    show_modal: Signal<bool>,
) -> Element {
    let client_manager: Signal<ClientManager> = use_context();

    let mut title = use_signal(String::new);
    let mut body = use_signal(String::new);
    let mut selected_tag_ids: Signal<Vec<String>> = use_signal(Vec::new);
    let mut submitting = use_signal(|| false);

    let title_empty = title.read().trim().is_empty();

    rsx! {
        div {
            class: "discord-forum-modal-backdrop",
            onclick: move |_| show_modal.set(false),
        }
        div { class: "discord-forum-modal",
            div { class: "discord-forum-modal-header",
                span { class: "discord-forum-modal-title", "New Post" }
                button {
                    class: "discord-forum-modal-close",
                    onclick: move |_| show_modal.set(false),
                    "×"
                }
            }
            div { class: "discord-forum-modal-body",
                label { class: "discord-forum-modal-label", "Title" }
                input {
                    class: "discord-forum-modal-input",
                    r#type: "text",
                    placeholder: "Post title",
                    value: "{title}",
                    oninput: move |e| title.set(e.value()),
                }
                if !tags.is_empty() {
                    label { class: "discord-forum-modal-label", "Tags" }
                    div { class: "discord-forum-modal-tags",
                        for tag in &tags {
                            {
                                let tid = tag.id.clone();
                                let is_active = selected_tag_ids.read().contains(&tid);
                                let label = if let Some(ref e) = tag.emoji {
                                    format!("{e} {}", tag.name)
                                } else {
                                    tag.name.clone()
                                };
                                rsx! {
                                    button {
                                        key: "{tid}",
                                        class: if is_active {
                                            "discord-forum-modal-tag-pill active"
                                        } else {
                                            "discord-forum-modal-tag-pill"
                                        },
                                        onclick: move |_| {
                                            let mut active = selected_tag_ids.write();
                                            if active.contains(&tid) {
                                                active.retain(|id| *id != tid);
                                            } else {
                                                active.push(tid.clone());
                                            }
                                        },
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
                label { class: "discord-forum-modal-label", "Body" }
                textarea {
                    class: "discord-forum-modal-textarea",
                    placeholder: "What's on your mind?",
                    value: "{body}",
                    oninput: move |e| body.set(e.value()),
                }
            }
            div { class: "discord-forum-modal-footer",
                button {
                    class: "discord-forum-modal-cancel",
                    onclick: move |_| show_modal.set(false),
                    "Cancel"
                }
                button {
                    class: "discord-forum-modal-submit",
                    disabled: title_empty || *submitting.read(),
                    onclick: {
                        let cid = forum_channel_id.clone();
                        let aid = account_id.clone();
                        move |_| {
                            if title.read().trim().is_empty() || *submitting.read() {
                                return;
                            }
                            submitting.set(true);
                            let cid = cid.clone();
                            let aid = aid.clone();
                            let post_title = title.read().clone();
                            let post_body = body.read().clone();
                            let post_tags = selected_tag_ids.read().clone();
                            spawn(async move {
                                let backend = client_manager.read().get_backend(&aid);
                                match backend {
                                    None => {
                                        tracing::warn!(
                                            "NewPostModal: no backend for account {aid}"
                                        );
                                    }
                                    Some(handle) => {
                                        let guard = handle.read().await;
                                        match guard
                                            .create_forum_post(
                                                &cid,
                                                &post_title,
                                                &post_body,
                                                post_tags,
                                            )
                                            .await
                                        {
                                            Ok(_new_post) => {
                                                tracing::info!(
                                                    "create_forum_post succeeded for {cid}"
                                                );
                                            }
                                            Err(err) => {
                                                tracing::info!(
                                                    "create_forum_post: {err:?} (expected \
                                                     NotSupported until Phase 5)"
                                                );
                                            }
                                        }
                                    }
                                }
                                submitting.set(false);
                                show_modal.set(false);
                            });
                        }
                    },
                    if *submitting.read() { "Posting…" } else { "Post" }
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use poly_client::ChannelType;

    fn make_channel(tags: Option<Vec<ForumTag>>) -> Channel {
        Channel {
            id: "ch1".to_string(),
            name: "test-forum".to_string(),
            server_id: "s1".to_string(),
            channel_type: ChannelType::Forum,
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: tags,
            parent_channel_id: None,
            thread_metadata: None,
        }
    }

    #[test]
    fn fmt_ts_recent() {
        let ts = Utc::now();
        assert_eq!(fmt_ts(ts), "just now");
    }

    #[test]
    fn is_media_channel_with_emoji_tag() {
        let ch = make_channel(Some(vec![ForumTag {
            id: "1".to_string(),
            name: "art".to_string(),
            emoji: Some("🎨".to_string()),
            moderated: false,
        }]));
        assert!(is_media_channel(&ch));
    }

    #[test]
    fn is_media_channel_without_emoji_tags() {
        let ch = make_channel(Some(vec![ForumTag {
            id: "2".to_string(),
            name: "general".to_string(),
            emoji: None,
            moderated: false,
        }]));
        assert!(!is_media_channel(&ch));
    }

    #[test]
    fn is_media_channel_no_tags() {
        let ch = make_channel(None);
        assert!(!is_media_channel(&ch));
    }
}
