//! Forum view — Lemmy/Reddit-style post list + threaded comment view
//! for `ChannelType::Forum` channels.

use crate::client_manager::ClientManager;
use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData};
use chrono::DateTime;
use dioxus::prelude::*;
use poly_client::{Message, MessageContent, MessageQuery};

// ─────────────────────────────────────────────────────────────────────────────
// Sort
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ForumSort {
    #[default]
    Hot,
    Top,
    New,
    Old,
}

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
        .max(reaction_count(msg, "👍")) as i64;
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
// Top-level ForumView
// ─────────────────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[component]
pub fn ForumView() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let mut selected_post: Signal<Option<Message>> = use_signal(|| None);
    let mut thread_comments: Signal<Vec<Message>> = use_signal(Vec::new);
    let mut thread_loading: Signal<bool> = use_signal(|| false);
    let mut sort = use_signal(|| ForumSort::Hot);

    let snapshot = chat_data.read();
    let current_channel = snapshot.current_channel.clone();
    let current_server = snapshot.current_server.clone();
    let posts = snapshot.messages.clone();
    drop(snapshot);

    let channel_name = current_channel.as_ref().map(|ch| ch.name.clone()).unwrap_or_default();
    let server_name = current_server
        .as_ref()
        .map(|s| format!("{} {}", backend_badge(&s.backend), s.backend.display_name()))
        .unwrap_or_default();

    let mut sorted_posts = posts.clone();
    match *sort.read() {
        ForumSort::Hot | ForumSort::Top => sorted_posts.sort_by(|a, b| post_score(b).cmp(&post_score(a))),
        ForumSort::New => sorted_posts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)),
        ForumSort::Old => sorted_posts.sort_by(|a, b| a.timestamp.cmp(&b.timestamp)),
    }

    let current_sort = *sort.read();
    let in_thread = selected_post.read().is_some();

    rsx! {
        div { class: "forum-view",
            // Header
            div { class: "forum-header",
                div { class: "forum-header-info",
                    if in_thread {
                        button {
                            class: "forum-back-btn",
                            onclick: move |_| {
                                selected_post.set(None);
                                thread_comments.set(vec![]);
                            },
                            "← Back"
                        }
                        if let Some(ref post) = *selected_post.read() {
                            span { class: "forum-thread-title",
                                "{post_text(&post.content).chars().take(60).collect::<String>()}…"
                            }
                        }
                    } else {
                        span { class: "forum-channel-name", "📋 {channel_name}" }
                        if !server_name.is_empty() {
                            span { class: "chat-source-badge", "{server_name}" }
                        }
                    }
                }
                if !in_thread {
                    div { class: "forum-sort-tabs",
                        button {
                            class: if current_sort == ForumSort::Hot { "forum-sort-tab active" } else { "forum-sort-tab" },
                            onclick: move |_| sort.set(ForumSort::Hot),
                            "🔥 Hot"
                        }
                        button {
                            class: if current_sort == ForumSort::Top { "forum-sort-tab active" } else { "forum-sort-tab" },
                            onclick: move |_| sort.set(ForumSort::Top),
                            "↑ Top"
                        }
                        button {
                            class: if current_sort == ForumSort::New { "forum-sort-tab active" } else { "forum-sort-tab" },
                            onclick: move |_| sort.set(ForumSort::New),
                            "✨ New"
                        }
                        button {
                            class: if current_sort == ForumSort::Old { "forum-sort-tab active" } else { "forum-sort-tab" },
                            onclick: move |_| sort.set(ForumSort::Old),
                            "📅 Old"
                        }
                    }
                }
            }

            // Content
            if in_thread {
                if let Some(post) = selected_post.read().clone() {
                    ForumThreadView {
                        post: post.clone(),
                        comments: thread_comments.read().clone(),
                        loading: *thread_loading.read(),
                    }
                }
            } else {
                div { class: "forum-post-list",
                    if sorted_posts.is_empty() {
                        div { class: "forum-empty",
                            div { class: "forum-empty-icon", "📋" }
                            p { "No posts yet." }
                        }
                    }
                    for post in sorted_posts {
                        {
                            let post2 = post.clone();
                            let post_id = post.id.clone();
                            let account_id = app_state.read().nav.active_account_id.clone();
                            let backend = account_id.as_deref()
                                .and_then(|aid| client_manager.read().get_backend(aid));
                            rsx! {
                                ForumPostCard {
                                    key: "{post_id}",
                                    post: post2.clone(),
                                    on_click: move |_| {
                                        selected_post.set(Some(post2.clone()));
                                        thread_comments.set(vec![]);
                                        thread_loading.set(true);
                                        let pid = post_id.clone();
                                        if let Some(ref b) = backend {
                                            let b = b.clone();
                                            spawn(async move {
                                                let guard = b.read().await;
                                                let result = guard
                                                    .get_messages(&pid, MessageQuery::default())
                                                    .await
                                                    .unwrap_or_default();
                                                thread_comments.set(result);
                                                thread_loading.set(false);
                                            });
                                        } else {
                                            thread_loading.set(false);
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Post card
// ─────────────────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[component]
fn ForumPostCard(post: Message, on_click: EventHandler<()>) -> Element {
    let score = post_score(&post);
    let comments_count = reaction_count(&post, "💬");
    let author_name = post.author.display_name.clone();
    let author_initial = author_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar_url = post.author.avatar_url.clone();
    let author_color = user_color(&post.author.id);
    let time_str = forum_ts(post.timestamp);
    let text = post_text(&post.content).to_string();
    let sc = score_class(score);

    rsx! {
        div {
            class: "forum-post-card",
            onclick: move |_| on_click.call(()),
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

#[rustfmt::skip]
#[component]
fn ForumComment(node: ForumCommentNode) -> Element {
    let msg = &node.msg;
    let depth = node.depth;
    let children = node.children.clone();

    let score = post_score(msg);
    let sc = score_class(score);
    let author_name = msg.author.display_name.clone();
    let author_initial = author_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar_url = msg.author.avatar_url.clone();
    let author_color = user_color(&msg.author.id);
    let time_str = forum_ts(msg.timestamp);
    let text = post_text(&msg.content).to_string();
    let score_label = format!("{score:+}");

    let indent_px = (depth.min(4) * 20) as i32;
    let border_color = match depth % 4 {
        0 => "#60a5fa",
        1 => "#4ade80",
        2 => "#fbbf24",
        _ => "#a78bfa",
    };

    rsx! {
        div {
            class: "forum-comment",
            style: "margin-left: {indent_px}px; border-left: 2px solid {border_color};",
            div { class: "forum-comment-header",
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
                span { class: "{sc} forum-comment-score", "{score_label}" }
            }
            p { class: "forum-comment-body", "{text}" }
            for child in children {
                ForumComment { node: child.clone() }
            }
        }
    }
}
