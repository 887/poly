//! In-memory state for the mock Discord server.
//!
//! IDs use twilight-model's typed `Id<Marker>` newtypes (ISC-licensed). IDs are
//! nonzero u64 snowflakes, matching real Discord. Seed IDs are small round
//! numbers (1, 2, 100, 200…) for readability — not real snowflakes.

use dashmap::DashMap;
use poly_test_common::{AuthState, EventBus};
use twilight_model::channel::ChannelType;
use twilight_model::id::marker::{ChannelMarker, GuildMarker, MessageMarker, UserMarker};
use twilight_model::id::Id;

/// Events dispatched to Gateway WebSocket clients.
#[derive(Clone, Debug)]
pub enum DiscordEvent {
    MessageCreate {
        channel_id: Id<ChannelMarker>,
        message: serde_json::Value,
    },
    MessageUpdate {
        channel_id: Id<ChannelMarker>,
        message: serde_json::Value,
    },
    MessageDelete {
        channel_id: Id<ChannelMarker>,
        message_id: Id<MessageMarker>,
    },
    TypingStart {
        channel_id: Id<ChannelMarker>,
        user_id: Id<UserMarker>,
    },
    PresenceUpdate {
        user_id: Id<UserMarker>,
        status: String,
    },
    GuildCreate {
        guild: serde_json::Value,
    },
}

/// All mock Discord state.
#[derive(Clone)]
pub struct DiscordState {
    pub auth: AuthState,
    pub users: DashMap<Id<UserMarker>, User>,
    pub guilds: DashMap<Id<GuildMarker>, Guild>,
    pub channels: DashMap<Id<ChannelMarker>, Channel>,
    pub messages: DashMap<Id<ChannelMarker>, Vec<Message>>,
    pub events: EventBus<DiscordEvent>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: Id<UserMarker>,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
    pub password: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Guild {
    pub id: Id<GuildMarker>,
    pub name: String,
    pub owner_id: Id<UserMarker>,
    pub channels: Vec<Id<ChannelMarker>>,
    pub members: Vec<Id<UserMarker>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub id: Id<ChannelMarker>,
    pub name: String,
    pub guild_id: Option<Id<GuildMarker>>,
    pub channel_type: ChannelType,
    pub parent_id: Option<Id<ChannelMarker>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: Id<MessageMarker>,
    pub content: String,
    pub author_id: Id<UserMarker>,
    pub channel_id: Id<ChannelMarker>,
    pub timestamp: String,
}

impl Default for DiscordState {
    fn default() -> Self {
        Self::new()
    }
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

    /// Seed demo data: Koala + Kangaroo + Wallaby, 2 guilds, channels, messages.
    /// Idempotent — skips if data already present.
    pub fn seed(&self) {
        if !self.users.is_empty() {
            return;
        }
        tracing::info!("seeding Discord demo data");

        // Users — IDs 1, 2, 3
        self.users.insert(Id::new(1), User {
            id: Id::new(1),
            username: "koala".into(),
            discriminator: "0001".into(),
            avatar: None,
            password: "testpass123".into(),
        });
        self.users.insert(Id::new(2), User {
            id: Id::new(2),
            username: "kangaroo".into(),
            discriminator: "0002".into(),
            avatar: None,
            password: "testpass123".into(),
        });
        self.users.insert(Id::new(3), User {
            id: Id::new(3),
            username: "wallaby".into(),
            discriminator: "0003".into(),
            avatar: None,
            password: "testpass123".into(),
        });

        // Channels — 200..=202 (guild), 300 (DM)
        self.channels.insert(Id::new(200), Channel {
            id: Id::new(200),
            name: "general".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::GuildText,
            parent_id: None,
        });
        self.channels.insert(Id::new(201), Channel {
            id: Id::new(201),
            name: "random".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::GuildText,
            parent_id: None,
        });
        self.channels.insert(Id::new(202), Channel {
            id: Id::new(202),
            name: "announcements".into(),
            guild_id: Some(Id::new(101)),
            channel_type: ChannelType::GuildText,
            parent_id: None,
        });
        self.channels.insert(Id::new(300), Channel {
            id: Id::new(300),
            name: "".into(),
            guild_id: None,
            channel_type: ChannelType::Private,
            parent_id: None,
        });

        // Guilds — 100, 101
        self.guilds.insert(Id::new(100), Guild {
            id: Id::new(100),
            name: "Australiana".into(),
            owner_id: Id::new(1),
            channels: vec![Id::new(200), Id::new(201)],
            members: vec![Id::new(1), Id::new(2), Id::new(3)],
        });
        self.guilds.insert(Id::new(101), Guild {
            id: Id::new(101),
            name: "Wildlife Chat".into(),
            owner_id: Id::new(2),
            channels: vec![Id::new(202)],
            members: vec![Id::new(1), Id::new(2)],
        });

        // Messages in channel 200
        self.messages.insert(Id::new(200), vec![
            Message {
                id: Id::new(400),
                content: "G'day everyone!".into(),
                author_id: Id::new(1),
                channel_id: Id::new(200),
                timestamp: "2026-04-05T10:00:00.000Z".into(),
            },
            Message {
                id: Id::new(401),
                content: "Crikey, it's good to be here!".into(),
                author_id: Id::new(2),
                channel_id: Id::new(200),
                timestamp: "2026-04-05T10:01:00.000Z".into(),
            },
            Message {
                id: Id::new(402),
                content: "Has anyone seen my joey?".into(),
                author_id: Id::new(3),
                channel_id: Id::new(200),
                timestamp: "2026-04-05T10:02:00.000Z".into(),
            },
        ]);
        self.messages.insert(Id::new(202), vec![
            Message {
                id: Id::new(410),
                content: "Welcome to Wildlife Chat!".into(),
                author_id: Id::new(2),
                channel_id: Id::new(202),
                timestamp: "2026-04-05T09:00:00.000Z".into(),
            },
        ]);
    }

    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.guilds.clear();
        self.channels.clear();
        self.messages.clear();
        tracing::info!("reset Discord state to empty");
    }

    pub fn reseed(&self) {
        self.reset();
        self.seed();
    }
}
