//! Phase 3 — Discord channel type mapping and field-parsing unit tests.
//!
//! Pure deserialization + mapping tests: no HTTP calls, no test server.
//! Tests:
//! - Channel type integers → `poly_client::ChannelType`
//! - Forum channel JSON → `Channel.forum_tags` populated
//! - Thread channel JSON → `Channel.thread_metadata` populated
//! - Message with `thread` field → `Message.thread = Some(...)`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::ChannelType;
use poly_discord::test_helpers::{channel_from_json, map_discord_channel_type, message_from_json};

// ─── Channel-type mapping ──────────────────────────────────────────────────

#[test]
fn guild_text_maps_to_text() {
    assert_eq!(map_discord_channel_type(0), ChannelType::Text);
}

#[test]
fn guild_announcement_maps_to_announcement() {
    assert_eq!(map_discord_channel_type(5), ChannelType::Announcement);
}

#[test]
fn announcement_thread_maps_to_thread() {
    assert_eq!(map_discord_channel_type(10), ChannelType::Thread);
}

#[test]
fn public_thread_maps_to_thread() {
    assert_eq!(map_discord_channel_type(11), ChannelType::Thread);
}

#[test]
fn private_thread_maps_to_thread() {
    assert_eq!(map_discord_channel_type(12), ChannelType::Thread);
}

#[test]
fn guild_forum_maps_to_forum() {
    assert_eq!(map_discord_channel_type(15), ChannelType::Forum);
}

#[test]
fn guild_media_maps_to_forum() {
    assert_eq!(map_discord_channel_type(16), ChannelType::Forum);
}

// ─── Forum channel: available_tags → forum_tags ────────────────────────────

#[test]
fn forum_channel_json_populates_forum_tags() {
    let json = r#"{
        "id": "99991",
        "name": "my-forum",
        "type": 15,
        "guild_id": "100",
        "available_tags": [
            {
                "id": "77771",
                "name": "question",
                "moderated": false,
                "emoji_id": null,
                "emoji_name": "❓"
            },
            {
                "id": "77772",
                "name": "announcement",
                "moderated": true,
                "emoji_id": null,
                "emoji_name": null
            }
        ]
    }"#;

    let ch = channel_from_json(json, "100").expect("channel parse");
    assert_eq!(ch.channel_type, ChannelType::Forum);
    let tags = ch.forum_tags.expect("forum_tags should be Some");
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].id, "77771");
    assert_eq!(tags[0].name, "question");
    assert_eq!(tags[0].emoji.as_deref(), Some("❓"));
    assert!(!tags[0].moderated);
    assert_eq!(tags[1].id, "77772");
    assert_eq!(tags[1].name, "announcement");
    assert!(tags[1].moderated);
    assert!(tags[1].emoji.is_none());
}

// ─── Thread channel: thread_metadata populated ────────────────────────────

#[test]
fn thread_channel_json_populates_thread_metadata() {
    let json = r#"{
        "id": "88881",
        "name": "my-thread",
        "type": 11,
        "guild_id": "100",
        "parent_id": "200",
        "thread_metadata": {
            "archived": false,
            "auto_archive_duration": 1440,
            "archive_timestamp": null,
            "locked": false,
            "create_timestamp": "2024-01-15T10:30:00+00:00"
        },
        "message_count": 7,
        "member_count": 3
    }"#;

    let ch = channel_from_json(json, "100").expect("thread channel parse");
    assert_eq!(ch.channel_type, ChannelType::Thread);
    assert_eq!(ch.parent_channel_id.as_deref(), Some("200"));
    let meta = ch.thread_metadata.expect("thread_metadata should be Some");
    assert!(!meta.archived);
    assert!(!meta.locked);
    assert_eq!(meta.auto_archive_minutes, 1440);
}

// ─── Message with thread field → Message.thread = Some(...) ───────────────

#[test]
fn message_with_thread_field_populates_thread_info() {
    let json = r#"{
        "id": "55551",
        "content": "Check out this thread!",
        "channel_id": "200",
        "timestamp": "2024-01-15T10:00:00+00:00",
        "author": {
            "id": "1",
            "username": "koala",
            "discriminator": "0",
            "global_name": "Koala"
        },
        "thread": {
            "id": "88882",
            "name": "discussion",
            "type": 11,
            "parent_id": "200",
            "guild_id": "100",
            "message_count": 5,
            "member_count": 2
        }
    }"#;

    let msg = message_from_json(json).expect("message parse");
    let thread_info = msg.thread.expect("Message.thread should be Some");
    assert_eq!(thread_info.thread_id, "88882");
    assert_eq!(thread_info.parent_channel_id, "200");
    assert_eq!(thread_info.message_count, 5);
    assert_eq!(thread_info.member_count, 2);
}

#[test]
fn message_without_thread_field_has_none() {
    let json = r#"{
        "id": "55552",
        "content": "Plain message",
        "channel_id": "200",
        "timestamp": "2024-01-15T10:00:00+00:00",
        "author": {
            "id": "1",
            "username": "koala",
            "discriminator": "0"
        }
    }"#;

    let msg = message_from_json(json).expect("message parse");
    assert!(msg.thread.is_none());
}
