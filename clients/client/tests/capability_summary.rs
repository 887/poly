//! Regression test for `BackendCapabilities::shape_rows()` — the helper that
//! drives the Settings > Plugins capability details panel.
//!
//! This helper returns FTL key pairs so the UI can render a human-readable
//! summary without duplicating the match-on-enum logic in a dozen places.
//! Pinning the rows here prevents silent drift where a variant is added to
//! `MessagingModel` / `NotificationSupport` / etc. but the summary rows
//! silently fall through to a wrong label.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{BackendCapabilities, capabilities_for_slug};

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
    let rows = capabilities_for_slug("hackernews").shape_rows();
    assert_eq!(rows[0].value_key, "cap-value-messaging-readonly");
    assert_eq!(rows[1].value_key, "cap-value-dms-none");
    assert_eq!(rows[2].value_key, "cap-value-friends-none");
    assert_eq!(rows[3].value_key, "cap-value-notifications-none");
    assert_eq!(rows[4].value_key, "cap-value-voice-none");
}

#[test]
fn lemmy_shape_rows_show_inbox_notifications_and_full_messaging() {
    let rows = capabilities_for_slug("lemmy").shape_rows();
    assert_eq!(rows[0].value_key, "cap-value-messaging-full");
    assert_eq!(rows[3].value_key, "cap-value-notifications-inbox");
}

#[test]
fn github_shape_rows_show_activity_notifications_on_readonly_base() {
    let rows = capabilities_for_slug("github").shape_rows();
    assert_eq!(rows[0].value_key, "cap-value-messaging-readonly");
    assert_eq!(rows[3].value_key, "cap-value-notifications-activity");
    assert_eq!(rows[4].value_key, "cap-value-voice-none");
}

#[test]
fn discord_shape_rows_show_full_social_chat() {
    let rows = capabilities_for_slug("discord").shape_rows();
    assert_eq!(rows[0].value_key, "cap-value-messaging-full");
    assert_eq!(rows[1].value_key, "cap-value-dms-user");
    assert_eq!(rows[2].value_key, "cap-value-friends-full");
    assert_eq!(rows[4].value_key, "cap-value-voice-full");
}

#[test]
fn matrix_has_no_voice_even_though_social_chat() {
    let rows = capabilities_for_slug("matrix").shape_rows();
    assert_eq!(rows[1].value_key, "cap-value-dms-user");
    assert_eq!(rows[4].value_key, "cap-value-voice-none");
}

