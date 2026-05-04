//! WP-10 — Capability matrix regression test.
//!
//! Pins the `BackendCapabilities` preset shapes used by the UI gating logic.
//! If someone tweaks a preset's fields without updating this fixture, the test
//! fails loudly so the downstream UI gating assumptions (WP-3/4/5/7/9) can be
//! re-verified.
//!
//! Slug-keyed parity tests that previously called `capabilities_for_slug()` were
//! removed in Phase B.2 of plan-solid-refactor-survey: the runtime registry in
//! `ClientManager` is now the single source of truth for per-slug capabilities.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::{
    BackendCapabilities, DmSupport, FriendModel, MessagingModel,
    NotificationSupport, VoiceSupport,
};

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
