//! In-memory state for the mock Stoat/Revolt server.

use dashmap::DashMap;
use poly_test_common::AuthState;

/// All mock Stoat state: users, servers, channels, messages, tokens.
#[derive(Clone)]
pub struct StoatState {
    pub auth: AuthState,
    pub users: DashMap<String, User>,
    pub servers: DashMap<String, Server>,
    pub channels: DashMap<String, Channel>,
    pub messages: DashMap<String, Vec<Message>>,
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
        }
    }

    /// Seed demo data: Stoat + Raccoon, 2 servers, channels, messages.
    pub fn seed(&self) {
        // TODO(4.4.12): Populate demo data
        tracing::info!("seeding Stoat demo data");
    }

    /// Clear all state and re-seed.
    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.servers.clear();
        self.channels.clear();
        self.messages.clear();
        self.seed();
    }
}
