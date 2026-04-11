#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-1 regression: Matrix is full social chat minus voice and create_server.

use poly_client::{
    ClientBackend, DmSupport, FriendModel, MessagingModel, NotificationSupport, VoiceSupport,
    capabilities_for_slug,
};
use poly_matrix::MatrixClient;

#[test]
fn matrix_declares_full_social_chat_without_voice() {
    let client = MatrixClient::new();
    let caps = client.backend_capabilities();
    assert_eq!(caps.messaging, MessagingModel::Full);
    assert_eq!(caps.dms, DmSupport::User);
    assert_eq!(caps.friends, FriendModel::Full);
    assert_eq!(caps.notifications, NotificationSupport::Activity);
    assert!(matches!(caps.voice, VoiceSupport::None), "Matrix should not advertise voice");
    assert!(!caps.create_server, "Matrix does not create servers");
    assert!(caps.presence);
    assert!(caps.typing_indicators);
    assert!(caps.reactions);
    assert!(caps.attachments);
}

/// WP-10 — plugin-vs-slug parity.
#[test]
fn matrix_plugin_matches_slug_lookup_table() {
    let via_trait = MatrixClient::new().backend_capabilities();
    let via_slug = capabilities_for_slug("matrix");
    assert_eq!(via_trait, via_slug);
}
