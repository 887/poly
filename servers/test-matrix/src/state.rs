//! In-memory state for the mock Matrix homeserver.

use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use poly_test_common::{AuthState, EventBus, HeaderInspectBuffer};
use std::sync::Arc as StdArc;

/// Events broadcast to `/sync` long-poll waiters.
#[derive(Clone, Debug)]
pub enum MatrixEvent {
    /// New timeline event in a room.
    Timeline {
        room_id: String,
        event: serde_json::Value,
    },
    /// Typing indicator changed.
    Typing {
        room_id: String,
        user_ids: Vec<String>,
    },
    /// User presence changed.
    Presence { user_id: String, presence: String },
}

/// All mock Matrix state.
#[derive(Clone)]
pub struct MatrixState {
    pub auth: AuthState,
    pub users: DashMap<String, UserProfile>,
    pub rooms: DashMap<String, Room>,
    /// room_id → Vec<timeline events> (append-only)
    pub timelines: DashMap<String, Vec<serde_json::Value>>,
    /// user_id → account data (type → value)
    pub account_data: DashMap<String, DashMap<String, serde_json::Value>>,
    pub events: EventBus<MatrixEvent>,
    /// Global event counter for sync tokens and event IDs.
    event_counter: std::sync::Arc<AtomicU64>,
    /// room_id → power_levels content (m.room.power_levels state event body).
    ///
    /// Absent = use Matrix spec defaults (ban/kick/redact/state_default = 50,
    /// events_default = 0, users_default = 0).
    pub power_levels: DashMap<String, serde_json::Value>,
    /// room_id → Vec<banned user_ids>  (tracked separately from state_events for
    /// easy membership=ban filter in the /members?membership=ban route).
    pub banned_members: DashMap<String, Vec<BannedEntry>>,
    /// Ring buffer of recent inbound request headers (Phase E inspection endpoint).
    pub inspect: StdArc<HeaderInspectBuffer>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub displayname: String,
    pub avatar_url: Option<String>,
    pub password: String,
    pub device_id: String,
}

/// A banned member record.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BannedEntry {
    pub user_id: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Room {
    pub room_id: String,
    pub name: String,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub members: Vec<String>,
    pub is_space: bool,
    pub parent_space_id: Option<String>,
    /// Room state events (m.room.create, m.room.name, m.room.member, etc.)
    pub state_events: Vec<serde_json::Value>,
}

impl Default for MatrixState {
    fn default() -> Self {
        Self::new()
    }
}

impl poly_test_common::BackendHarness for MatrixState {
    const BACKEND: &'static str = "matrix";

    fn new(auth: poly_test_common::AuthState) -> Self {
        let mut s = MatrixState::new();
        s.auth = auth;
        s
    }

    fn seed(&self) { MatrixState::seed(self); }
    fn reset(&self) { MatrixState::reset(self); }
    // reseed() uses the default: reset() + seed()

    fn routes(state: std::sync::Arc<Self>) -> axum::Router<std::sync::Arc<Self>> {
        crate::routes_only(state)
    }

    fn inspect_buf(&self) -> std::sync::Arc<poly_test_common::HeaderInspectBuffer> {
        StdArc::clone(&self.inspect)
    }
}

impl MatrixState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            rooms: DashMap::new(),
            timelines: DashMap::new(),
            account_data: DashMap::new(),
            events: EventBus::new(),
            event_counter: std::sync::Arc::new(AtomicU64::new(1)),
            power_levels: DashMap::new(),
            banned_members: DashMap::new(),
            inspect: StdArc::new(HeaderInspectBuffer::new()),
        }
    }

    /// Get next event ID like "$evt1", "$evt2", etc.
    #[must_use]
    pub fn next_event_id(&self) -> String {
        let n = self.event_counter.fetch_add(1, Ordering::Relaxed);
        format!("$evt{n}")
    }

    /// Get current sync token (stringified counter).
    #[must_use]
    pub fn sync_token(&self) -> String {
        self.event_counter.load(Ordering::Relaxed).to_string()
    }

    /// Seed demo data: Owl + Axolotl, 2 spaces, rooms, messages, DMs.
    // lint-allow-unused: serde_json::json! macros use bare integer literals for power-level values
    #[allow(clippy::default_numeric_fallback)]
    pub fn seed(&self) {
        if !self.users.is_empty() {
            tracing::info!("Matrix demo data already seeded, skipping");
            return;
        }
        tracing::info!("seeding Matrix demo data");

        let owl_id = "@owl:localhost".to_string();
        let axolotl_id = "@axolotl:localhost".to_string();
        let cat_id = "@cat:localhost".to_string();
        let dog_id = "@dog:localhost".to_string();

        // Users — mxc URIs map to /_matrix/media/v3/thumbnail/localhost/{media_id}
        self.users.insert(
            owl_id.clone(),
            UserProfile {
                user_id: owl_id.clone(),
                displayname: "Owl".into(),
                avatar_url: Some("mxc://localhost/owl_avatar".into()),
                password: "testpass123".into(),
                device_id: "OWLDEVICE01".into(),
            },
        );
        self.users.insert(
            axolotl_id.clone(),
            UserProfile {
                user_id: axolotl_id.clone(),
                displayname: "Axolotl".into(),
                avatar_url: Some("mxc://localhost/axolotl_avatar".into()),
                password: "testpass123".into(),
                device_id: "AXOLDEVICE01".into(),
            },
        );
        self.users.insert(
            cat_id.clone(),
            UserProfile {
                user_id: cat_id.clone(),
                displayname: "Cat".into(),
                avatar_url: Some("mxc://localhost/cat_avatar".into()),
                password: "testpass123".into(),
                device_id: "CATDEVICE01".into(),
            },
        );
        self.users.insert(
            dog_id.clone(),
            UserProfile {
                user_id: dog_id.clone(),
                displayname: "Dog".into(),
                avatar_url: Some("mxc://localhost/dog_avatar".into()),
                password: "testpass123".into(),
                device_id: "DOGDEVICE01".into(),
            },
        );

        // Space 1: The Hollow Tree — Owl + Axolotl + Cat + Dog all member
        let space1_id = "!space1:localhost".to_string();
        let gen1_id = "!general1:localhost".to_string();
        let random1_id = "!random1:localhost".to_string();
        let announce1_id = "!announce1:localhost".to_string();

        self.create_room(&space1_id, "The Hollow Tree", None, true, None, &[&owl_id, &axolotl_id, &cat_id, &dog_id]);
        self.create_room(&gen1_id, "general", Some("General discussion"), false, Some(&space1_id), &[&owl_id, &axolotl_id, &cat_id, &dog_id]);
        self.create_room(&random1_id, "random", Some("Off-topic chat"), false, Some(&space1_id), &[&owl_id, &axolotl_id, &cat_id, &dog_id]);
        self.create_room(&announce1_id, "announcements", Some("Important updates"), false, Some(&space1_id), &[&owl_id, &axolotl_id]);

        // Power levels for Space 1: Owl is admin (100), Axolotl is moderator (50).
        self.power_levels.insert(
            space1_id.clone(),
            serde_json::json!({
                "ban": 50,
                "kick": 50,
                "redact": 50,
                "state_default": 50,
                "events_default": 0,
                "users_default": 0,
                "users": {
                    "@owl:localhost": 100,
                    "@axolotl:localhost": 50,
                }
            }),
        );

        // Add avatar to Space 1
        if let Some(mut room) = self.rooms.get_mut(&space1_id) {
            room.state_events.push(serde_json::json!({
                "type": "m.room.avatar",
                "state_key": "",
                "content": { "url": "mxc://localhost/hollow_tree_avatar" },
                "sender": &*owl_id,
                "event_id": self.next_event_id(),
            }));
        }

        // Test Arena room (clean state for back-and-forth tests)
        let arena_id = "!test_arena:localhost".to_string();
        self.create_room(&arena_id, "test-arena", Some("Dedicated back-and-forth test room"), false, Some(&space1_id), &[&owl_id, &axolotl_id]);

        // Space 2: Neon Reef
        let space2_id = "!space2:localhost".to_string();
        let gen2_id = "!general2:localhost".to_string();
        let memes_id = "!memes:localhost".to_string();
        let music_id = "!music:localhost".to_string();

        self.create_room(&space2_id, "Neon Reef", None, true, None, &[&owl_id, &axolotl_id]);
        self.create_room(&gen2_id, "general", Some("Main chat"), false, Some(&space2_id), &[&owl_id, &axolotl_id]);
        self.create_room(&memes_id, "memes", Some("Funny stuff"), false, Some(&space2_id), &[&owl_id, &axolotl_id]);
        self.create_room(&music_id, "music", Some("Share tunes"), false, Some(&space2_id), &[&owl_id, &axolotl_id]);

        // Power levels for Space 2: Axolotl is admin (100), Owl is moderator (50).
        self.power_levels.insert(
            space2_id.clone(),
            serde_json::json!({
                "ban": 50,
                "kick": 50,
                "redact": 50,
                "state_default": 50,
                "events_default": 0,
                "users_default": 0,
                "users": {
                    "@axolotl:localhost": 100,
                    "@owl:localhost": 50,
                }
            }),
        );

        // Add avatar to Space 2
        if let Some(mut room) = self.rooms.get_mut(&space2_id) {
            room.state_events.push(serde_json::json!({
                "type": "m.room.avatar",
                "state_key": "",
                "content": { "url": "mxc://localhost/neon_reef_avatar" },
                "sender": &*axolotl_id,
                "event_id": self.next_event_id(),
            }));
        }

        // DM room
        let dm_id = "!dm1:localhost".to_string();
        self.create_room(&dm_id, "DM", None, false, None, &[&owl_id, &axolotl_id]);

        // m.direct account data
        let owl_account = DashMap::new();
        owl_account.insert(
            "m.direct".to_string(),
            serde_json::json!({ axolotl_id.clone(): [dm_id.clone()] }),
        );
        self.account_data.insert(owl_id.clone(), owl_account);

        let axolotl_account = DashMap::new();
        axolotl_account.insert(
            "m.direct".to_string(),
            serde_json::json!({ owl_id.clone(): [dm_id.clone()] }),
        );
        self.account_data.insert(axolotl_id.clone(), axolotl_account);

        // Seed messages in general channels
        self.add_message(&gen1_id, &owl_id, "Hoot! Welcome to The Hollow Tree 🌳");
        self.add_message(&gen1_id, &axolotl_id, "Thanks Owl! This place is cozy 🪸");
        self.add_message(&gen1_id, &owl_id, "I've been reading about nocturnal ecosystems.");
        self.add_message(&gen1_id, &axolotl_id, "That sounds fascinating! Tell me more?");
        self.add_message(&gen1_id, &owl_id, "Did you know owls can rotate their heads 270 degrees?");

        self.add_message(&gen2_id, &axolotl_id, "Hey Owl, check out this reef! 🐠");
        self.add_message(&gen2_id, &owl_id, "The bioluminescence is incredible.");
        self.add_message(&gen2_id, &axolotl_id, "Right?! I feel right at home underwater.");

        self.add_message(&dm_id, &owl_id, "Hey Axolotl, want to grab lunch?");
        self.add_message(&dm_id, &axolotl_id, "Sure! How about algae wraps? 😄");
        self.add_message(&dm_id, &owl_id, "I was thinking more like field mice but we can compromise.");
    }

    /// Wipe all data to empty state.
    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.rooms.clear();
        self.timelines.clear();
        self.account_data.clear();
        self.power_levels.clear();
        self.banned_members.clear();
        self.inspect.clear();
        tracing::info!("reset Matrix state to empty");
    }

    /// Helper: create a room with state events.
    #[allow(clippy::too_many_arguments)]
    fn create_room(
        &self,
        room_id: &str,
        name: &str,
        topic: Option<&str>,
        is_space: bool,
        parent_space_id: Option<&str>,
        members: &[&str],
    ) {
        let mut state_events = vec![];

        // m.room.create
        let mut create_content = serde_json::json!({ "creator": members.first().unwrap_or(&"") });
        if is_space
            && let Some(obj) = create_content.as_object_mut()
        {
            obj.insert("type".to_string(), serde_json::json!("m.space"));
        }
        state_events.push(serde_json::json!({
            "type": "m.room.create",
            "state_key": "",
            "content": create_content,
            "sender": members.first().unwrap_or(&""),
            "event_id": self.next_event_id(),
        }));

        // m.room.name
        state_events.push(serde_json::json!({
            "type": "m.room.name",
            "state_key": "",
            "content": { "name": name },
            "sender": members.first().unwrap_or(&""),
            "event_id": self.next_event_id(),
        }));

        // m.room.topic
        if let Some(t) = topic {
            state_events.push(serde_json::json!({
                "type": "m.room.topic",
                "state_key": "",
                "content": { "topic": t },
                "sender": members.first().unwrap_or(&""),
                "event_id": self.next_event_id(),
            }));
        }

        // m.room.member for each member
        for member in members {
            let user = self.users.get(*member);
            let displayname = user.as_ref().map(|u| u.displayname.clone());
            let avatar_url = user.as_ref().and_then(|u| u.avatar_url.clone());

            state_events.push(serde_json::json!({
                "type": "m.room.member",
                "state_key": member,
                "content": {
                    "membership": "join",
                    "displayname": displayname,
                    "avatar_url": avatar_url,
                },
                "sender": member,
                "event_id": self.next_event_id(),
            }));
        }

        // m.space.child (if this room belongs to a space)
        if let Some(space_id) = parent_space_id
            && let Some(mut space) = self.rooms.get_mut(space_id)
        {
            space.state_events.push(serde_json::json!({
                "type": "m.space.child",
                "state_key": room_id,
                "content": { "via": ["localhost"] },
                "sender": members.first().unwrap_or(&""),
                "event_id": self.next_event_id(),
            }));
        }

        self.rooms.insert(
            room_id.to_string(),
            Room {
                room_id: room_id.to_string(),
                name: name.to_string(),
                topic: topic.map(std::string::ToString::to_string),
                avatar_url: None,
                members: members.iter().map(|s| (*s).to_string()).collect(),
                is_space,
                parent_space_id: parent_space_id.map(std::string::ToString::to_string),
                state_events,
            },
        );

        self.timelines
            .insert(room_id.to_string(), Vec::new());
    }

    /// Helper: add a message to a room's timeline.
    fn add_message(&self, room_id: &str, sender: &str, body: &str) {
        let event_id = self.next_event_id();
        let event = serde_json::json!({
            "type": "m.room.message",
            "event_id": event_id,
            "sender": sender,
            "origin_server_ts": chrono::Utc::now().timestamp_millis(),
            "content": {
                "msgtype": "m.text",
                "body": body,
            },
        });

        if let Some(mut timeline) = self.timelines.get_mut(room_id) {
            timeline.push(event);
        }
    }
}
