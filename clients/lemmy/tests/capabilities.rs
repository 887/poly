#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-1 regression: Lemmy supports messaging but no DMs/friends/voice.

use poly_client::{
    ClientBackend, DmSupport, FriendModel, MessagingModel, NotificationSupport, VoiceSupport,
    capabilities_for_slug,
};
use poly_lemmy::LemmyClient;

#[test]
fn lemmy_declares_messaging_no_social_with_inbox() {
    let client = LemmyClient::new("https://lemmy.example");
    let caps = client.backend_capabilities();
    assert_eq!(caps.messaging, MessagingModel::Full);
    assert_eq!(caps.dms, DmSupport::None);
    assert_eq!(caps.friends, FriendModel::None);
    assert_eq!(caps.notifications, NotificationSupport::Inbox);
    assert!(matches!(caps.voice, VoiceSupport::None));
    assert!(caps.attachments);
    assert!(caps.search_messages);
    assert!(caps.reactions);
    assert!(!caps.create_server);
    assert!(!caps.create_channel);
    assert!(!caps.presence);
    assert!(!caps.typing_indicators);
}

/// WP-10 — plugin-vs-slug parity.
#[test]
fn lemmy_plugin_matches_slug_lookup_table() {
    let via_trait = LemmyClient::new("https://lemmy.example").backend_capabilities();
    let via_slug = capabilities_for_slug("lemmy");
    assert_eq!(via_trait, via_slug);
}
