//! In-memory state for the mock Stoat/Revolt server.

use dashmap::DashMap;
use poly_test_common::{AuthState, EventBus};

/// Events broadcast to Bonfire WebSocket clients.
///
/// When a REST handler mutates state (e.g. sends a message), it publishes
/// a `StoatEvent` to the bus. Connected WebSocket clients receive them in
/// real-time, matching the Revolt Bonfire protocol.
#[derive(Clone, Debug)]
pub enum StoatEvent {
    /// New message created in a channel.
    Message {
        channel_id: String,
        message: serde_json::Value,
    },
    /// Message updated (edited).
    MessageUpdate {
        channel_id: String,
        message_id: String,
        data: serde_json::Value,
    },
    /// Message deleted.
    MessageDelete {
        channel_id: String,
        message_id: String,
    },
    /// User started typing in a channel.
    ChannelStartTyping {
        channel_id: String,
        user_id: String,
    },
    /// User presence/status changed.
    UserUpdate {
        user_id: String,
        data: serde_json::Value,
    },
}

/// All mock Stoat state: users, servers, channels, messages, tokens, broadcast bus.
#[derive(Clone)]
pub struct StoatState {
    pub auth: AuthState,
    pub users: DashMap<String, User>,
    pub servers: DashMap<String, Server>,
    pub channels: DashMap<String, Channel>,
    pub messages: DashMap<String, Vec<Message>>,
    /// Event bus for real-time delivery to Bonfire WebSocket clients.
    pub events: EventBus<StoatEvent>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub password: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub channels: Vec<String>,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub server_id: Option<String>,
    pub channel_type: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub author: String,
    pub channel: String,
    pub timestamp: String,
}

impl StoatState {
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            servers: DashMap::new(),
            channels: DashMap::new(),
            messages: DashMap::new(),
            events: EventBus::new(),
        }
    }

    /// Seed demo data: Stoat + Raccoon, 2 servers, channels, messages.
    /// Idempotent — skips if data already present.
    pub fn seed(&self) {
        // TODO(4.4.12): Populate demo data
        tracing::info!("seeding Stoat demo data");
    }

    /// Wipe all data to empty state.
    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.servers.clear();
        self.channels.clear();
        self.messages.clear();
        tracing::info!("reset Stoat state to empty");
    }

    /// Wipe all data and re-seed. Most common operation between test runs.
    pub fn reseed(&self) {
        self.reset();
        self.seed();
    }
}
