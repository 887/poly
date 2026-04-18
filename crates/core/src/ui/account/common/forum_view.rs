//! Forum view — Lemmy/Reddit-style post list + threaded comment view
//! for `ChannelType::Forum` channels, and Hacker News feed view for
//! `ChannelType::HackerNews` channels.

use crate::client_manager::ClientManager;
use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData};
use crate::ui::client_ui::ClientView;
use crate::ui::context_menu::menus::{forum_post_entry, ForumPostCtx};
use crate::ui::favorites_sidebar::restore_server_channel;
use crate::ui::routes::Route;
use chrono::DateTime;
use dioxus::prelude::*;
use poly_client::{Message, MessageContent, MessageQuery};
use poly_ui_macros::{context_menu, ui_action};

// ─────────────────────────────────────────────────────────────────────────────
// Comment tree node — stores a Message + its recursively resolved children
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct ForumCommentNode {
    msg: Message,
    depth: u32,
    children: Vec<ForumCommentNode>,
}

fn build_comment_tree(post_id: &str, comments: &[Message]) -> Vec<ForumCommentNode> {
    fn children_of(parent_id: &str, all: &[Message], depth: u32) -> Vec<ForumCommentNode> {
        if depth > 8 {
            return vec![];
        }
        all.iter()
            .filter(|m| {
                m.reply_to
                    .as_ref()
                    .is_some_and(|r| r.message_id == parent_id)
            })
            .map(|m| ForumCommentNode {
                children: children_of(&m.id, all, depth + 1),
                msg: m.clone(),
                depth,
            })
            .collect()
    }
    comments
        .iter()
        .filter(|m| {
            m.reply_to.is_none()
                || m.reply_to
                    .as_ref()
                    .is_some_and(|r| r.message_id == post_id)
        })
        .map(|m| ForumCommentNode {
            children: children_of(&m.id, comments, 1),
            msg: m.clone(),
            depth: 0,
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn reaction_count(msg: &Message, emoji: &str) -> u32 {
    msg.reactions
        .iter()
        .find(|r| r.emoji == emoji)
        .map(|r| r.count)
        .unwrap_or(0)
}

fn post_score(msg: &Message) -> i64 {
    let up = reaction_count(msg, "🔥")
        .max(reaction_count(msg, "❤️"))
        .max(reaction_count(msg, "👍"))
        .max(reaction_count(msg, "🎉"))
        .max(reaction_count(msg, "🦀")) as i64;
    let down = reaction_count(msg, "👎") as i64;
    up - down
}

fn post_text(content: &MessageContent) -> &str {
    match content {
        MessageContent::Text(s) => s.as_str(),
        MessageContent::WithAttachments { text, .. } => text.as_str(),
    }
}

fn forum_ts(ts: DateTime<chrono::Utc>) -> String {
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

fn score_class(score: i64) -> &'static str {
    if score > 0 {
        "forum-score positive"
    } else if score < 0 {
        "forum-score negative"
    } else {
        "forum-score"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Top-level ForumView — thin wrapper over `ClientView`. The plugin declares
// its own view-descriptor (list / card-grid / tree / split); the host engine
// renders it. Legacy HN/Lemmy-specific rendering is gone (plan WP 5).
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(None)]
#[context_menu(None)]
#[component]
pub fn ForumView() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    // Channel id resolution (fixes back-button + server-switch bugs):
    //   1. Prefer `nav.selected_channel` (set by sync_route_to_app_state on
    //      ServerChat routes).
    //   2. Fall back to `chat_data.current_channel.id` (set by
    //      load_server_data after click nav).
    //   3. Finally, pick the first channel in the loaded channels list —
    //      handles ServerHome route which intentionally leaves
    //      selected_channel = None, and also handles the 'navigate back from
    //      ForumPostRoute' flow where nav.selected_channel may be stale until
    //      load_server_data resolves.
    let account_id = app_state
        .read()
        .nav
        .active_account_id
        .clone()
        .unwrap_or_default();
    let channel_id = {
        let s = app_state.read();
        if let Some(id) = s.nav.selected_channel.clone() {
            if !id.is_empty() {
                id
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };
    let channel_id = if channel_id.is_empty() {
        let cd = chat_data.read();
        // Fall back to current_channel only if it's actually in the CURRENT
        // server's channel list — after switching servers, `current_channel`
        // can lag the `channels` vec for a tick. Taking a stale
        // current_channel here leaks the previous server's posts into the new
        // server's forum view.
        let current_matches_server = cd
            .current_channel
            .as_ref()
            .is_some_and(|ch| cd.channels.iter().any(|c| c.id == ch.id));
        if current_matches_server {
            cd.current_channel
                .as_ref()
                .map(|ch| ch.id.clone())
                .or_else(|| cd.channels.first().map(|ch| ch.id.clone()))
                .unwrap_or_default()
        } else {
            cd.channels
                .first()
                .map(|ch| ch.id.clone())
                .unwrap_or_default()
        }
    } else {
        channel_id
    };
    if channel_id.is_empty() || account_id.is_empty() {
        return rsx! {
            div { class: "forum-view-missing-context",
                "No channel selected"
            }
        };
    }
    // Key forces a full remount on channel change so use_resource inside
    // ClientView (and its body engines) picks up the new channel_id.
    // Without this, switching servers keeps showing the previous server's
    // posts because use_resource holds a captured String that Dioxus
    // can't track reactively.
    let key = format!("{}:{}", channel_id, account_id);
    rsx! {
        ClientView { key: "{key}", channel_id, account_id }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ForumPostView — route component: load + render single post + comments
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(None)]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn ForumPostView(channel_id: String, post_id: String) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let mut thread_comments: Signal<Vec<Message>> = use_signal(Vec::new);
    let mut thread_loading: Signal<bool> = use_signal(|| true);

    // Load channel data + comments on mount / when post_id changes.
    let post_id_clone = post_id.clone();
    let channel_id_clone = channel_id.clone();
    use_effect(move || {
        let pid = post_id_clone.clone();
        let cid = channel_id_clone.clone();

        // Ensure the server+channel context is loaded (handles direct URL navigation).
        let server_id = app_state.read().nav.selected_server.clone().unwrap_or_default();

        let account_id = app_state.read().nav.active_account_id.clone();
        let backend = account_id.as_deref()
            .and_then(|aid| client_manager.read().get_backend(aid));

        // Check if channel data is already loaded for this channel.
        let already_loaded = {
            let snap = chat_data.read();
            snap.current_channel.as_ref().is_some_and(|ch| ch.id == cid)
                && snap.current_server.as_ref().is_some_and(|s| s.id == server_id)
        };

        if !already_loaded {
            let app_state2 = app_state;
            let client_manager2 = client_manager;
            let chat_data2 = chat_data;
            let backend2 = backend.clone();
            let pid2 = pid.clone();
            spawn(async move {
                restore_server_channel(
                    server_id,
                    cid,
                    app_state2,
                    client_manager2,
                    chat_data2,
                )
                .await;
                // After channel loaded, fetch comments.
                if let Some(ref b) = backend2 {
                    let b = b.clone();
                    let comment_channel = format!("hn-post-{pid2}");
                    let result = b.read().await
                        .get_messages(&comment_channel, MessageQuery { limit: Some(200), ..Default::default() })
                        .await
                        .unwrap_or_default();
                    thread_comments.set(result);
                }
                thread_loading.set(false);
            });
        } else {
            // Channel already loaded — just fetch comments.
            if let Some(ref b) = backend {
                let b = b.clone();
                spawn(async move {
                    let comment_channel = format!("hn-post-{pid}");
                    let result = b.read().await
                        .get_messages(&comment_channel, MessageQuery { limit: Some(200), ..Default::default() })
                        .await
                        .unwrap_or_default();
                    thread_comments.set(result);
                    thread_loading.set(false);
                });
            } else {
                thread_loading.set(false);
            }
        }
    });

    // Find the post in the currently loaded messages.
    let post = chat_data.read().messages.iter()
        .find(|m| m.id == post_id)
        .cloned();

    match post {
        None => rsx! {
            div { class: "forum-post-loading",
                if *thread_loading.read() {
                    span { "Loading post…" }
                } else {
                    span { "Post not found." }
                }
            }
        },
        Some(p) => rsx! {
            ForumThreadView {
                post: p,
                comments: thread_comments.read().clone(),
                loading: *thread_loading.read(),
            }
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Post card
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(crate::ui::context_menu::menus::ForumPostContextMenu)]
#[rustfmt::skip]
#[component]
fn ForumPostCard(post: Message, on_click: EventHandler<()>) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let score = post_score(&post);
    let comments_count = reaction_count(&post, "💬");
    let author_name = post.author.display_name.clone();
    let author_initial = author_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar_url = post.author.avatar_url.clone();
    let author_color = user_color(&post.author.id);
    let time_str = forum_ts(post.timestamp);
    let text = post_text(&post.content).to_string();
    let sc = score_class(score);

    let ctx_post_id = post.id.clone();
    let ctx_author_id = post.author.id.clone();
    let ctx_author_name = post.author.display_name.clone();
    let ctx_text = text.clone();

    rsx! {
        div {
            class: "forum-post-card",
            onclick: move |_| on_click.call(()),
            oncontextmenu: move |evt| {
                evt.prevent_default();
                evt.stop_propagation();
                let ctx = ForumPostCtx {
                    post_id: ctx_post_id.clone(),
                    author_id: ctx_author_id.clone(),
                    author_name: ctx_author_name.clone(),
                    text: ctx_text.clone(),
                };
                let entry = forum_post_entry(ctx, &evt);
                app_state.write().context_menu_stack.push(entry);
            },
            div { class: "forum-post-votes",
                button { class: "forum-vote-btn up", title: "Upvote",
                    onclick: |e: MouseEvent| e.stop_propagation(),
                    "▲"
                }
                span { class: "{sc}", "{score}" }
                button { class: "forum-vote-btn down", title: "Downvote",
                    onclick: |e: MouseEvent| e.stop_propagation(),
                    "▼"
                }
            }
            div { class: "forum-post-body",
                div { class: "forum-post-author-row",
                    if let Some(ref url) = avatar_url {
                        img { class: "forum-post-avatar", src: "{url}", alt: "{author_name}" }
                    } else {
                        div {
                            class: "forum-post-avatar forum-post-avatar-initial",
                            style: "background:{author_color}",
                            "{author_initial}"
                        }
                    }
                    span { class: "forum-post-author-name", "{author_name}" }
                    span { class: "forum-post-time", "· {time_str}" }
                }
                p { class: "forum-post-content", "{text}" }
                div { class: "forum-post-footer",
                    span { class: "forum-post-comments", "💬 {comments_count} comments" }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Thread view
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(None)]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
fn ForumThreadView(post: Message, comments: Vec<Message>, loading: bool) -> Element {
    let score = post_score(&post);
    let sc = score_class(score);
    let author_name = post.author.display_name.clone();
    let author_initial = author_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar_url = post.author.avatar_url.clone();
    let author_color = user_color(&post.author.id);
    let time_str = forum_ts(post.timestamp);
    let text = post_text(&post.content).to_string();
    let n = comments.len();
    let count_label = if loading {
        "Loading comments…".to_string()
    } else if n == 0 {
        "No comments yet".to_string()
    } else if n == 1 {
        "1 comment".to_string()
    } else {
        format!("{n} comments")
    };
    let tree = build_comment_tree(&post.id, &comments);

    rsx! {
        div { class: "forum-thread-view",
            // Original post
            div { class: "forum-thread-post",
                div { class: "forum-post-votes",
                    button { class: "forum-vote-btn up", "▲" }
                    span { class: "{sc}", "{score}" }
                    button { class: "forum-vote-btn down", "▼" }
                }
                div { class: "forum-post-body",
                    div { class: "forum-post-author-row",
                        if let Some(ref url) = avatar_url {
                            img { class: "forum-post-avatar", src: "{url}", alt: "{author_name}" }
                        } else {
                            div {
                                class: "forum-post-avatar forum-post-avatar-initial",
                                style: "background:{author_color}",
                                "{author_initial}"
                            }
                        }
                        span { class: "forum-post-author-name", "{author_name}" }
                        span { class: "forum-post-time", "· {time_str}" }
                    }
                    p { class: "forum-thread-post-content", "{text}" }
                }
            }
            // Comment count header
            div { class: "forum-comments-header",
                span { class: "forum-comments-count", "{count_label}" }
            }
            // Comment tree
            div { class: "forum-comment-list",
                for node in tree {
                    ForumComment { node: node.clone() }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Recursive comment component (named ForumComment to avoid struct/component clash)
// ─────────────────────────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(crate::ui::context_menu::menus::ForumPostContextMenu)]
#[rustfmt::skip]
#[component]
fn ForumComment(node: ForumCommentNode) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let msg = &node.msg;
    let depth = node.depth;
    let children = node.children.clone();

    let mut collapsed = use_signal(|| false);

    let score = post_score(msg);
    let sc = score_class(score);
    let show_score = msg.author.backend.slug() != "hackernews" && score != 0;
    let author_name = msg.author.display_name.clone();
    let author_initial = author_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar_url = msg.author.avatar_url.clone();
    let author_color = user_color(&msg.author.id);
    let time_str = forum_ts(msg.timestamp);
    let text = post_text(&msg.content).to_string();
    let score_label = format!("{score:+}");

    let ctx_post_id = msg.id.clone();
    let ctx_author_id = msg.author.id.clone();
    let ctx_author_name = msg.author.display_name.clone();
    let ctx_text = text.clone();

    let indent_px = (depth.min(4) * 20) as i32;
    let border_color = match depth % 4 {
        0 => "#60a5fa",
        1 => "#4ade80",
        2 => "#fbbf24",
        _ => "#a78bfa",
    };

    // Count total descendants for the collapsed summary.
    fn count_descendants(nodes: &[ForumCommentNode]) -> usize {
        nodes.iter().fold(0, |acc, n| acc + 1 + count_descendants(&n.children))
    }
    let descendant_count = count_descendants(&children);
    let is_collapsed = *collapsed.read();
    let toggle_label = if is_collapsed { "[+]" } else { "[-]" };
    let collapsed_hint = if is_collapsed && descendant_count > 0 {
        format!(" ({descendant_count} hidden)")
    } else {
        String::new()
    };

    rsx! {
        div {
            class: "forum-comment",
            style: "margin-left: {indent_px}px; border-left: 2px solid {border_color};",
            oncontextmenu: move |evt| {
                evt.prevent_default();
                evt.stop_propagation();
                let ctx = ForumPostCtx {
                    post_id: ctx_post_id.clone(),
                    author_id: ctx_author_id.clone(),
                    author_name: ctx_author_name.clone(),
                    text: ctx_text.clone(),
                };
                let entry = forum_post_entry(ctx, &evt);
                app_state.write().context_menu_stack.push(entry);
            },
            div { class: "forum-comment-header",
                button {
                    class: "forum-comment-collapse",
                    onclick: move |_| collapsed.set(!is_collapsed),
                    "{toggle_label}"
                }
                if let Some(ref url) = avatar_url {
                    img { class: "forum-comment-avatar", src: "{url}", alt: "{author_name}" }
                } else {
                    div {
                        class: "forum-comment-avatar forum-comment-avatar-initial",
                        style: "background:{author_color}",
                        "{author_initial}"
                    }
                }
                span { class: "forum-comment-author", "{author_name}" }
                span { class: "forum-comment-time", "· {time_str}" }
                if show_score {
                    span { class: "{sc} forum-comment-score", "{score_label}" }
                }
                if !collapsed_hint.is_empty() {
                    span { class: "forum-comment-collapsed-hint", "{collapsed_hint}" }
                }
            }
            if !is_collapsed {
                p { class: "forum-comment-body", "{text}" }
                for child in children {
                    ForumComment { node: child.clone() }
                }
            }
        }
    }
}
