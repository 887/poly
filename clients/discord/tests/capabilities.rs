#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-1 regression: Discord is full social chat with full moderation capabilities.

use poly_client::{
    BackendCapabilities, ClientBackend, DmSupport, FriendModel, MessagingModel,
    NotificationSupport, VoiceSupport, capabilities_for_slug,
};
use poly_discord::DiscordClient;

#[test]
fn discord_declares_full_social_chat() {
    let client = DiscordClient::new();
    let caps = client.backend_capabilities();
    // Core social-chat properties.
    assert_eq!(caps.messaging, MessagingModel::Full);
    assert_eq!(caps.dms, DmSupport::User);
    assert_eq!(caps.friends, FriendModel::Full);
    assert_eq!(caps.notifications, NotificationSupport::Activity);
    assert!(matches!(caps.voice, VoiceSupport::Full));
    // B-DS-10: full moderation capabilities.
    assert!(caps.has_roles, "discord must have has_roles");
    assert!(caps.has_kick, "discord must have has_kick");
    assert!(caps.has_ban, "discord must have has_ban");
    assert!(caps.has_timed_ban, "discord must have has_timed_ban");
    assert!(caps.has_channel_mgmt, "discord must have has_channel_mgmt");
    assert!(caps.has_moderation_log, "discord must have has_moderation_log");
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
