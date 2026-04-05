//! Forum view — Lemmy/Reddit-style post list for `ChannelType::Forum` channels.
//!
//! Replaces `ChatView` when the active channel is a Forum channel.
//! Shows posts as cards with vote counts, author, timestamp, and content.
//! No member sidebar and no message input.

use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData};
use chrono::DateTime;
use dioxus::prelude::*;
use poly_client::{Message, MessageContent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ForumSort {
    #[default]
    Hot,
    Top,
    New,
    Old,
}

/// Extract a reaction count by emoji from a message.
fn reaction_count(msg: &Message, emoji: &str) -> u32 {
    msg.reactions
        .iter()
        .find(|r| r.emoji == emoji)
        .map(|r| r.count)
        .unwrap_or(0)
}

/// Format a timestamp relative to now (e.g. "3 hours ago").
fn forum_timestamp(ts: DateTime<chrono::Utc>) -> String {
    let local = ts.with_timezone(&chrono::Local);
    let now = chrono::Local::now();
    let diff = now.signed_duration_since(local);

    let secs = diff.num_seconds();
    if secs < 60 {
        return "just now".to_string();
    }
    let mins = diff.num_minutes();
    if mins < 60 {
        return format!("{mins} minute{} ago", if mins == 1 { "" } else { "s" });
    }
    let hours = diff.num_hours();
    if hours < 24 {
        return format!("{hours} hour{} ago", if hours == 1 { "" } else { "s" });
    }
    let days = diff.num_days();
    if days < 7 {
        return format!("{days} day{} ago", if days == 1 { "" } else { "s" });
    }
    local.format("%b %-d, %Y").to_string()
}

/// Extract text content from a MessageContent.
fn post_text(content: &MessageContent) -> &str {
    match content {
        MessageContent::Text(s) => s.as_str(),
        MessageContent::WithAttachments { text, .. } => text.as_str(),
    }
}

#[rustfmt::skip]
#[component]
pub fn ForumView() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let _app_state: Signal<AppState> = use_context();

    let mut sort = use_signal(|| ForumSort::Hot);

    let snapshot = chat_data.read();
    let current_channel = snapshot.current_channel.clone();
    let current_server = snapshot.current_server.clone();
    let posts = snapshot.messages.clone();
    drop(snapshot);

    let channel_name = current_channel
        .as_ref()
        .map(|ch| ch.name.clone())
        .unwrap_or_default();
    let server_name = current_server
        .as_ref()
        .map(|s| format!("{} {}", backend_badge(&s.backend), s.backend.display_name()))
        .unwrap_or_default();

    // Sort posts client-side based on selected tab.
    let mut sorted = posts.clone();
    match *sort.read() {
        ForumSort::Hot | ForumSort::Top => {
            sorted.sort_by(|a, b| {
                let va = reaction_count(a, "🔥").saturating_add(reaction_count(a, "❤️"));
                let vb = reaction_count(b, "🔥").saturating_add(reaction_count(b, "❤️"));
                vb.cmp(&va)
            });
        }
        ForumSort::New => {
            sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        }
        ForumSort::Old => {
            sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        }
    }

    let current_sort = *sort.read();

    rsx! {
        div { class: "forum-view",
            // Header: channel name + sort tabs
            div { class: "forum-header",
                div { class: "forum-header-info",
                    span { class: "forum-channel-name", "📋 {channel_name}" }
                    if !server_name.is_empty() {
                        span { class: "chat-source-badge", "{server_name}" }
                    }
                }
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

            // Post list
            div { class: "forum-post-list",
                if sorted.is_empty() {
                    div { class: "forum-empty",
                        div { class: "forum-empty-icon", "📋" }
                        p { "No posts yet." }
                    }
                }
                for post in sorted {
                    ForumPostCard { post: post.clone() }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn ForumPostCard(post: Message) -> Element {
    let upvotes = reaction_count(&post, "🔥").max(reaction_count(&post, "❤️"));
    let downvotes = reaction_count(&post, "👎");
    let comments = reaction_count(&post, "💬");
    let score: i64 = upvotes as i64 - downvotes as i64;

    let author_name = post.author.display_name.clone();
    let author_initial = author_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar_url = post.author.avatar_url.clone();
    let author_color = user_color(&post.author.id);
    let time_str = forum_timestamp(post.timestamp);
    let text = post_text(&post.content).to_string();

    let score_class = if score > 0 {
        "forum-score positive"
    } else if score < 0 {
        "forum-score negative"
    } else {
        "forum-score"
    };

    rsx! {
        div { class: "forum-post-card",
            // Vote column
            div { class: "forum-post-votes",
                button { class: "forum-vote-btn up", title: "Upvote", "▲" }
                span { class: "{score_class}", "{score}" }
                button { class: "forum-vote-btn down", title: "Downvote", "▼" }
            }
            // Post body
            div { class: "forum-post-body",
                div { class: "forum-post-author-row",
                    if let Some(ref url) = avatar_url {
                        img {
                            class: "forum-post-avatar",
                            src: "{url}",
                            alt: "{author_name}",
                        }
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
                    if comments > 0 {
                        span { class: "forum-post-comments", "💬 {comments} comments" }
                    }
                    if downvotes > 0 {
                        span { class: "forum-post-downvotes", "👎 {downvotes}" }
                    }
                }
            }
        }
    }
}
