//! Regression test for `BackendCapabilities::shape_rows()` — the helper that
//! drives the Settings > Plugins capability details panel.
//!
//! This helper returns FTL key pairs so the UI can render a human-readable
//! summary without duplicating the match-on-enum logic in a dozen places.
//! Pinning the rows here prevents silent drift where a variant is added to
//! `MessagingModel` / `NotificationSupport` / etc. but the summary rows
//! silently fall through to a wrong label.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{BackendCapabilities, NotificationSupport, VoiceSupport};

#[test]
fn shape_rows_cover_all_five_dimensions_in_stable_order() {
    let rows = BackendCapabilities::FULL_SOCIAL_CHAT.shape_rows();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].label_key, "cap-label-messaging");
    assert_eq!(rows[1].label_key, "cap-label-dms");
    assert_eq!(rows[2].label_key, "cap-label-friends");
    assert_eq!(rows[3].label_key, "cap-label-notifications");
    assert_eq!(rows[4].label_key, "cap-label-voice");
}

#[test]
fn read_only_feed_shape_rows_match_hackernews_preset() {
    let rows = BackendCapabilities::READ_ONLY_FEED.shape_rows();
    assert_eq!(rows[0].value_key, "cap-value-messaging-readonly");
    assert_eq!(rows[1].value_key, "cap-value-dms-none");
    assert_eq!(rows[2].value_key, "cap-value-friends-none");
    assert_eq!(rows[3].value_key, "cap-value-notifications-none");
    assert_eq!(rows[4].value_key, "cap-value-voice-none");
}

#[test]
fn messaging_no_social_shape_rows_show_inbox_notifications_and_full_messaging() {
    let rows = BackendCapabilities::MESSAGING_NO_SOCIAL.shape_rows();
    assert_eq!(rows[0].value_key, "cap-value-messaging-full");
    assert_eq!(rows[3].value_key, "cap-value-notifications-inbox");
}

#[test]
fn activity_notifications_on_readonly_base() {
    // BackendCapabilities with Activity notifications on ReadOnly base (e.g. GitHub/Forgejo).
    let caps = BackendCapabilities {
        notifications: NotificationSupport::Activity,
        ..BackendCapabilities::READ_ONLY_FEED
    };
    let rows = caps.shape_rows();
    assert_eq!(rows[0].value_key, "cap-value-messaging-readonly");
    assert_eq!(rows[3].value_key, "cap-value-notifications-activity");
    assert_eq!(rows[4].value_key, "cap-value-voice-none");
}

#[test]
fn full_social_chat_shape_rows_show_full_social_chat() {
    let rows = BackendCapabilities::FULL_SOCIAL_CHAT.shape_rows();
    assert_eq!(rows[0].value_key, "cap-value-messaging-full");
    assert_eq!(rows[1].value_key, "cap-value-dms-user");
    assert_eq!(rows[2].value_key, "cap-value-friends-full");
    assert_eq!(rows[4].value_key, "cap-value-voice-full");
}

#[test]
fn no_voice_social_chat_shape_rows_have_voice_none() {
    // Matrix/Stoat: full social chat but VoiceSupport::None.
    let caps = BackendCapabilities {
        voice: VoiceSupport::None,
        ..BackendCapabilities::FULL_SOCIAL_CHAT
    };
    let rows = caps.shape_rows();
    assert_eq!(rows[1].value_key, "cap-value-dms-user");
    assert_eq!(rows[4].value_key, "cap-value-voice-none");
}
