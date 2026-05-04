#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-2 parity: `BackendCapabilities::is_forum_layout()` returns the expected
//! answer for the known forum and non-forum backend presets.
//!
//! `BackendType::uses_forum_layout()` was removed in Phase B.2 of
//! plan-solid-refactor-survey; the runtime registry in `ClientManager` is now
//! the single source of truth for per-slug capabilities. This test asserts the
//! preset constants that seed that registry.

use poly_client::{BackendCapabilities, NotificationSupport, CommunitySearchSupport};

#[test]
fn forum_layout_matches_legacy_slug_list() {
    // These slugs resolve to presets that have is_forum_layout() == true.
    // hackernews, github, forgejo → READ_ONLY_FEED (or Activity variant)
    // lemmy, demo_forum → MESSAGING_NO_SOCIAL (or moderation variant)
    let forum_presets: &[(&str, BackendCapabilities)] = &[
        ("hackernews", BackendCapabilities::READ_ONLY_FEED),
        ("github", BackendCapabilities {
            notifications: NotificationSupport::Activity,
            ..BackendCapabilities::READ_ONLY_FEED
        }),
        ("lemmy", BackendCapabilities {
            has_ban: true,
            has_timed_ban: true,
            has_moderation_log: true,
            community_search: CommunitySearchSupport::SubscribedLocalAll,
            supports_comment_feed: true,
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        }),
        ("demo_forum", BackendCapabilities {
            has_ban: true,
            has_timed_ban: true,
            has_moderation_log: true,
            community_search: CommunitySearchSupport::SubscribedLocalAll,
            supports_comment_feed: true,
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        }),
    ];

    for (slug, caps) in forum_presets {
        assert!(
            caps.is_forum_layout(),
            "backend '{slug}' preset should use forum layout"
        );
    }

    // These slugs resolve to FULL_SOCIAL_CHAT (or a variant) — not forum layout.
    let non_forum_presets: &[(&str, BackendCapabilities)] = &[
        ("demo", BackendCapabilities::FULL_SOCIAL_CHAT),
        ("matrix", BackendCapabilities::FULL_SOCIAL_CHAT),
        ("discord", BackendCapabilities::FULL_SOCIAL_CHAT),
        ("teams", BackendCapabilities::FULL_SOCIAL_CHAT),
        ("stoat", BackendCapabilities::FULL_SOCIAL_CHAT),
        ("poly", BackendCapabilities::FULL_SOCIAL_CHAT),
    ];

    for (slug, caps) in non_forum_presets {
        assert!(
            !caps.is_forum_layout(),
            "backend '{slug}' preset should NOT use forum layout"
        );
    }
}
