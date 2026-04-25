//! WP-10 — Capability matrix regression test.
//!
//! Pins the exact `BackendCapabilities` shape that every compiled-in plugin slug
//! resolves to via `capabilities_for_slug()`. If someone tweaks a plugin's
//! capability declaration without updating this fixture, the test fails loudly
//! so the downstream UI gating assumptions (WP-3/4/5/7/9) can be re-verified.
//!
//! The per-plugin tests in `clients/<plugin>/tests/capabilities.rs` assert each
//! plugin's OWN declaration. This test is the *central* registry — it catches
//! the case where a plugin ships a new capability but the slug-mapping table in
//! `clients/client/src/types.rs` is never updated.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::{
    BackendCapabilities, DmSupport, FriendModel, MessagingModel,
    NotificationSupport, VoiceSupport, capabilities_for_slug,
};

fn expected(slug: &str) -> BackendCapabilities {
    match slug {
        "hackernews" => BackendCapabilities::READ_ONLY_FEED,
        "github" | "forgejo" => BackendCapabilities {
            notifications: NotificationSupport::Activity,
            // landing inherits LandingPage::Overview from READ_ONLY_FEED.
            ..BackendCapabilities::READ_ONLY_FEED
        },
        "lemmy" | "demo_forum" => BackendCapabilities {
            has_ban: true,
            has_timed_ban: true,
            has_moderation_log: true,
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        },
        "matrix" => BackendCapabilities {
            voice: VoiceSupport::None,
            has_kick: true,
            has_ban: true,
            has_channel_mgmt: true,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
        "stoat" => BackendCapabilities {
            voice: VoiceSupport::None,
            has_roles: true,
            has_kick: true,
            has_ban: true,
            has_timed_ban: true,
            has_channel_mgmt: true,
            has_moderation_log: false,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
        "teams" => BackendCapabilities {
            supports_typing_indicators: false,
            has_roles: false,
            has_kick: true,
            has_ban: false,
            has_timed_ban: false,
            has_channel_mgmt: true,
            has_moderation_log: false,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
        "discord" | "demo" | "poly" => BackendCapabilities {
            has_roles: true,
            has_kick: true,
            has_ban: true,
            has_timed_ban: true,
            has_channel_mgmt: true,
            has_moderation_log: true,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
        _ => BackendCapabilities::READ_ONLY_FEED,
    }
}

#[test]
fn every_compiled_slug_matches_expected_capabilities() {
    for slug in [
        "hackernews",
        "github",
        "lemmy",
        "demo_forum",
        "matrix",
        "stoat",
        "teams",
        "discord",
        "demo",
        "poly",
    ] {
        let actual = capabilities_for_slug(slug);
        let want = expected(slug);
        assert_eq!(
            actual, want,
            "capability mismatch for '{slug}':\n  actual: {actual:?}\n  expected: {want:?}"
        );
    }
}

#[test]
fn unknown_slug_returns_read_only_feed() {
    // An unknown plugin should be treated conservatively: pure read-only feed.
    // This prevents a malicious or bugged plugin from silently gaining write
    // access to the UI via a novel slug.
    let caps = capabilities_for_slug("this-is-not-a-real-plugin");
    assert_eq!(caps, BackendCapabilities::READ_ONLY_FEED);
}

#[test]
fn read_only_feed_is_forum_layout() {
    // is_forum_layout() is a const fn used by BackendType::uses_forum_layout().
    // It must be true for the read-only preset — that's the whole reason HN /
    // Lemmy / GitHub render the forum view instead of the chat view.
    assert!(BackendCapabilities::READ_ONLY_FEED.is_forum_layout());
}

#[test]
fn full_social_chat_is_not_forum_layout() {
    // Full-chat backends (Discord, Stoat, Matrix, Teams, Poly) must NOT be
    // treated as forums — that would trigger ForumView rendering instead of
    // ChatView and break the composer entirely.
    assert!(!BackendCapabilities::FULL_SOCIAL_CHAT.is_forum_layout());
}

#[test]
fn messaging_no_social_is_forum_layout() {
    // Lemmy posts threaded replies → forum layout.
    assert!(BackendCapabilities::MESSAGING_NO_SOCIAL.is_forum_layout());
}

#[test]
fn read_only_feed_has_no_messaging() {
    assert!(matches!(
        BackendCapabilities::READ_ONLY_FEED.messaging,
        MessagingModel::None | MessagingModel::ReadOnly
    ));
    assert!(matches!(
        BackendCapabilities::READ_ONLY_FEED.dms,
        DmSupport::None
    ));
    assert!(matches!(
        BackendCapabilities::READ_ONLY_FEED.friends,
        FriendModel::None
    ));
    assert!(matches!(
        BackendCapabilities::READ_ONLY_FEED.voice,
        VoiceSupport::None
    ));
}

#[test]
fn full_social_chat_has_all_social_features() {
    let caps = BackendCapabilities::FULL_SOCIAL_CHAT;
    assert!(matches!(caps.messaging, MessagingModel::Full));
    assert!(matches!(caps.dms, DmSupport::User));
    assert!(matches!(caps.friends, FriendModel::Full));
    assert!(!matches!(caps.notifications, NotificationSupport::None));
    assert!(matches!(caps.voice, VoiceSupport::Full));
}
