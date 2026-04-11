#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-1 regression: Hacker News is a read-only feed.

use poly_client::{
    BackendCapabilities, ClientBackend, DmSupport, FriendModel, MessagingModel,
    NotificationSupport, VoiceSupport, capabilities_for_slug,
};
use poly_hackernews::HackerNewsClient;

#[test]
fn hackernews_declares_read_only_feed() {
    let client = HackerNewsClient::new();
    let caps = client.backend_capabilities();
    assert_eq!(caps, BackendCapabilities::READ_ONLY_FEED);
    assert_eq!(caps.messaging, MessagingModel::ReadOnly);
    assert_eq!(caps.dms, DmSupport::None);
    assert_eq!(caps.friends, FriendModel::None);
    assert_eq!(caps.notifications, NotificationSupport::None);
    assert!(matches!(caps.voice, VoiceSupport::None));
    assert!(!caps.presence);
    assert!(!caps.typing_indicators);
    assert!(!caps.reactions);
    assert!(!caps.attachments);
    assert!(!caps.create_server);
    assert!(!caps.create_channel);
}

/// WP-10 — plugin-vs-slug parity.
#[test]
fn hackernews_plugin_matches_slug_lookup_table() {
    let via_trait = HackerNewsClient::new().backend_capabilities();
    let via_slug = capabilities_for_slug("hackernews");
    assert_eq!(via_trait, via_slug);
}
