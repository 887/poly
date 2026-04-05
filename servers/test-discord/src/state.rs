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
        if !self.users.is_empty() {
            return;
        }
        tracing::info!("seeding Discord demo data");

        // Users
        self.users.insert("U001".into(), User {
            id: "U001".into(),
            username: "koala".into(),
            discriminator: "0001".into(),
            avatar: None,
            password: "testpass123".into(),
        });
        self.users.insert("U002".into(), User {
            id: "U002".into(),
            username: "kangaroo".into(),
            discriminator: "0002".into(),
            avatar: None,
            password: "testpass123".into(),
        });
        self.users.insert("U003".into(), User {
            id: "U003".into(),
            username: "wallaby".into(),
            discriminator: "0003".into(),
            avatar: None,
            password: "testpass123".into(),
        });

        // Channels for guild G001
        self.channels.insert("CH001".into(), Channel {
            id: "CH001".into(),
            name: "general".into(),
            guild_id: Some("G001".into()),
            channel_type: 0,
            parent_id: None,
        });
        self.channels.insert("CH002".into(), Channel {
            id: "CH002".into(),
            name: "random".into(),
            guild_id: Some("G001".into()),
            channel_type: 0,
            parent_id: None,
        });
        // Channel for guild G002
        self.channels.insert("CH003".into(), Channel {
            id: "CH003".into(),
            name: "announcements".into(),
            guild_id: Some("G002".into()),
            channel_type: 0,
            parent_id: None,
        });
        // DM channel
        self.channels.insert("DM001".into(), Channel {
            id: "DM001".into(),
            name: "".into(),
            guild_id: None,
            channel_type: 1,
            parent_id: None,
        });

        // Guilds
        self.guilds.insert("G001".into(), Guild {
            id: "G001".into(),
            name: "Australiana".into(),
            owner_id: "U001".into(),
            channels: vec!["CH001".into(), "CH002".into()],
            members: vec!["U001".into(), "U002".into(), "U003".into()],
        });
        self.guilds.insert("G002".into(), Guild {
            id: "G002".into(),
            name: "Wildlife Chat".into(),
            owner_id: "U002".into(),
            channels: vec!["CH003".into()],
            members: vec!["U001".into(), "U002".into()],
        });

        // Messages for CH001
        self.messages.insert("CH001".into(), vec![
            Message {
                id: "M001".into(),
                content: "G'day everyone!".into(),
                author_id: "U001".into(),
                channel_id: "CH001".into(),
                timestamp: "2026-04-05T10:00:00.000Z".into(),
            },
            Message {
                id: "M002".into(),
                content: "Crikey, it's good to be here!".into(),
                author_id: "U002".into(),
                channel_id: "CH001".into(),
                timestamp: "2026-04-05T10:01:00.000Z".into(),
            },
            Message {
                id: "M003".into(),
                content: "Has anyone seen my joey?".into(),
                author_id: "U003".into(),
                channel_id: "CH001".into(),
                timestamp: "2026-04-05T10:02:00.000Z".into(),
            },
        ]);
        self.messages.insert("CH003".into(), vec![
            Message {
                id: "M010".into(),
                content: "Welcome to Wildlife Chat!".into(),
                author_id: "U002".into(),
                channel_id: "CH003".into(),
                timestamp: "2026-04-05T09:00:00.000Z".into(),
            },
        ]);
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
