//! In-memory state for the mock Teams/Graph API server.

use dashmap::DashMap;
use poly_test_common::AuthState;

/// All mock Teams state: users, teams, channels, chats, messages, tokens.
#[derive(Clone)]
pub struct TeamsState {
    pub auth: AuthState,
    pub users: DashMap<String, User>,
    pub teams: DashMap<String, Team>,
    pub channels: DashMap<String, Channel>,
    pub chats: DashMap<String, Chat>,
    pub messages: DashMap<String, Vec<Message>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: String,
    pub display_name: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub password: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Team {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub id: String,
    pub display_name: String,
    pub team_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Chat {
    pub id: String,
    pub chat_type: String,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: String,
    pub body_content: String,
    pub from_user_id: String,
    pub channel_or_chat_id: String,
    pub created_date_time: String,
}

impl TeamsState {
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            teams: DashMap::new(),
            channels: DashMap::new(),
            chats: DashMap::new(),
            messages: DashMap::new(),
        }
    }

    /// Seed demo data: Sheep + Walrus, 2 teams, channels, chats, messages.
    /// Idempotent — skips if data already present.
    pub fn seed(&self) {
        // TODO(4.6.13): Populate demo data
        tracing::info!("seeding Teams demo data");
    }

    /// Wipe all data to empty state.
    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.teams.clear();
        self.channels.clear();
        self.chats.clear();
        self.messages.clear();
        tracing::info!("reset Teams state to empty");
    }

    /// Wipe all data and re-seed. Most common operation between test runs.
    pub fn reseed(&self) {
        self.reset();
        self.seed();
    }
}
