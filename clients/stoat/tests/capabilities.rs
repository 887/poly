#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-1 regression: Stoat (Revolt) is full social chat minus voice.

use poly_client::{
    ClientBackend, DmSupport, FriendModel, MessagingModel, NotificationSupport, VoiceSupport,
    capabilities_for_slug,
};
use poly_stoat::StoatClient;

#[test]
fn stoat_declares_full_social_chat_without_voice() {
    let client = StoatClient::new();
    let caps = client.backend_capabilities();
    assert_eq!(caps.messaging, MessagingModel::Full);
    assert_eq!(caps.dms, DmSupport::User);
    assert_eq!(caps.friends, FriendModel::Full);
    assert_eq!(caps.notifications, NotificationSupport::Activity);
    assert!(matches!(caps.voice, VoiceSupport::None));
    assert!(caps.presence);
    assert!(caps.typing_indicators);
    assert!(caps.reactions);
}

/// WP-10 — plugin-vs-slug parity.
#[test]
fn stoat_plugin_matches_slug_lookup_table() {
    let via_trait = StoatClient::new().backend_capabilities();
    let via_slug = capabilities_for_slug("stoat");
    assert_eq!(via_trait, via_slug);
}
