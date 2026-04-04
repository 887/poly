//! In-memory state for the mock Discord server.

use dashmap::DashMap;
use poly_test_common::{AuthState, EventBus};

/// Events dispatched to Gateway WebSocket clients.
///
/// When a REST handler mutates state (e.g. sends a message), it publishes
/// a `DiscordEvent` to the bus. Connected Gateway clients receive them as
/// dispatch payloads matching Discord's event types.
#[derive(Clone, Debug)]
pub enum DiscordEvent {
    /// New message in a channel.
    MessageCreate {
        channel_id: String,
        message: serde_json::Value,
    },
    /// Message updated (edited).
    MessageUpdate {
        channel_id: String,
        message: serde_json::Value,
    },
    /// Message deleted.
    MessageDelete {
        channel_id: String,
        message_id: String,
    },
    /// User started typing.
    TypingStart {
        channel_id: String,
        user_id: String,
    },
    /// User presence changed.
    PresenceUpdate {
        user_id: String,
        status: String,
    },
    /// Guild (server) available — sent on READY.
    GuildCreate {
        guild: serde_json::Value,
    },
}

/// All mock Discord state: users, guilds, channels, messages, tokens, broadcast bus.
#[derive(Clone)]
pub struct DiscordState {
    pub auth: AuthState,
    pub users: DashMap<String, User>,
    pub guilds: DashMap<String, Guild>,
    pub channels: DashMap<String, Channel>,
    pub messages: DashMap<String, Vec<Message>>,
    /// Event bus for real-time delivery to Gateway WebSocket clients.
    pub events: EventBus<DiscordEvent>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
    pub password: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Guild {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub channels: Vec<String>,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub guild_id: Option<String>,
    pub channel_type: u8,
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub author_id: String,
    pub channel_id: String,
    pub timestamp: String,
}

impl DiscordState {
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            guilds: DashMap::new(),
            channels: DashMap::new(),
            messages: DashMap::new(),
            events: EventBus::new(),
        }
    }

    /// Seed demo data: Koala + Kangaroo, 2 guilds, channels, messages.
    /// Idempotent — skips if data already present.
    pub fn seed(&self) {
        // TODO(4.5.12): Populate demo data
        tracing::info!("seeding Discord demo data");
    }

    /// Wipe all data to empty state.
    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.guilds.clear();
        self.channels.clear();
        self.messages.clear();
        tracing::info!("reset Discord state to empty");
    }

    /// Wipe all data and re-seed. Most common operation between test runs.
    pub fn reseed(&self) {
        self.reset();
        self.seed();
    }
}
