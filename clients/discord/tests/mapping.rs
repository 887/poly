//! Phase 3 — Discord channel type mapping and field-parsing unit tests.
//!
//! Pure deserialization + mapping tests: no HTTP calls, no test server.
//! Tests:
//! - Channel type integers → `poly_client::ChannelType`
//! - Forum channel JSON → `Channel.forum_tags` populated
//! - Thread channel JSON → `Channel.thread_metadata` populated
//! - Message with `thread` field → `Message.thread = Some(...)`
//! - Gateway events THREAD_CREATE / THREAD_UPDATE / THREAD_DELETE / THREAD_LIST_SYNC

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_client::{ChannelType, ClientEvent};
use poly_discord::test_helpers::{
    channel_from_json, gateway_events_from_json, map_discord_channel_type, message_from_json,
};

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

// ─── Gateway events (3.8 / 3.9) ──────────────────────────────────────────────

/// THREAD_CREATE emits ChannelUpdated with a Thread channel.
#[test]
fn gateway_thread_create_emits_channel_updated() {
    let data = r#"{
        "id": "88883",
        "name": "new-thread",
        "type": 11,
        "guild_id": "100",
        "parent_id": "200",
        "thread_metadata": {
            "archived": false,
            "auto_archive_duration": 1440,
            "locked": false,
            "create_timestamp": "2024-01-20T10:00:00+00:00"
        },
        "message_count": 0,
        "member_count": 1
    }"#;
    let events = gateway_events_from_json("THREAD_CREATE", data, "100").expect("parse");
    assert_eq!(events.len(), 1);
    match &events[0] {
        ClientEvent::ChannelUpdated(ch) => {
            assert_eq!(ch.id, "88883");
            assert_eq!(ch.channel_type, ChannelType::Thread);
            assert_eq!(ch.parent_channel_id.as_deref(), Some("200"));
        }
        other => panic!("expected ChannelUpdated, got {other:?}"),
    }
}

/// THREAD_UPDATE emits ChannelUpdated (e.g. thread archived).
#[test]
fn gateway_thread_update_emits_channel_updated() {
    let data = r#"{
        "id": "88884",
        "name": "archived-thread",
        "type": 11,
        "guild_id": "100",
        "parent_id": "200",
        "thread_metadata": {
            "archived": true,
            "auto_archive_duration": 1440,
            "archive_timestamp": "2024-01-22T08:00:00+00:00",
            "locked": false,
            "create_timestamp": "2024-01-20T10:00:00+00:00"
        },
        "message_count": 5,
        "member_count": 3
    }"#;
    let events = gateway_events_from_json("THREAD_UPDATE", data, "100").expect("parse");
    assert_eq!(events.len(), 1);
    match &events[0] {
        ClientEvent::ChannelUpdated(ch) => {
            assert_eq!(ch.id, "88884");
            let meta = ch.thread_metadata.as_ref().expect("thread_metadata present");
            assert!(meta.archived, "thread should be archived");
        }
        other => panic!("expected ChannelUpdated, got {other:?}"),
    }
}

/// THREAD_DELETE emits a tombstone ChannelUpdated.
#[test]
fn gateway_thread_delete_emits_tombstone() {
    let data = r#"{
        "id": "88885",
        "guild_id": "100",
        "parent_id": "200",
        "type": 11
    }"#;
    let events = gateway_events_from_json("THREAD_DELETE", data, "100").expect("parse");
    assert_eq!(events.len(), 1);
    match &events[0] {
        ClientEvent::ChannelUpdated(ch) => {
            assert_eq!(ch.id, "88885");
            assert_eq!(ch.channel_type, ChannelType::Thread);
            // Tombstone has archived = true.
            let meta = ch.thread_metadata.as_ref().expect("tombstone has thread_metadata");
            assert!(meta.archived);
            assert!(meta.locked);
        }
        other => panic!("expected ChannelUpdated tombstone, got {other:?}"),
    }
}

/// THREAD_LIST_SYNC emits one ChannelUpdated per thread in the list.
#[test]
fn gateway_thread_list_sync_emits_per_thread() {
    let data = r#"{
        "guild_id": "100",
        "channel_ids": ["200", "201"],
        "threads": [
            {
                "id": "88886",
                "name": "sync-thread-a",
                "type": 11,
                "guild_id": "100",
                "parent_id": "200",
                "thread_metadata": {
                    "archived": false,
                    "auto_archive_duration": 1440,
                    "locked": false
                },
                "message_count": 2,
                "member_count": 1
            },
            {
                "id": "88887",
                "name": "sync-thread-b",
                "type": 11,
                "guild_id": "100",
                "parent_id": "201",
                "thread_metadata": {
                    "archived": false,
                    "auto_archive_duration": 4320,
                    "locked": false
                },
                "message_count": 7,
                "member_count": 4
            }
        ]
    }"#;
    let events = gateway_events_from_json("THREAD_LIST_SYNC", data, "100").expect("parse");
    assert_eq!(events.len(), 2);
    let ids: Vec<&str> = events.iter().map(|e| match e {
        ClientEvent::ChannelUpdated(ch) => ch.id.as_str(),
        _ => panic!("expected ChannelUpdated"),
    }).collect();
    assert!(ids.contains(&"88886"));
    assert!(ids.contains(&"88887"));
}

/// Unknown gateway event names produce no events (graceful no-op).
#[test]
fn gateway_unknown_event_returns_empty() {
    let events = gateway_events_from_json("MESSAGE_REACTION_ADD", r#"{}"#, "100").expect("parse");
    assert!(events.is_empty());
}
