//! Pure mapping helpers — Lemmy API types → Poly `poly_client` types.
//!
//! No I/O. Tests at the bottom of this module exercise the mappers
//! against checked-in fixtures.

use chrono::{DateTime, Utc};
use poly_client::{
    Attachment, BackendType, Category, Channel, ChannelType, Cursor, CursorKind, DmChannel,
    MenuTargetKind, Message, MessageContent, PresenceStatus, Reaction, Server, User, ViewRow,
};

use super::types::{
    CommentView, CommunityView, LemmyCommunity, LemmyPerson, LemmyPost, PostView,
    PrivateMessageView,
};

/// Determine whether a `LemmyPost` links to a video source.
///
/// Returns `true` when `embed_video_url` is populated (the canonical Lemmy
/// signal for video embeds) OR when `post.url` uses a recognised video
/// file extension OR a known video-host domain (YouTube / Vimeo /
/// well-known PeerTube flagship instances). PeerTube federation is
/// per-instance, so the host list is best-effort — instances missing
/// from the list still get the right signal via `embed_video_url`,
/// which is always trusted.
pub fn post_is_video(post: &LemmyPost) -> bool {
    if post.embed_video_url.is_some() {
        return true;
    }
    post.url.as_deref().is_some_and(|u| {
        let lower = u.to_lowercase();
        // File-extension match.
        if lower.ends_with(".mp4")
            || lower.ends_with(".webm")
            || lower.ends_with(".ogv")
            || lower.ends_with(".mov")
        {
            return true;
        }
        // Domain-match for the well-known video hosts. `contains` to
        // tolerate `www.` / `m.` prefixes and `https://` schemes.
        const VIDEO_HOSTS: &[&str] = &[
            "youtube.com/",
            "youtu.be/",
            "vimeo.com/",
            // PeerTube flagship instances — federation makes a complete
            // list impossible; this is the largest-traffic subset.
            "peertube.tv/",
            "tilvids.com/",
            "kolektiva.media/",
            "framatube.org/",
            // Invidious frontends are youtube-equivalent.
            "invidious.io/",
            "yewtu.be/",
        ];
        VIDEO_HOSTS.iter().any(|host| lower.contains(host))
    })
}

/// Map a `LemmyPerson` to a Poly `User`.
pub fn map_person(person: &LemmyPerson) -> User {
    User {
        id: format!("lemmy-user-{}", person.id),
        display_name: person
            .display_name
            .clone()
            .unwrap_or_else(|| person.name.clone()),
        avatar_url: person.avatar.clone(),
        presence: PresenceStatus::Offline,
        backend: BackendType::from(crate::SLUG),
    }
}

/// Map a `CommunityView` to a Poly `Server`.
pub fn map_community_to_server(view: &CommunityView, account_id: &str, account_display_name: &str) -> Server {
    let community = &view.community;
    let channel_id = format!("lemmy-feed-{}", community.id);
    Server {
        id: format!("lemmy-community-{}", community.id),
        name: community.title.clone(),
        icon_url: community.icon.clone(),
        banner_url: community.banner.clone(),
        categories: vec![Category {
            id: "posts".to_string(),
            name: "Posts".to_string(),
            channel_ids: vec![channel_id],
        }],
        backend: BackendType::from(crate::SLUG),
        unread_count: 0,
        mention_count: 0,
        account_id: account_id.to_string(),
        account_display_name: account_display_name.to_string(),
        default_channel_id: None,
        description: None,
        star_count: None,
        language: None,
        forks_count: None,
        open_issues_count: None,
    }
}

/// Map a `CommunityView` to a Poly `ViewRow` for the account overview card grid.
///
/// - `primary_text`   — community title (display name)
/// - `secondary_text` — short handle name (`!rust@lemmy.example.com` style) or description
/// - `meta_text`      — `"X subscribers · Y active · Z unread"`
pub fn map_community_to_viewrow(view: &CommunityView, unread: u32) -> ViewRow {
    let community = &view.community;
    let counts = &view.counts;

    let secondary = community
        .description
        .as_deref()
        .filter(|d| !d.is_empty()).map_or_else(|| community.name.clone(), std::string::ToString::to_string);

    let meta = format!(
        "{} subscribers · {} active · {} unread",
        counts.subscribers, counts.users_active_week, unread,
    );

    ViewRow {
        id: format!("lemmy-community-{}", community.id),
        primary_text: community.title.clone(),
        secondary_text: Some(secondary),
        meta_text: Some(meta),
        icon: community.icon.clone(),
        badge: if unread > 0 { Some(unread.to_string()) } else { None },
        context_menu_target_kind: MenuTargetKind::Server,
        preview_image_url: None,
        is_video: false,
    }
}

/// Map a community ID to its implicit forum `Channel`.
pub fn community_to_channel(community: &LemmyCommunity) -> Channel {
    Channel {
        id: format!("lemmy-feed-{}", community.id),
        name: community.title.clone(),
        channel_type: ChannelType::Forum,
        server_id: format!("lemmy-community-{}", community.id),
        unread_count: 0,
        mention_count: 0,
        last_message_id: None,
        forum_tags: None,
        parent_channel_id: None,
        thread_metadata: None,
    }
}

/// Map a `PostView` to a Poly `Message`.
///
/// The post title becomes the message content. URL and body are appended as
/// attachments (body as inline text attachment, URL as a remote attachment).
pub fn map_post_to_message(view: &PostView) -> Message {
    let post = &view.post;
    let creator = &view.creator;
    let counts = &view.counts;

    let mut content_text = post.name.clone();
    if let Some(body) = &post.body {
        content_text.push('\n');
        content_text.push_str(body);
    }

    let mut attachments = Vec::new();
    if let Some(url) = &post.url {
        attachments.push(Attachment::remote(
            format!("lemmy-post-url-{}", post.id),
            "link".to_string(),
            "text/uri-list".to_string(),
            url.clone(),
            0,
        ));
    }
    // Add a preview image attachment when the post has a pict-rs thumbnail.
    // This lets the message-view layer render the image inline (not just
    // the forum-row thumbnail). For video posts we hint video/mp4 so the
    // host can choose a different render path if desired.
    if let Some(thumb) = &post.thumbnail_url {
        let is_vid = post_is_video(post);
        let (content_type, filename) = if is_vid {
            ("video/mp4", "video_preview.jpg")
        } else {
            ("image/png", "preview.png")
        };
        attachments.push(Attachment::remote(
            format!("lemmy-post-preview-{}", post.id),
            filename.to_string(),
            content_type.to_string(),
            thumb.clone(),
            0,
        ));
    }

    let reactions = vec![
        Reaction {
            emoji: "upvote".to_string(),
            count: u32::try_from(counts.upvotes.max(0)).unwrap_or(u32::MAX),
            me: view.my_vote == Some(1_i32),
        },
        Reaction {
            emoji: "downvote".to_string(),
            count: u32::try_from(counts.downvotes.max(0)).unwrap_or(u32::MAX),
            me: view.my_vote == Some(-1_i32),
        },
    ];

    Message {
        id: format!("lemmy-post-{}", post.id),
        author: map_person(creator),
        content: MessageContent::Text(content_text),
        timestamp: post.published,
        attachments,
        reactions,
        reply_to: None,
        edited: post.updated.is_some(),
        thread: None,
        preview_image_url: post.thumbnail_url.clone(),
    }
}

/// Format an approximate age like "3h" / "2d" / "5m" from a publish time.
///
/// Pure fn — takes `now` explicitly so tests can pin the clock.
pub fn humanize_age(published: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = now
        .signed_duration_since(published)
        .num_seconds()
        .max(0);
    // lint-allow-unused: time-bucket boundaries; truncation is the desired display semantic
    #[allow(clippy::integer_division)]
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

/// Map a `PostView` to a Poly `ViewRow` (Pack E.1).
///
/// Pure mapping — no I/O. Used by `get_view_rows`. The `SCORE:` prefix on
/// `meta_text` is load-bearing: ListBody/TreeBody render the vote-card
/// shape when it appears (per Pack A).
pub fn map_post_to_viewrow(view: &PostView, now: DateTime<Utc>, render_previews: bool) -> ViewRow {
    let post = &view.post;
    let creator = &view.creator;
    let counts = &view.counts;

    let id = post.ap_id.clone().unwrap_or_else(|| post.id.to_string());
    let secondary = format!("by {}", creator.display_name.clone().unwrap_or_else(|| creator.name.clone()));
    let meta = format!(
        "SCORE:{} · {} comments · {}",
        counts.score,
        counts.comments,
        humanize_age(post.published, now)
    );

    let preview_image_url = if render_previews {
        post.thumbnail_url.clone()
    } else {
        None
    };

    ViewRow {
        id,
        primary_text: post.name.clone(),
        secondary_text: Some(secondary),
        meta_text: Some(meta),
        icon: None,
        badge: None,
        context_menu_target_kind: MenuTargetKind::Message,
        preview_image_url,
        is_video: post_is_video(post),
    }
}

/// Build a next-page `Cursor` for offset-paginated Lemmy endpoints.
pub fn next_page_cursor(current_page: u32, page_size: usize, rows_returned: usize) -> Option<Cursor> {
    if rows_returned < page_size {
        return None;
    }
    Some(Cursor {
        kind: CursorKind::Offset,
        value: current_page.saturating_add(1).to_string(),
    })
}

/// Parse a Lemmy view cursor (offset-based) back into a 1-indexed page number.
pub fn cursor_to_page(cursor: Option<&Cursor>) -> u32 {
    cursor
        .and_then(|c| match c.kind {
            CursorKind::Offset => c.value.parse::<u32>().ok(),
            CursorKind::Timestamp | CursorKind::Id | CursorKind::Opaque => None,
        })
        .unwrap_or(1)
}

/// Map a `CommentView` to a Poly `Message`.
pub fn map_comment_to_message(view: &CommentView) -> Message {
    let comment = &view.comment;
    let creator = &view.creator;
    let counts = &view.counts;

    let reactions = vec![
        Reaction {
            emoji: "upvote".to_string(),
            count: u32::try_from(counts.upvotes.max(0)).unwrap_or(u32::MAX),
            me: view.my_vote == Some(1_i32),
        },
        Reaction {
            emoji: "downvote".to_string(),
            count: u32::try_from(counts.downvotes.max(0)).unwrap_or(u32::MAX),
            me: view.my_vote == Some(-1_i32),
        },
    ];

    Message {
        id: format!("lemmy-comment-{}", comment.id),
        author: map_person(creator),
        content: MessageContent::Text(comment.content.clone()),
        timestamp: comment.published,
        attachments: vec![],
        reactions,
        reply_to: None,
        edited: comment.updated.is_some(),
        thread: None,
        preview_image_url: None, // comments do not have preview thumbnails
    }
}

/// Map a `PrivateMessageView` to a Poly `DmChannel`.
///
/// `my_user_id` is the authenticated user's Lemmy integer ID, used to
/// identify which side of the conversation is the "other" user.
pub fn map_pm_to_dm_channel(
    view: &PrivateMessageView,
    my_user_id: i64,
    account_id: &str,
) -> DmChannel {
    let other = if view.creator.id == my_user_id {
        &view.recipient
    } else {
        &view.creator
    };

    let last_msg = map_pm_to_message(view, my_user_id);

    DmChannel {
        id: format!("lemmy-dm-{}", other.id),
        user: map_person(other),
        last_message: Some(last_msg),
        unread_count: u32::from(!view.private_message.read),
        backend: BackendType::from(crate::SLUG),
        account_id: account_id.to_string(),
    }
}

/// Map a single `PrivateMessageView` to a Poly `Message`.
pub fn map_pm_to_message(view: &PrivateMessageView, my_user_id: i64) -> Message {
    let pm = &view.private_message;
    let author = if pm.creator_id == my_user_id {
        &view.creator
    } else {
        &view.creator
    };

    Message {
        id: format!("lemmy-pm-{}", pm.id),
        author: map_person(author),
        content: MessageContent::Text(pm.content.clone()),
        timestamp: pm.published,
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited: false,
        thread: None,
        preview_image_url: None, // private messages do not have preview thumbnails
    }
}

// ── Unit tests (Pack E.1 layer-a) ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // lint-allow-unused: test module wide clippy relaxation
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

    use super::*;
    use super::super::types::PostListResponse;
    use chrono::TimeZone;

    /// Parse the checked-in Lemmy post-list fixture and exercise the pure
    /// `map_post_to_viewrow` mapping. NO NETWORK.
    #[test]
    fn map_post_to_viewrow_from_fixture() {
        let raw = include_str!("../../tests/fixtures/post_list.json");
        let resp: PostListResponse =
            serde_json::from_str(raw).expect("fixture must deserialize as PostListResponse");

        assert_eq!(resp.posts.len(), 2);

        // Pin the clock so humanize_age output is deterministic.
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();

        let row0 = map_post_to_viewrow(&resp.posts[0], now, true);
        assert_eq!(row0.id, "https://lemmy.example.com/post/101");
        assert_eq!(row0.primary_text, "Rust 2025 edition is here");
        assert_eq!(row0.secondary_text.as_deref(), Some("by Alice A."));
        let meta = row0.meta_text.expect("meta required");
        assert!(meta.starts_with("SCORE:42"), "meta must lead with SCORE:42, got {meta}");
        assert!(meta.contains("12 comments"), "meta must include comment count: {meta}");
        assert!(meta.contains("2h"), "meta must include humanized age 2h: {meta}");
        assert_eq!(row0.context_menu_target_kind, MenuTargetKind::Message);

        // Row 1: creator has no display_name → falls back to `name`.
        let row1 = map_post_to_viewrow(&resp.posts[1], now, true);
        assert_eq!(row1.secondary_text.as_deref(), Some("by bob"));
        let meta1 = row1.meta_text.expect("meta required");
        assert!(meta1.starts_with("SCORE:128"));
        assert!(meta1.contains("5 comments"));
    }

    #[test]
    fn humanize_age_buckets() {
        let base = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        assert_eq!(
            humanize_age(base - chrono::Duration::seconds(30), base),
            "30s"
        );
        assert_eq!(
            humanize_age(base - chrono::Duration::minutes(5), base),
            "5m"
        );
        assert_eq!(humanize_age(base - chrono::Duration::hours(3), base), "3h");
        assert_eq!(humanize_age(base - chrono::Duration::days(2), base), "2d");
    }

    #[test]
    fn cursor_round_trip_offset() {
        let c = Cursor {
            kind: CursorKind::Offset,
            value: "3".to_string(),
        };
        assert_eq!(cursor_to_page(Some(&c)), 3);
        assert_eq!(cursor_to_page(None), 1);

        // Full page → next cursor advances.
        let next = next_page_cursor(3, 25, 25).expect("full page must produce next cursor");
        assert_eq!(next.value, "4");
        assert_eq!(next.kind, CursorKind::Offset);

        // Short page → no next cursor.
        assert!(next_page_cursor(3, 25, 10).is_none());
    }

    /// Verify that `LemmyPost.thumbnail_url` propagates to `ViewRow.preview_image_url`
    /// through `map_post_to_viewrow` when `render_previews` is true, and is suppressed
    /// when `render_previews` is false.
    ///
    /// Also verifies propagation through `map_post_to_message.preview_image_url`.
    #[test]
    fn thumbnail_url_propagates_to_preview_image_url() {
        let raw = include_str!("../../tests/fixtures/post_list.json");
        let resp: PostListResponse =
            serde_json::from_str(raw).expect("fixture must deserialize");

        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();

        // Post 0 has thumbnail_url set in the fixture.
        let view0 = &resp.posts[0];
        assert_eq!(
            view0.post.thumbnail_url.as_deref(),
            Some("https://lemmy.example.com/pictrs/image/test-preview.png"),
            "fixture thumbnail_url must deserialize correctly"
        );

        // render_previews = true: preview_image_url is populated on the ViewRow.
        let row_on = map_post_to_viewrow(view0, now, true);
        assert_eq!(
            row_on.preview_image_url.as_deref(),
            Some("https://lemmy.example.com/pictrs/image/test-preview.png"),
            "render_previews=true must propagate thumbnail_url to ViewRow.preview_image_url"
        );

        // render_previews = false: preview_image_url is suppressed on the ViewRow.
        let row_off = map_post_to_viewrow(view0, now, false);
        assert_eq!(
            row_off.preview_image_url,
            None,
            "render_previews=false must suppress preview_image_url even when thumbnail_url is set"
        );

        // map_post_to_message always propagates thumbnail_url → preview_image_url
        // (the mechanism check lives in get_view_rows, not in the message mapper).
        let msg = map_post_to_message(view0);
        assert_eq!(
            msg.preview_image_url.as_deref(),
            Some("https://lemmy.example.com/pictrs/image/test-preview.png"),
            "map_post_to_message must propagate thumbnail_url to Message.preview_image_url"
        );

        // Post 1 has no thumbnail_url — preview_image_url must be None.
        let view1 = &resp.posts[1];
        assert!(view1.post.thumbnail_url.is_none(), "post[1] has no thumbnail_url in fixture");
        let row1 = map_post_to_viewrow(view1, now, true);
        assert_eq!(
            row1.preview_image_url,
            None,
            "absent thumbnail_url must produce None preview_image_url"
        );
    }
}
