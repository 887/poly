//! Conversions from `crate::parser::*` raw types to `poly_client::*` UI
//! types, plus the HTML sanitisers / sort-key codecs shared by both
//! `IsBackend` and `ViewDescriptorBackend` impls.
//!
//! Carved out in SOLID-audit-reddit C.3.

use super::ids::{
    channel_id_for_sub, dm_channel_id_for_dm, message_id_for_dm, message_id_for_post,
    server_id_for_sub, user_id_for_name,
};
use crate::parser::{RawComment, RawDm, RawPost, UserProfile};
use crate::SortKind;
use poly_client::*;

/// Strip HTML tags + decode common entities from a reddit comment body.
///
/// Reddit's parser emits `body_html` already converted from markdown, but
/// `MessageContent::Text` is rendered as plain text by the chat view (no
/// HTML interpretation). This conversion gives readable text — paragraphs
/// joined with newlines, lists flattened, links shown as link text only
/// (URLs lost). Lossy but the right floor for the existing chat-view.
///
/// Future improvement: round-trip HTML → markdown so the chat-view's
/// markdown renderer can lay out lists / links / code blocks properly.
pub(crate) fn html_to_plain_text(html: &str) -> String {
    // Replace block-level closing tags with double-newline so paragraphs
    // and list items separate visually.
    let mut s = html.to_string();
    for close in ["</p>", "</li>", "</div>", "</blockquote>", "<br>", "<br/>", "<br />"] {
        s = s.replace(close, "\n\n");
    }
    // Strip remaining tags via a tiny state-machine.
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    // Decode the common HTML entities reddit emits.
    out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    // Collapse runs of 3+ newlines down to 2.
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

/// Walk a comment tree depth-first and push each comment as a flat
/// Message into the output Vec. Used by `get_messages` for the
/// per-post comment-fetch route (`hn-post-<pid>`) so ForumPostView
/// can render the thread as a flat list (Message-level reply_to
/// threading is a separate, future pass).
pub(crate) fn flatten_comments_into_messages(
    comments: &[RawComment],
    backend: &BackendType,
    out: &mut Vec<Message>,
) {
    for c in comments {
        out.push(Message {
            id: format!("t1_{}", c.id),
            author: User {
                id: user_id_for_name(&c.author),
                display_name: c.author.clone(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: backend.clone(),
            },
            // body_html is reddit's pre-rendered HTML; the chat-view
            // renders MessageContent::Text as plain text (no HTML
            // interpretation), so strip tags + decode entities first.
            content: MessageContent::Text(html_to_plain_text(&c.body_html)),
            timestamp: c.timestamp,
            attachments: Vec::new(),
            reactions: Vec::new(),
            reply_to: None,
            edited: false,
            thread: None,
            preview_image_url: None,
        });
        if !c.replies.is_empty() {
            flatten_comments_into_messages(&c.replies, backend, out);
        }
    }
}

/// Recursively emit reddit comments as depth-indented sanitized HTML.
/// Used by `get_view_detail` to inline the comment thread under the
/// post body (TreeSpec-via-ViewRow doesn't support hierarchy yet).
pub(crate) fn render_comments_to_html(out: &mut String, comments: &[RawComment], depth: u32, max_depth: u32) {
    fn html_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    let indent_px = depth.min(max_depth).saturating_mul(16);
    for comment in comments {
        out.push_str(&format!(
            "<div class=\"reddit-comment\" style=\"margin-left:{indent_px}px\">"
        ));
        out.push_str(&format!(
            "<div class=\"reddit-comment-meta\">u/{} · {} points</div>",
            html_escape(&comment.author),
            comment.score,
        ));
        // Body is already HTML-rendered by the parser (markdown → HTML by
        // reddit), so include verbatim — host's CustomBlock sanitizer
        // strips dangerous tags downstream.
        out.push_str(&format!(
            "<div class=\"reddit-comment-body\">{}</div>",
            comment.body_html,
        ));
        out.push_str("</div>");
        if depth < max_depth && !comment.replies.is_empty() {
            render_comments_to_html(out, &comment.replies, depth.saturating_add(1), max_depth);
        }
    }
}

/// Split a free-form composer string into `(title, body)` for
/// `/api/submit`. The first non-empty line becomes the title; everything
/// after the first newline is the body (verbatim — leading blank lines
/// trimmed).
pub(crate) fn split_title_body(text: &str) -> (String, &str) {
    let trimmed = text.trim_start_matches('\n');
    if let Some(idx) = trimmed.find('\n') {
        let (title, rest) = trimmed.split_at(idx);
        (title.trim().to_string(), rest.trim_start_matches('\n'))
    } else {
        (trimmed.trim().to_string(), "")
    }
}

pub(crate) fn raw_post_to_message(post: &RawPost, backend: &BackendType) -> Message {
    let content = if let Some(body) = &post.body {
        let body_text = html_to_plain_text(body);
        if !body_text.is_empty() {
            MessageContent::Text(format!("{}\n\n{}", post.title, body_text))
        } else {
            MessageContent::Text(post.title.clone())
        }
    } else if let Some(url) = &post.url {
        MessageContent::Text(format!("{}\n\n{}", post.title, url))
    } else {
        MessageContent::Text(post.title.clone())
    };

    // Add an attachment for image previews so the message view can render them.
    // For video posts we use the preview thumbnail (if available) and mark the
    // attachment content-type as video/mp4 as a hint. Galleries get a single
    // cover-image attachment.
    let mut attachments = Vec::new();
    if let Some(ref preview) = post.preview_url {
        let (content_type, filename) = if post.is_video {
            ("video/mp4", "video_preview.jpg")
        } else {
            ("image/png", "preview.png")
        };
        attachments.push(Attachment::remote(
            format!("reddit-preview-{}", post.id),
            filename.to_string(),
            content_type.to_string(),
            preview.clone(),
            0,
        ));
    }

    Message {
        id: message_id_for_post(&post.id),
        author: User {
            id: user_id_for_name(&post.author),
            display_name: post.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        content,
        timestamp: post.timestamp,
        attachments,
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
        thread: None,
        preview_image_url: post.preview_url.clone(),
    }
}

pub(crate) fn raw_dm_to_dm_channel(dm: &RawDm, account_id: &str, backend: &BackendType) -> DmChannel {
    let last_message = Message {
        id: message_id_for_dm(&dm.id),
        author: User {
            id: user_id_for_name(&dm.author),
            display_name: dm.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        content: MessageContent::Text(dm.subject.clone()),
        timestamp: dm.timestamp,
        attachments: Vec::new(),
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
        thread: None,
        preview_image_url: None,
    };

    DmChannel {
        id: dm_channel_id_for_dm(&dm.id),
        user: User {
            id: user_id_for_name(&dm.author),
            display_name: dm.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        last_message: Some(last_message),
        unread_count: 0,
        backend: backend.clone(),
        account_id: account_id.to_string(),
    }
}

pub(crate) fn user_profile_to_user(profile: &UserProfile, backend: &BackendType) -> User {
    User {
        id: user_id_for_name(&profile.name),
        display_name: profile.name.clone(),
        avatar_url: profile.avatar_url.clone(),
        presence: PresenceStatus::Offline,
        backend: backend.clone(),
    }
}

pub(crate) fn raw_post_to_viewrow(post: &RawPost, show_previews: bool) -> ViewRow {
    let secondary = format!("by u/{}", post.author);
    let preview_image_url = if show_previews { post.preview_url.clone() } else { None };

    ViewRow {
        id: message_id_for_post(&post.id),
        primary_text: post.title.clone(),
        secondary_text: Some(secondary),
        // SCORE: prefix is load-bearing for the forum-post-card render path in
        // list_body.rs — ListBodyRow renders the vote-card shape when it appears.
        meta_text: Some(format!("SCORE:{} · {} comments", post.score, post.comment_count)),
        icon: None,
        badge: None,
        context_menu_target_kind: MenuTargetKind::Message,
        preview_image_url,
        is_video: post.is_video,
    }
}

pub(crate) fn build_sub_server(
    sub: &str,
    account_id: &str,
    account_display_name: &str,
    backend: &BackendType,
) -> Server {
    Server {
        id: server_id_for_sub(sub),
        name: format!("r/{sub}"),
        icon_url: None,
        banner_url: None,
        categories: vec![Category {
            id: format!("cat_{sub}"),
            name: "Channels".to_string(),
            channel_ids: vec![channel_id_for_sub(sub)],
        }],
        backend: backend.clone(),
        unread_count: 0,
        mention_count: 0,
        account_id: account_id.to_string(),
        account_display_name: account_display_name.to_string(),
        default_channel_id: Some(channel_id_for_sub(sub)),
        description: None,
        star_count: None,
        language: None,
        forks_count: None,
        open_issues_count: None,
    }
}

pub(crate) fn build_sub_channel(sub: &str) -> Channel {
    Channel {
        id: channel_id_for_sub(sub),
        name: "posts".to_string(),
        channel_type: ChannelType::Forum,
        server_id: server_id_for_sub(sub),
        unread_count: 0,
        mention_count: 0,
        last_message_id: None,
        forum_tags: None,
        parent_channel_id: None,
        thread_metadata: None,
    }
}

// ─── Sort state helpers ───────────────────────────────────────────────────────

/// Stable string key used to persist a `SortKind` in `settings_storage`.
pub(crate) fn sort_kind_to_str(sort: SortKind) -> &'static str {
    match sort {
        SortKind::Hot => "hot",
        SortKind::New => "new",
        SortKind::Rising => "rising",
        SortKind::Controversial => "controversial",
        SortKind::Top => "top",
        SortKind::TopHour => "top-hour",
        SortKind::TopDay => "top-day",
        SortKind::TopWeek => "top-week",
        SortKind::TopMonth => "top-month",
        SortKind::TopYear => "top-year",
        SortKind::TopAll => "top-all",
    }
}

/// Parse a persisted sort key back into a `SortKind`.
///
/// Returns `SortKind::Hot` for unrecognised or absent values (safe default).
pub(crate) fn sort_kind_from_str(s: &str) -> SortKind {
    match s {
        "hot" => SortKind::Hot,
        "new" => SortKind::New,
        "rising" => SortKind::Rising,
        "controversial" => SortKind::Controversial,
        "top" => SortKind::Top,
        "top-hour" => SortKind::TopHour,
        "top-day" => SortKind::TopDay,
        "top-week" => SortKind::TopWeek,
        "top-month" => SortKind::TopMonth,
        "top-year" => SortKind::TopYear,
        "top-all" => SortKind::TopAll,
        _ => SortKind::Hot,
    }
}
