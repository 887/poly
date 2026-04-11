#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-1 regression: Discord is full social chat.

use poly_client::{
    BackendCapabilities, ClientBackend, DmSupport, FriendModel, MessagingModel,
    NotificationSupport, VoiceSupport, capabilities_for_slug,
};
use poly_discord::DiscordClient;

#[test]
fn discord_declares_full_social_chat() {
    let client = DiscordClient::new();
    let caps = client.backend_capabilities();
    assert_eq!(caps, BackendCapabilities::FULL_SOCIAL_CHAT);
    assert_eq!(caps.messaging, MessagingModel::Full);
    assert_eq!(caps.dms, DmSupport::User);
    assert_eq!(caps.friends, FriendModel::Full);
    assert_eq!(caps.notifications, NotificationSupport::Activity);
    assert!(matches!(caps.voice, VoiceSupport::Full));
    assert!(caps.presence);
    assert!(caps.typing_indicators);
}

/// WP-10 — plugin-vs-slug parity. If someone tweaks Discord's declaration
/// without syncing `capabilities_for_slug("discord")` the UI gating layer
/// will drift silently. This test catches that drift.
#[test]
fn discord_plugin_matches_slug_lookup_table() {
    let via_trait = DiscordClient::new().backend_capabilities();
    let via_slug = capabilities_for_slug("discord");
    assert_eq!(
        via_trait, via_slug,
        "DiscordClient::backend_capabilities() and capabilities_for_slug(\"discord\") diverged"
    );
}
