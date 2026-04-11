//! Regression test for `BackendCapabilities::shape_rows()` and
//! `feature_flags()` — the two helpers that drive the Settings > Plugins
//! capability details panel.
//!
//! These helpers return FTL key pairs so the UI can render a human-readable
//! summary without duplicating the match-on-enum logic in a dozen places.
//! Pinning them here prevents silent drift where a variant is added to
//! `MessagingModel` / `NotificationSupport` / etc. but the summary rows
//! silently fall through to a wrong label.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

#[test]
fn feature_flags_order_is_stable_for_ui_rendering() {
    let flags = BackendCapabilities::FULL_SOCIAL_CHAT.feature_flags();
    let keys: Vec<&'static str> = flags.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        keys,
        vec![
            "cap-flag-presence",
            "cap-flag-typing",
            "cap-flag-reactions",
            "cap-flag-search",
            "cap-flag-attachments",
            "cap-flag-create-server",
            "cap-flag-create-channel",
        ]
    );
}

#[test]
fn feature_flags_reflect_hackernews_everything_off() {
    let flags = capabilities_for_slug("hackernews").feature_flags();
    for (key, supported) in &flags {
        assert!(!supported, "hackernews should not advertise {key}");
    }
}

#[test]
fn feature_flags_reflect_discord_all_on() {
    let flags = capabilities_for_slug("discord").feature_flags();
    for (key, supported) in &flags {
        assert!(supported, "discord should advertise {key}");
    }
}

#[test]
fn feature_flags_reflect_teams_typing_disabled() {
    let flags = capabilities_for_slug("teams").feature_flags();
    let typing = flags
        .iter()
        .find(|(k, _)| *k == "cap-flag-typing")
        .expect("typing flag present");
    assert!(!typing.1, "teams has no typing indicators");
}
