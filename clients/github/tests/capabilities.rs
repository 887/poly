#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-1 regression: GitHub is a read-only feed with an activity notification inbox.

use poly_client::{
    ClientBackend, DmSupport, FriendModel, MessagingModel, NotificationSupport, VoiceSupport,
    capabilities_for_slug,
};
use poly_github::GitHubClient;

#[test]
fn github_declares_read_only_feed_with_activity_inbox() {
    let client = GitHubClient::dotcom();
    let caps = client.backend_capabilities();
    assert_eq!(caps.messaging, MessagingModel::ReadOnly);
    assert_eq!(caps.dms, DmSupport::None);
    assert_eq!(caps.friends, FriendModel::None);
    assert_eq!(caps.notifications, NotificationSupport::Activity);
    assert!(matches!(caps.voice, VoiceSupport::None));
    assert!(caps.search_messages);
    assert!(!caps.presence);
    assert!(!caps.typing_indicators);
    assert!(!caps.reactions);
    assert!(!caps.attachments);
    assert!(!caps.create_server);
    assert!(!caps.create_channel);
}

/// WP-10 — plugin-vs-slug parity.
#[test]
fn github_plugin_matches_slug_lookup_table() {
    let via_trait = GitHubClient::dotcom().backend_capabilities();
    let via_slug = capabilities_for_slug("github");
    assert_eq!(via_trait, via_slug);
}
