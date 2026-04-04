//! In-memory state for the mock Discord server.

use dashmap::DashMap;
use poly_test_common::AuthState;

/// All mock Discord state: users, guilds, channels, messages, tokens.
#[derive(Clone)]
pub struct DiscordState {
    pub auth: AuthState,
    pub users: DashMap<String, User>,
    pub guilds: DashMap<String, Guild>,
    pub channels: DashMap<String, Channel>,
    pub messages: DashMap<String, Vec<Message>>,
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
