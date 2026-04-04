//! In-memory state for the mock Matrix homeserver.

use dashmap::DashMap;
use poly_test_common::{AuthState, EventBus};

/// Events broadcast to `/sync` long-poll waiters.
///
/// When a REST handler mutates state (e.g. sends a message), it publishes
/// a `MatrixEvent` to the bus. The `/sync` handler subscribes and wakes up,
/// returning the new events to the client.
#[derive(Clone, Debug)]
pub enum MatrixEvent {
    /// New timeline event in a room (message, state change, etc.)
    Timeline {
        room_id: String,
        event: serde_json::Value,
    },
    /// Typing indicator changed in a room.
    Typing {
        room_id: String,
        user_ids: Vec<String>,
    },
    /// User presence changed (online/offline/unavailable).
    Presence {
        user_id: String,
        presence: String,
    },
}

/// All mock Matrix state: users, rooms, events, tokens, broadcast bus.
#[derive(Clone)]
pub struct MatrixState {
    pub auth: AuthState,
    /// user_id → UserProfile
    pub users: DashMap<String, UserProfile>,
    /// room_id → Room
    pub rooms: DashMap<String, Room>,
    /// Event bus for real-time delivery to /sync long-poll clients.
    pub events: EventBus<MatrixEvent>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub displayname: String,
    pub avatar_url: Option<String>,
    pub password: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Room {
    pub room_id: String,
    pub name: String,
    pub topic: Option<String>,
    pub members: Vec<String>,
    pub is_space: bool,
    pub parent_space_id: Option<String>,
    pub events: Vec<serde_json::Value>,
}

impl MatrixState {
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            rooms: DashMap::new(),
            events: EventBus::new(),
        }
    }

    /// Seed demo data: Owl + Axolotl, 2 spaces, rooms, messages, DMs.
    /// Idempotent — skips if data already present.
    pub fn seed(&self) {
        // TODO(4.3.19): Populate demo data
        tracing::info!("seeding Matrix demo data");
    }

    /// Wipe all data to empty state.
    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.rooms.clear();
        tracing::info!("reset Matrix state to empty");
    }

    /// Wipe all data and re-seed. Most common operation between test runs.
    pub fn reseed(&self) {
        self.reset();
        self.seed();
    }
}
