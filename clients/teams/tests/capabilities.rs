#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-1 regression: Teams is full social chat except no typing indicators.

use poly_client::{
    ClientBackend, DmSupport, FriendModel, MessagingModel, NotificationSupport, VoiceSupport,
    capabilities_for_slug,
};
use poly_teams::TeamsClient;

#[test]
fn teams_declares_full_social_chat_without_typing() {
    let client = TeamsClient::new();
    let caps = client.backend_capabilities();
    assert_eq!(caps.messaging, MessagingModel::Full);
    assert_eq!(caps.dms, DmSupport::User);
    assert_eq!(caps.friends, FriendModel::Full);
    assert_eq!(caps.notifications, NotificationSupport::Activity);
    assert!(matches!(caps.voice, VoiceSupport::Full));
    assert!(caps.presence);
    assert!(!caps.typing_indicators, "Teams should disable typing indicators");
    assert!(caps.reactions);
    assert!(caps.attachments);
}

/// WP-10 — plugin-vs-slug parity.
#[test]
fn teams_plugin_matches_slug_lookup_table() {
    let via_trait = TeamsClient::new().backend_capabilities();
    let via_slug = capabilities_for_slug("teams");
    assert_eq!(via_trait, via_slug);
}
