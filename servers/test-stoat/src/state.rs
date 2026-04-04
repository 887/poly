//! In-memory state for the mock Stoat/Revolt server.

use std::sync::atomic::{AtomicU64, Ordering};

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
    /// channel_id → Vec<Message> (append-only)
    pub messages: DashMap<String, Vec<Message>>,
    /// user_id → Vec<DM/Group channel IDs>
    pub dm_channels: DashMap<String, Vec<String>>,
    /// channel_id → unread state
    pub unreads: DashMap<String, UnreadState>,
    /// Event bus for real-time delivery to Bonfire WebSocket clients.
    pub events: EventBus<StoatEvent>,
    /// Global counter for message IDs.
    msg_counter: std::sync::Arc<AtomicU64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub password: String,
    pub status: Option<UserStatus>,
    pub online: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserStatus {
    pub text: Option<String>,
    pub presence: String, // "Online", "Idle", "Busy", "Invisible"
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub icon_url: Option<String>,
    pub channels: Vec<String>,
    pub categories: Vec<Category>,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Category {
    pub id: String,
    pub title: String,
    pub channels: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub server_id: Option<String>,
    /// "TextChannel", "DirectMessage", "Group", "SavedMessages"
    pub channel_type: String,
    /// For DMs/Groups: list of participant user IDs
    pub recipients: Vec<String>,
    pub last_message_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    #[serde(rename = "_id")]
    pub id: String,
    pub content: String,
    pub author: String,
    pub channel: String,
    pub nonce: Option<String>,
    pub replies: Option<Vec<String>>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UnreadState {
    pub channel_id: String,
    pub last_id: Option<String>,
    pub mentions: Vec<String>,
}

impl StoatState {
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            servers: DashMap::new(),
            channels: DashMap::new(),
            messages: DashMap::new(),
            dm_channels: DashMap::new(),
            unreads: DashMap::new(),
            events: EventBus::new(),
            msg_counter: std::sync::Arc::new(AtomicU64::new(1)),
        }
    }

    /// Get next unique message ID (ULID-like format).
    pub fn next_message_id(&self) -> String {
        let n = self.msg_counter.fetch_add(1, Ordering::Relaxed);
        format!("01HMSG{n:010}")
    }

    /// Seed demo data: Stoat + Raccoon, 2 servers, channels, messages, DM.
    /// Idempotent — skips if data already present.
    pub fn seed(&self) {
        if !self.users.is_empty() {
            tracing::info!("Stoat demo data already seeded, skipping");
            return;
        }
        tracing::info!("seeding Stoat demo data");

        let stoat_id = "STOAT01".to_string();
        let raccoon_id = "RACCOON01".to_string();

        // Users
        self.users.insert(
            stoat_id.clone(),
            User {
                id: stoat_id.clone(),
                username: "stoat".into(),
                discriminator: "0001".into(),
                display_name: Some("Stoat".into()),
                avatar_url: Some("/avatars/stoat.png".into()),
                password: "testpass123".into(),
                status: Some(UserStatus {
                    text: Some("Hunting voles".into()),
                    presence: "Online".into(),
                }),
                online: true,
            },
        );
        self.users.insert(
            raccoon_id.clone(),
            User {
                id: raccoon_id.clone(),
                username: "raccoon".into(),
                discriminator: "0002".into(),
                display_name: Some("Raccoon".into()),
                avatar_url: Some("/avatars/raccoon.png".into()),
                password: "testpass123".into(),
                status: Some(UserStatus {
                    text: Some("Rummaging through bins".into()),
                    presence: "Online".into(),
                }),
                online: true,
            },
        );

        // Server 1: The Burrow
        let srv1_id = "SRV001".to_string();
        let gen1_id = "CH001".to_string();
        let random1_id = "CH002".to_string();
        let memes1_id = "CH003".to_string();

        self.create_channel(&gen1_id, "general", Some("General discussion"), Some(&srv1_id), "TextChannel");
        self.create_channel(&random1_id, "random", Some("Off-topic"), Some(&srv1_id), "TextChannel");
        self.create_channel(&memes1_id, "memes", Some("Funny stuff"), Some(&srv1_id), "TextChannel");

        self.servers.insert(
            srv1_id.clone(),
            Server {
                id: srv1_id.clone(),
                name: "The Burrow".into(),
                owner: stoat_id.clone(),
                icon_url: None,
                channels: vec![gen1_id.clone(), random1_id.clone(), memes1_id.clone()],
                categories: vec![Category {
                    id: "CAT001".into(),
                    title: "Text Channels".into(),
                    channels: vec![gen1_id.clone(), random1_id.clone(), memes1_id.clone()],
                }],
                members: vec![stoat_id.clone(), raccoon_id.clone()],
            },
        );

        // Server 2: Midnight Dumpster
        let srv2_id = "SRV002".to_string();
        let gen2_id = "CH004".to_string();
        let food_id = "CH005".to_string();

        self.create_channel(&gen2_id, "general", Some("Main chat"), Some(&srv2_id), "TextChannel");
        self.create_channel(&food_id, "food-finds", Some("Best dumpster diving spots"), Some(&srv2_id), "TextChannel");

        self.servers.insert(
            srv2_id.clone(),
            Server {
                id: srv2_id.clone(),
                name: "Midnight Dumpster".into(),
                owner: raccoon_id.clone(),
                icon_url: None,
                channels: vec![gen2_id.clone(), food_id.clone()],
                categories: vec![Category {
                    id: "CAT002".into(),
                    title: "Text Channels".into(),
                    channels: vec![gen2_id.clone(), food_id.clone()],
                }],
                members: vec![stoat_id.clone(), raccoon_id.clone()],
            },
        );

        // DM between Stoat and Raccoon
        let dm_id = "CHDM001".to_string();
        self.channels.insert(
            dm_id.clone(),
            Channel {
                id: dm_id.clone(),
                name: String::new(),
                description: None,
                server_id: None,
                channel_type: "DirectMessage".into(),
                recipients: vec![stoat_id.clone(), raccoon_id.clone()],
                last_message_id: None,
            },
        );
        self.messages.insert(dm_id.clone(), Vec::new());
        self.dm_channels
            .entry(stoat_id.clone())
            .or_default()
            .push(dm_id.clone());
        self.dm_channels
            .entry(raccoon_id.clone())
            .or_default()
            .push(dm_id.clone());

        // Seed messages
        self.add_message(&gen1_id, &stoat_id, "Welcome to The Burrow! Watch your head.");
        self.add_message(&gen1_id, &raccoon_id, "Nice place! Smells like earth and worms.");
        self.add_message(&gen1_id, &stoat_id, "That's the ambiance.");
        self.add_message(&gen1_id, &raccoon_id, "I brought snacks from the dumpster behind Whole Foods.");
        self.add_message(&gen1_id, &stoat_id, "Raccoon, those are just... regular groceries with dents.");

        self.add_message(&gen2_id, &raccoon_id, "Found an entire pizza behind the mall!");
        self.add_message(&gen2_id, &stoat_id, "How entire are we talking?");
        self.add_message(&gen2_id, &raccoon_id, "Like 6 out of 8 slices. Premium find.");

        self.add_message(&dm_id, &stoat_id, "Hey Raccoon, want to raid the compost bin tonight?");
        self.add_message(&dm_id, &raccoon_id, "Obviously. Meet at midnight?");
        self.add_message(&dm_id, &stoat_id, "Deal. Bring your tiny hands.");
    }

    /// Wipe all data to empty state.
    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.servers.clear();
        self.channels.clear();
        self.messages.clear();
        self.dm_channels.clear();
        self.unreads.clear();
        tracing::info!("reset Stoat state to empty");
    }

    /// Wipe all data and re-seed. Most common operation between test runs.
    pub fn reseed(&self) {
        self.reset();
        self.seed();
    }

    fn create_channel(&self, id: &str, name: &str, description: Option<&str>, server_id: Option<&str>, channel_type: &str) {
        self.channels.insert(
            id.to_string(),
            Channel {
                id: id.to_string(),
                name: name.to_string(),
                description: description.map(|s| s.to_string()),
                server_id: server_id.map(|s| s.to_string()),
                channel_type: channel_type.to_string(),
                recipients: vec![],
                last_message_id: None,
            },
        );
        self.messages.insert(id.to_string(), Vec::new());
    }

    /// Helper: add a message to a channel.
    pub fn add_message(&self, channel_id: &str, author: &str, content: &str) -> String {
        let msg_id = self.next_message_id();
        let msg = Message {
            id: msg_id.clone(),
            content: content.to_string(),
            author: author.to_string(),
            channel: channel_id.to_string(),
            nonce: None,
            replies: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        if let Some(mut timeline) = self.messages.get_mut(channel_id) {
            timeline.push(msg);
        }

        // Update last_message_id
        if let Some(mut ch) = self.channels.get_mut(channel_id) {
            ch.last_message_id = Some(msg_id.clone());
        }

        msg_id
    }
}
