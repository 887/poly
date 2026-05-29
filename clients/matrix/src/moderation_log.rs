//! Moderation-log synthesiser (SOLID audit Phase D.1).
//!
//! Matrix has no native audit log. This module synthesises one by walking
//! recent timeline events on each child room of a Space and projecting
//! `m.room.member` transitions + `m.room.redaction` events into
//! [`ModerationLogEntry`] rows the host UI already understands.
//!
//! ## Design choice — on-demand vs cached
//!
//! The plan sketches a "background indexer task + persistent log feed"
//! (~600 LoC). That's the right shape for a long-running session where we
//! want push notifications when a new ban/kick happens. The minimal cleanest
//! version, shipped here, is on-demand: each `get_moderation_log` call walks
//! the latest ~50 timeline events on every space-child room and projects
//! them. This trades freshness for simplicity:
//!
//! - No new state to keep coherent between sync loops and on-demand reads.
//! - No persistence schema migration.
//! - Bounded I/O: capped at one HTTP request per child room.
//! - Drawback: the log only sees events still in the per-room timeline
//!   window. Old bans drop off when newer events push them past the cap.
//!
//! When `get_moderation_log` proves hot enough to warrant a background
//! indexer, the sync loop can call back into this module with the projector
//! functions kept intact.
//!
//! ## Event projections
//!
//! `m.room.member` transitions:
//! - `join` → no entry (not a moderation action).
//! - `invite` → no entry.
//! - `leave` where sender == target → no entry (self-leave).
//! - `leave` where sender != target and `prev_content.membership == "join"`
//!   → `MemberKicked`.
//! - `leave` where sender != target and `prev_content.membership == "ban"`
//!   → `MemberUnbanned`.
//! - `ban` → `MemberBanned`.
//! - `knock` → no entry.
//!
//! `m.room.redaction`:
//! - Always → `MessageDeleted`, with `target_user_id` left `None` (the
//!   redacted event's sender is not in the redaction event itself).

use poly_client::{
    BackendType, ClientResult, ModerationAction, ModerationLogEntry, PresenceStatus, User,
};

use crate::api::{MemberEventContent, RedactionEventContent, RoomEvent, UnsignedData};
use crate::http::MatrixHttpClient;

/// How many recent events to scan per child room when synthesising the log.
const PER_ROOM_EVENT_CAP: u64 = 50;

/// Walk the space hierarchy and project member/redaction events into a
/// merged, time-sorted moderation log capped at `limit` entries.
pub async fn synthesize(
    http: &MatrixHttpClient,
    server_id: &str,
    limit: usize,
) -> ClientResult<Vec<ModerationLogEntry>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    // Enumerate child rooms of the space. Skip nested spaces — we only
    // synthesise moderation entries from leaf rooms where members live.
    let hierarchy = http.fetch_space_hierarchy(server_id).await?;
    let room_ids: Vec<String> = hierarchy
        .rooms
        .iter()
        .filter(|r| r.room_type.as_deref() != Some("m.space"))
        .map(|r| r.room_id.clone())
        .collect();

    if room_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch recent timeline events from each room in parallel. Each room
    // requires its own pagination token; we use a fresh `/sync?timeout=0`
    // to obtain a coherent `next_batch` to start backwards-pagination from.
    // Failures per room are swallowed so a single inaccessible room doesn't
    // blank the whole view.
    let next_batch = http
        .sync(None, Some(0))
        .await
        .map(|s| s.next_batch)
        .unwrap_or_default();

    if next_batch.is_empty() {
        return Ok(Vec::new());
    }

    let message_futures = room_ids.iter().map(|room_id| {
        let token = next_batch.clone();
        async move {
            http.fetch_messages(room_id, &token, "b", Some(PER_ROOM_EVENT_CAP))
                .await
                .ok()
                .map(|resp| (room_id.clone(), resp.chunk))
        }
    });
    let per_room: Vec<_> = futures::future::join_all(message_futures)
        .await
        .into_iter()
        .flatten()
        .collect();

    let mut entries: Vec<ModerationLogEntry> = Vec::new();
    for (room_id, events) in per_room {
        for event in &events {
            if let Some(entry) = project_event(event, &room_id) {
                entries.push(entry);
            }
        }
    }

    // Sort newest-first by the synthesised RFC3339 timestamp string. The
    // timestamps are derived from `origin_server_ts` and serialise in
    // lexicographically-comparable order, so descending string sort matches
    // descending time order.
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    entries.truncate(limit);
    Ok(entries)
}

/// Project a single timeline event onto a `ModerationLogEntry`, or `None`
/// if the event is not a moderation action (e.g. self-join, message send,
/// avatar change).
fn project_event(event: &RoomEvent, room_id: &str) -> Option<ModerationLogEntry> {
    match event.event_type.as_str() {
        "m.room.member" => project_member_event(event, room_id),
        "m.room.redaction" => project_redaction_event(event, room_id),
        _ => None,
    }
}

fn project_member_event(event: &RoomEvent, room_id: &str) -> Option<ModerationLogEntry> {
    let content: MemberEventContent = serde_json::from_value(event.content.clone()).ok()?;
    let sender = event.sender.as_deref()?;
    let target = event.state_key.as_deref()?;
    let event_id = event.event_id.as_deref()?;
    let ts = event.origin_server_ts?;

    let prev_membership = event
        .unsigned
        .as_ref()
        .and_then(|u| serde_json::from_value::<UnsignedData>(u.clone()).ok())
        .and_then(|u| u.prev_content)
        .map(|c| c.membership);

    let action = match (content.membership.as_str(), prev_membership.as_deref()) {
        ("ban", _) => ModerationAction::MemberBanned,
        ("leave", Some("ban")) if sender != target => ModerationAction::MemberUnbanned,
        ("leave", Some("join" | "invite")) if sender != target => ModerationAction::MemberKicked,
        // Self-leaves, joins, invites, knocks: not moderation actions.
        _ => return None,
    };

    Some(ModerationLogEntry {
        id: format!("matrix:{event_id}"),
        action,
        moderator: synth_user(sender),
        target_user_id: Some(target.to_string()),
        target_display_name: content.displayname.clone(),
        channel_id: Some(room_id.to_string()),
        message_id: None,
        reason: content.reason,
        timestamp: ts_to_rfc3339(ts),
    })
}

fn project_redaction_event(event: &RoomEvent, room_id: &str) -> Option<ModerationLogEntry> {
    let sender = event.sender.as_deref()?;
    let event_id = event.event_id.as_deref()?;
    let ts = event.origin_server_ts?;
    let redacts = event.redacts.clone();

    let reason: Option<String> = serde_json::from_value::<RedactionEventContent>(event.content.clone())
        .ok()
        .and_then(|c| c.reason);

    Some(ModerationLogEntry {
        id: format!("matrix:{event_id}"),
        action: ModerationAction::MessageDeleted,
        moderator: synth_user(sender),
        // Matrix redactions don't carry the original sender; the UI must
        // resolve target metadata from the redacted message itself.
        target_user_id: None,
        target_display_name: None,
        channel_id: Some(room_id.to_string()),
        message_id: redacts,
        reason,
        timestamp: ts_to_rfc3339(ts),
    })
}

/// Build a placeholder `User` for the moderator slot.
///
/// We only have the MXID at projection time. Resolving display names would
/// require a profile lookup per moderator, which inflates the on-demand
/// call's I/O budget. The host UI is expected to hydrate display names from
/// its own profile cache keyed by `id`.
fn synth_user(mxid: &str) -> User {
    User {
        id: mxid.to_string(),
        display_name: mxid.to_string(),
        avatar_url: None,
        presence: PresenceStatus::Unknown,
        backend: BackendType::from(crate::SLUG),
    }
}

fn ts_to_rfc3339(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(i64::try_from(ms).unwrap_or(i64::MAX))
        .unwrap_or_default()
        .to_rfc3339()
}

// lint-allow-unused: tests panic on bad fixture; clippy noise unhelpful here.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_member_event(
        sender: &str,
        target: &str,
        membership: &str,
        prev_membership: Option<&str>,
        ts: u64,
    ) -> RoomEvent {
        let unsigned = prev_membership.map(|m| {
            json!({ "prev_content": { "membership": m } })
        });
        RoomEvent {
            event_type: "m.room.member".to_string(),
            event_id: Some(format!("$evt:{ts}")),
            sender: Some(sender.to_string()),
            origin_server_ts: Some(ts),
            state_key: Some(target.to_string()),
            content: json!({ "membership": membership }),
            redacts: None,
            unsigned,
        }
    }

    fn make_redaction(sender: &str, redacts: &str, reason: Option<&str>, ts: u64) -> RoomEvent {
        let mut content = serde_json::Map::new();
        if let Some(r) = reason {
            content.insert("reason".to_string(), json!(r));
        }
        RoomEvent {
            event_type: "m.room.redaction".to_string(),
            event_id: Some(format!("$red:{ts}")),
            sender: Some(sender.to_string()),
            origin_server_ts: Some(ts),
            state_key: None,
            content: serde_json::Value::Object(content),
            redacts: Some(redacts.to_string()),
            unsigned: None,
        }
    }

    #[test]
    fn self_join_is_not_a_moderation_event() {
        let ev = make_member_event("@alice:hs", "@alice:hs", "join", None, 1000);
        assert!(project_event(&ev, "!room:hs").is_none());
    }

    #[test]
    fn self_leave_is_not_a_moderation_event() {
        let ev = make_member_event("@alice:hs", "@alice:hs", "leave", Some("join"), 1000);
        assert!(project_event(&ev, "!room:hs").is_none());
    }

    #[test]
    fn kick_projects_to_member_kicked() {
        let ev = make_member_event("@mod:hs", "@alice:hs", "leave", Some("join"), 2000);
        let entry = project_event(&ev, "!room:hs").unwrap();
        assert!(matches!(entry.action, ModerationAction::MemberKicked));
        assert_eq!(entry.moderator.id, "@mod:hs");
        assert_eq!(entry.target_user_id.as_deref(), Some("@alice:hs"));
    }

    #[test]
    fn ban_projects_to_member_banned() {
        let ev = make_member_event("@mod:hs", "@bob:hs", "ban", Some("join"), 3000);
        let entry = project_event(&ev, "!room:hs").unwrap();
        assert!(matches!(entry.action, ModerationAction::MemberBanned));
    }

    #[test]
    fn unban_projects_to_member_unbanned() {
        let ev = make_member_event("@mod:hs", "@bob:hs", "leave", Some("ban"), 4000);
        let entry = project_event(&ev, "!room:hs").unwrap();
        assert!(matches!(entry.action, ModerationAction::MemberUnbanned));
    }

    #[test]
    fn redaction_projects_to_message_deleted() {
        let ev = make_redaction("@mod:hs", "$victim_evt:hs", Some("spam"), 5000);
        let entry = project_event(&ev, "!room:hs").unwrap();
        assert!(matches!(entry.action, ModerationAction::MessageDeleted));
        assert_eq!(entry.message_id.as_deref(), Some("$victim_evt:hs"));
        assert_eq!(entry.reason.as_deref(), Some("spam"));
        // Target user is unknown from a redaction event alone.
        assert!(entry.target_user_id.is_none());
    }

    #[test]
    fn invite_is_not_a_moderation_event() {
        let ev = make_member_event("@inviter:hs", "@invitee:hs", "invite", None, 6000);
        assert!(project_event(&ev, "!room:hs").is_none());
    }
}
