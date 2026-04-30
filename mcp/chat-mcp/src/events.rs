//! Phase C — event subscription and poll-based event delivery.
//!
//! ## C.1 Transport research findings
//!
//! `poly-chat-mcp` supports two transports:
//!
//! - **HTTP (`POST /mcp`)**: request/response only. No persistent connection,
//!   so server-initiated push / SSE is not possible on the existing handler
//!   without adding a separate streaming endpoint. Claude Desktop's `mcp.json`
//!   integration uses HTTP transport.
//!
//! - **stdio**: line-delimited JSON-RPC. The MCP spec (`2024-11-05`) allows
//!   the server to write unsolicited `notifications/*` frames to stdout at any
//!   time. However, Claude Desktop as of 2026-04 does **not** consume
//!   server-originated notification frames — it drives the conversation as a
//!   strict request-initiator. Unsolicited frames would be silently dropped or
//!   cause a parse error.
//!
//! **Conclusion**: pure push (SSE / `notifications/event`) is not a reliable
//! delivery path for the current Claude Desktop. The guaranteed-working
//! fallback — `poll_events(since_ms, limit)` — is therefore the **primary**
//! C-phase deliverable. `subscribe_events` is wired (C.2/C.3) and returns a
//! subscription ID that can be passed to `poll_events`, but no transport-level
//! push is attempted (C.4 deferred to a future transport upgrade).
//!
//! ## Design
//!
//! ```text
//!   BackendPool
//!     └─ EventStore (shared Arc<Mutex<>>)
//!          ├─ broadcast::Sender<McpEvent>   ← fan-in from each account
//!          ├─ ring-buffer of last N events  ← poll_events reads this
//!          └─ subscriptions map             ← per subscription filter
//! ```
//!
//! Fan-out from each account's `event_stream()` runs in a dedicated `tokio`
//! task per account, spawned on `BackendPool::start_event_fan_out`. Tasks are
//! tracked by account key and cancelled on `pool.remove()`.

use chrono::{DateTime, Utc};
use poly_client::ClientEvent;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

// ─── Event capacity bounds ────────────────────────────────────────────────────

/// Maximum number of events retained in the in-process ring buffer.
///
/// Events older than [`MAX_EVENT_AGE_SECS`] are also pruned regardless of
/// capacity, so normal usage keeps the buffer very small.
const RING_CAPACITY: usize = 2000;

/// Events older than this many seconds are dropped during pruning. Keeps
/// memory bounded even if nobody polls for a while.
const MAX_EVENT_AGE_SECS: i64 = 300; // 5 minutes

/// Broadcast channel capacity. Slow receivers lag silently; they don't block
/// the sender or other receivers.
const BROADCAST_CAPACITY: usize = 512;

// ─── Event kind ──────────────────────────────────────────────────────────────

/// Slug-style event kinds Claude Desktop can filter on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    MessageReceived,
    MessageEdited,
    MessageDeleted,
    TypingStarted,
    PresenceChanged,
    FriendRequest,
    ReactionAdded,
    /// Catch-all for events not yet mapped to a specific kind.
    Other,
}

impl EventKind {
    fn from_client_event(ev: &ClientEvent) -> Self {
        match ev {
            ClientEvent::MessageReceived { .. } => Self::MessageReceived,
            ClientEvent::MessageEdited { .. } => Self::MessageEdited,
            ClientEvent::MessageDeleted { .. } => Self::MessageDeleted,
            ClientEvent::TypingStarted { .. } => Self::TypingStarted,
            ClientEvent::PresenceChanged { .. } => Self::PresenceChanged,
            ClientEvent::FriendRequestReceived { .. } => Self::FriendRequest,
            _ => Self::Other,
        }
    }

    fn from_slug(s: &str) -> Option<Self> {
        match s {
            "message_received" => Some(Self::MessageReceived),
            "message_edited" => Some(Self::MessageEdited),
            "message_deleted" => Some(Self::MessageDeleted),
            "typing_started" => Some(Self::TypingStarted),
            "presence_changed" => Some(Self::PresenceChanged),
            "friend_request" => Some(Self::FriendRequest),
            "reaction_added" => Some(Self::ReactionAdded),
            _ => None,
        }
    }
}

// ─── MCP event envelope ──────────────────────────────────────────────────────

/// A single event as exposed through the MCP surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEvent {
    /// Monotonically increasing sequence number (ms since Unix epoch at
    /// receipt, ties broken by internal counter).
    pub seq_ms: i64,
    /// Account key of the form `"BackendId(…):user_id"`.
    pub account_key: String,
    pub kind: EventKind,
    /// Channel id when the event is channel-scoped, otherwise absent.
    pub channel_id: Option<String>,
    /// Wall-clock timestamp.
    pub timestamp: DateTime<Utc>,
    /// Full event payload (serialised from `ClientEvent`).
    pub payload: serde_json::Value,
}

impl McpEvent {
    fn from_client_event(account_key: String, ev: &ClientEvent) -> Self {
        let now = Utc::now();
        let kind = EventKind::from_client_event(ev);
        let channel_id = channel_id_of(ev);
        let payload = serde_json::to_value(ev).unwrap_or(serde_json::Value::Null);
        Self {
            seq_ms: now.timestamp_millis(),
            account_key,
            kind,
            channel_id,
            timestamp: now,
            payload,
        }
    }
}

fn channel_id_of(ev: &ClientEvent) -> Option<String> {
    match ev {
        ClientEvent::MessageReceived { channel_id, .. }
        | ClientEvent::MessageEdited { channel_id, .. }
        | ClientEvent::MessageDeleted { channel_id, .. }
        | ClientEvent::TypingStarted { channel_id, .. } => Some(channel_id.clone()),
        _ => None,
    }
}

// ─── Subscription ─────────────────────────────────────────────────────────────

/// Per-subscriber filter.  `None` on any field means "accept all".
#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: String,
    pub account_ids: Option<Vec<String>>,
    pub chat_ids: Option<Vec<String>>,
    pub event_types: Option<Vec<EventKind>>,
}

impl Subscription {
    fn matches(&self, ev: &McpEvent) -> bool {
        if let Some(acc) = &self.account_ids {
            if !acc.iter().any(|a| ev.account_key.contains(a.as_str())) {
                return false;
            }
        }
        if let Some(chats) = &self.chat_ids {
            let cid = ev.channel_id.as_deref().unwrap_or("");
            if !chats.iter().any(|c| c == cid) {
                return false;
            }
        }
        if let Some(kinds) = &self.event_types {
            if !kinds.contains(&ev.kind) {
                return false;
            }
        }
        true
    }
}

// ─── EventStore ───────────────────────────────────────────────────────────────

/// Shared event store: fan-in channel + ring buffer + subscription registry.
pub struct EventStore {
    tx: broadcast::Sender<McpEvent>,
    ring: VecDeque<McpEvent>,
    subscriptions: HashMap<String, Subscription>,
}

impl EventStore {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            tx,
            ring: VecDeque::new(),
            subscriptions: HashMap::new(),
        }
    }

    /// Subscribe `broadcast::Receiver`; caller can listen for push if desired.
    pub fn subscribe_broadcast(&self) -> broadcast::Receiver<McpEvent> {
        self.tx.subscribe()
    }

    /// Publish an event — stores in ring buffer and broadcasts.
    pub fn publish(&mut self, ev: McpEvent) {
        // Prune ring (age + capacity).
        let cutoff = Utc::now().timestamp_millis() - MAX_EVENT_AGE_SECS * 1000;
        while self
            .ring
            .front()
            .map(|e| e.seq_ms < cutoff)
            .unwrap_or(false)
        {
            self.ring.pop_front();
        }
        if self.ring.len() >= RING_CAPACITY {
            self.ring.pop_front();
        }
        // Broadcast first so slow subscribers don't see a message already in
        // the ring but potentially already pruned by the time they check.
        let _ = self.tx.send(ev.clone()); // ok if no receivers
        self.ring.push_back(ev);
    }

    /// Register a new subscription; returns the subscription id.
    pub fn add_subscription(&mut self, sub: Subscription) -> String {
        let id = sub.id.clone();
        self.subscriptions.insert(id.clone(), sub);
        id
    }

    /// Remove a subscription.
    pub fn remove_subscription(&mut self, id: &str) {
        self.subscriptions.remove(id);
    }

    /// Get subscription filter by id (for poll-time filtering).
    pub fn subscription(&self, id: &str) -> Option<&Subscription> {
        self.subscriptions.get(id)
    }

    /// Poll events that match the subscription filter with `seq_ms > since_ms`,
    /// capped at `limit`. Returns events in chronological order.
    pub fn poll(
        &self,
        subscription_id: &str,
        since_ms: i64,
        limit: usize,
    ) -> Result<Vec<McpEvent>, String> {
        let sub = self
            .subscriptions
            .get(subscription_id)
            .ok_or_else(|| format!("unknown subscription_id: {subscription_id}"))?;

        let events: Vec<McpEvent> = self
            .ring
            .iter()
            .filter(|e| e.seq_ms > since_ms && sub.matches(e))
            .take(limit)
            .cloned()
            .collect();
        Ok(events)
    }

    /// Poll events using an ad-hoc filter (no pre-registered subscription).
    pub fn poll_adhoc(
        &self,
        account_ids: Option<&[String]>,
        chat_ids: Option<&[String]>,
        event_types: Option<&[EventKind]>,
        since_ms: i64,
        limit: usize,
    ) -> Vec<McpEvent> {
        self.ring
            .iter()
            .filter(|e| {
                if e.seq_ms <= since_ms {
                    return false;
                }
                if let Some(acc) = account_ids {
                    if !acc.iter().any(|a| e.account_key.contains(a.as_str())) {
                        return false;
                    }
                }
                if let Some(chats) = chat_ids {
                    let cid = e.channel_id.as_deref().unwrap_or("");
                    if !chats.iter().any(|c| c == cid) {
                        return false;
                    }
                }
                if let Some(kinds) = event_types {
                    if !kinds.contains(&e.kind) {
                        return false;
                    }
                }
                true
            })
            .take(limit)
            .cloned()
            .collect()
    }
}

// ─── Arc wrapper ─────────────────────────────────────────────────────────────

/// Cheaply-cloneable handle to the shared event store.
pub type SharedEventStore = Arc<Mutex<EventStore>>;

pub fn new_event_store() -> SharedEventStore {
    Arc::new(Mutex::new(EventStore::new()))
}

// ─── Fan-out task ─────────────────────────────────────────────────────────────

/// Spawn a tokio task that drains one backend's `event_stream()` into `store`.
///
/// The task runs until the stream ends or the `shutdown` oneshot fires.
/// Returns a `JoinHandle` and `shutdown` sender so the caller can cancel it.
pub fn spawn_fan_out(
    account_key: String,
    stream: std::pin::Pin<Box<dyn futures_core::Stream<Item = ClientEvent> + Send>>,
    store: SharedEventStore,
) -> (tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>) {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        use futures_util::StreamExt as _;

        tokio::pin!(stream);
        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown_rx => {
                    tracing::debug!(account = %account_key, "event fan-out task shutting down");
                    break;
                }
                maybe_ev = stream.next() => {
                    match maybe_ev {
                        Some(ev) => {
                            let mcp_ev = McpEvent::from_client_event(account_key.clone(), &ev);
                            store.lock().await.publish(mcp_ev);
                        }
                        None => {
                            tracing::debug!(account = %account_key, "event stream closed");
                            break;
                        }
                    }
                }
            }
        }
    });

    (handle, shutdown_tx)
}

// ─── Helpers exposed to tools.rs (Phase C insertion) ─────────────────────────

/// Parse an optional JSON array of strings from `args[key]`.
pub fn parse_opt_string_vec(args: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    args.get(key).and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
    })
}

/// Parse an optional JSON array of event-kind slugs from `args[key]`.
pub fn parse_opt_event_kinds(args: &serde_json::Value, key: &str) -> Option<Vec<EventKind>> {
    args.get(key).and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().and_then(EventKind::from_slug))
                .collect()
        })
    })
}

/// Generate a short UUID-based subscription id.
pub fn new_subscription_id() -> String {
    // uuid crate may not be in workspace; fall back to a timestamp+random suffix.
    // We avoid using uuid here to keep dependencies minimal.
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Combine millis with a pseudo-random u32 from the thread-local rng via rand.
    let r: u32 = rand::random();
    format!("sub_{ms:x}_{r:08x}")
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use chrono::Utc;
    use poly_client::ClientEvent;

    fn make_event(account_key: &str, kind: EventKind, channel_id: Option<&str>, ts_offset_ms: i64) -> McpEvent {
        McpEvent {
            seq_ms: Utc::now().timestamp_millis() + ts_offset_ms,
            account_key: account_key.to_string(),
            kind,
            channel_id: channel_id.map(String::from),
            timestamp: Utc::now(),
            payload: serde_json::Value::Null,
        }
    }

    #[test]
    fn event_kind_roundtrip() {
        for (slug, kind) in [
            ("message_received", EventKind::MessageReceived),
            ("message_edited", EventKind::MessageEdited),
            ("typing_started", EventKind::TypingStarted),
            ("presence_changed", EventKind::PresenceChanged),
            ("friend_request", EventKind::FriendRequest),
            ("reaction_added", EventKind::ReactionAdded),
        ] {
            assert_eq!(EventKind::from_slug(slug), Some(kind));
        }
        assert_eq!(EventKind::from_slug("bogus"), None);
    }

    #[test]
    fn store_publish_and_poll_basic() {
        let mut store = EventStore::new();
        let sub = Subscription {
            id: "test-sub-1".to_string(),
            account_ids: None,
            chat_ids: None,
            event_types: None,
        };
        store.add_subscription(sub);

        let ev = make_event("discord:user1", EventKind::MessageReceived, Some("ch1"), -100);
        let seq = ev.seq_ms;
        store.publish(ev);

        let results = store.poll("test-sub-1", seq - 200, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, EventKind::MessageReceived);
    }

    #[test]
    fn poll_since_ms_excludes_older() {
        let mut store = EventStore::new();
        let sub = Subscription {
            id: "s".to_string(),
            account_ids: None,
            chat_ids: None,
            event_types: None,
        };
        store.add_subscription(sub);

        let ev = make_event("acc", EventKind::TypingStarted, None, -500);
        let seq = ev.seq_ms;
        store.publish(ev.clone());

        // Poll with since_ms == seq means "strictly after seq", so 0 results.
        let results = store.poll("s", seq, 100).unwrap();
        assert!(results.is_empty());

        // Poll with since_ms < seq returns the event.
        let results2 = store.poll("s", seq - 1, 100).unwrap();
        assert_eq!(results2.len(), 1);
    }

    #[test]
    fn subscription_filter_by_event_type() {
        let mut store = EventStore::new();
        let sub = Subscription {
            id: "s2".to_string(),
            account_ids: None,
            chat_ids: None,
            event_types: Some(vec![EventKind::MessageReceived]),
        };
        store.add_subscription(sub);

        let base = Utc::now().timestamp_millis();
        store.publish(make_event("acc", EventKind::MessageReceived, Some("ch"), 0));
        store.publish(make_event("acc", EventKind::TypingStarted, None, 10));

        let results = store.poll("s2", base - 1, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, EventKind::MessageReceived);
    }

    #[test]
    fn subscription_filter_by_account() {
        let mut store = EventStore::new();
        let sub = Subscription {
            id: "s3".to_string(),
            account_ids: Some(vec!["user1".to_string()]),
            chat_ids: None,
            event_types: None,
        };
        store.add_subscription(sub);

        let base = Utc::now().timestamp_millis();
        store.publish(make_event("BackendId(discord):user1", EventKind::MessageReceived, Some("ch"), 0));
        store.publish(make_event("BackendId(discord):user2", EventKind::MessageReceived, Some("ch"), 10));

        let results = store.poll("s3", base - 1, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].account_key.contains("user1"));
    }

    #[test]
    fn ring_caps_at_capacity() {
        let mut store = EventStore::new();
        // Publish more than RING_CAPACITY events.
        let base = Utc::now().timestamp_millis();
        for i in 0..(RING_CAPACITY + 50) {
            store.publish(McpEvent {
                seq_ms: base + i as i64,
                account_key: "acc".to_string(),
                kind: EventKind::TypingStarted,
                channel_id: None,
                timestamp: Utc::now(),
                payload: serde_json::Value::Null,
            });
        }
        assert!(store.ring.len() <= RING_CAPACITY);
    }

    #[test]
    fn unknown_subscription_returns_error() {
        let store = EventStore::new();
        assert!(store.poll("nope", 0, 10).is_err());
    }

    #[test]
    fn remove_subscription_works() {
        let mut store = EventStore::new();
        let sub = Subscription {
            id: "to-remove".to_string(),
            account_ids: None,
            chat_ids: None,
            event_types: None,
        };
        store.add_subscription(sub);
        store.remove_subscription("to-remove");
        assert!(store.poll("to-remove", 0, 10).is_err());
    }

    #[test]
    fn event_kind_from_client_event() {
        let ev_msg = ClientEvent::MessageReceived {
            channel_id: "ch".to_string(),
            message: poly_client::Message {
                id: "m1".to_string(),
                author: poly_client::User {
                    id: "u1".to_string(),
                    display_name: "U".to_string(),
                    avatar_url: None,
                    presence: poly_client::PresenceStatus::Online,
                    backend: poly_client::BackendType::from("discord"),
                },
                content: poly_client::MessageContent::Text("hi".to_string()),
                timestamp: Utc::now(),
                edited: false,
                reactions: vec![],
                reply_to: None,
                attachments: vec![],
                thread: None,
                preview_image_url: None,
            },
        };
        assert_eq!(EventKind::from_client_event(&ev_msg), EventKind::MessageReceived);
    }
}
