//! In-memory state for the mock Stoat/Revolt server.

use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use poly_test_common::{AuthState, EventBus, HeaderInspectBuffer};
use std::sync::Arc as StdArc;

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

/// A server ban record.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BanRecord {
    pub server_id: String,
    pub user_id: String,
    pub reason: Option<String>,
}

/// Per-member moderation state (timeout).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct MemberModState {
    /// RFC3339 timeout expiry when the member is timed out.
    pub timeout: Option<String>,
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
    /// "server_id/user_id" → BanRecord
    pub bans: DashMap<String, BanRecord>,
    /// "server_id/user_id" → MemberModState
    pub member_mod: DashMap<String, MemberModState>,
    /// Ring buffer of recent inbound request headers (Phase E inspection endpoint).
    pub inspect: StdArc<HeaderInspectBuffer>,
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

impl poly_test_common::BackendHarness for StoatState {
    const BACKEND: &'static str = "stoat";

    fn new(auth: poly_test_common::AuthState) -> Self {
        let mut s = StoatState::new();
        s.auth = auth;
        s
    }

    fn seed(&self) { StoatState::seed(self); }
    fn reset(&self) { StoatState::reset(self); }
    // reseed() uses the default: reset() + seed()

    fn routes(state: std::sync::Arc<Self>) -> axum::Router<std::sync::Arc<Self>> {
        crate::routes_only(state)
    }

    fn inspect_buf(&self) -> std::sync::Arc<poly_test_common::HeaderInspectBuffer> {
        StdArc::clone(&self.inspect)
    }
}

impl Default for StoatState {
    fn default() -> Self {
        Self::new()
    }
}

impl StoatState {
    #[must_use]
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
            bans: DashMap::new(),
            member_mod: DashMap::new(),
            inspect: StdArc::new(HeaderInspectBuffer::new()),
        }
    }

    /// Composite key for ban/member-mod maps.
    #[must_use]
    pub fn member_key(server_id: &str, user_id: &str) -> String {
        format!("{server_id}/{user_id}")
    }

    /// Get next unique message ID (ULID-like format).
    #[must_use]
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
        let lemming_id = "LEMMING01".to_string();

        // Users
        self.users.insert(
            stoat_id.clone(),
            User {
                id: stoat_id.clone(),
                username: "stoat".into(),
                discriminator: "0001".into(),
                display_name: Some("Stoat".into()),
                avatar_url: Some("stoat".into()),
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
                avatar_url: Some("raccoon".into()),
                password: "testpass123".into(),
                status: Some(UserStatus {
                    text: Some("Rummaging through bins".into()),
                    presence: "Online".into(),
                }),
                online: true,
            },
        );
        self.users.insert(
            lemming_id.clone(),
            User {
                id: lemming_id.clone(),
                username: "lemming".into(),
                discriminator: "0003".into(),
                display_name: Some("Lemming".into()),
                avatar_url: Some("lemming".into()),
                password: "testpass123".into(),
                status: Some(UserStatus {
                    text: Some("Following the crowd off the cliff 🐾".into()),
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
                members: vec![stoat_id.clone(), raccoon_id.clone(), lemming_id.clone()],
            },
        );

        // Test Arena server — dedicated clean space for back-and-forth tests
        let arena_srv_id = "SRV_ARENA".to_string();
        let arena_ch_id = "CH_ARENA".to_string();

        self.create_channel(&arena_ch_id, "test-arena", Some("Dedicated back-and-forth test channel"), Some(&arena_srv_id), "TextChannel");

        self.servers.insert(
            arena_srv_id.clone(),
            Server {
                id: arena_srv_id.clone(),
                name: "Test Arena".into(),
                owner: stoat_id.clone(),
                icon_url: None,
                channels: vec![arena_ch_id.clone()],
                categories: vec![Category {
                    id: "CAT_ARENA".into(),
                    title: "Test Channels".into(),
                    channels: vec![arena_ch_id.clone()],
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
                members: vec![stoat_id.clone(), raccoon_id.clone(), lemming_id.clone()],
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

        // Seed messages — The Burrow #general
        self.add_message(&gen1_id, &stoat_id, "Welcome to The Burrow! Watch your head.");
        self.add_message(&gen1_id, &raccoon_id, "Nice place! Smells like earth and worms.");
        self.add_message(&gen1_id, &stoat_id, "That's the ambiance.");
        self.add_message(&gen1_id, &raccoon_id, "I brought snacks from the dumpster behind Whole Foods.");
        self.add_message(&gen1_id, &stoat_id, "Raccoon, those are just... regular groceries with dents.");
        self.add_message(&gen1_id, &raccoon_id, "Dented = discounted = free. Basic economics.");
        self.add_message(&gen1_id, &stoat_id, "That's not how economics works.");
        self.add_message(&gen1_id, &raccoon_id, "It is in the dumpster economy 📈");

        // The Burrow #random
        self.add_message(&random1_id, &stoat_id, "Fun fact: stoats can take down rabbits 10x their size.");
        self.add_message(&random1_id, &raccoon_id, "Fun fact: raccoons can open any trash can ever made.");
        self.add_message(&random1_id, &stoat_id, "That's not a fact, that's a lifestyle.");
        self.add_message(&random1_id, &raccoon_id, "Same thing 🦝");

        // The Burrow #memes
        self.add_message(&memes1_id, &raccoon_id, "just found out stoats do a war dance to confuse prey");
        self.add_message(&memes1_id, &stoat_id, "It's called the *weasel war dance* and it's a legitimate hunting strategy.");
        self.add_message(&memes1_id, &raccoon_id, "dude you literally backflip at rabbits until they forget how to run");
        self.add_message(&memes1_id, &stoat_id, "...it works though.");

        // Midnight Dumpster #general
        self.add_message(&gen2_id, &raccoon_id, "Found an entire pizza behind the mall!");
        self.add_message(&gen2_id, &stoat_id, "How entire are we talking?");
        self.add_message(&gen2_id, &raccoon_id, "Like 6 out of 8 slices. Premium find.");
        self.add_message(&gen2_id, &stoat_id, "What happened to the other 2?");
        self.add_message(&gen2_id, &raccoon_id, "Quality control. Had to taste test.");

        // Midnight Dumpster #food-finds
        self.add_message(&food_id, &raccoon_id, "🍕 Rating: the dumpster behind Italian Palace. 9/10. Fresh bread daily at 10pm.");
        self.add_message(&food_id, &stoat_id, "You have a rating system?");
        self.add_message(&food_id, &raccoon_id, "Obviously. I'm a professional. Here's the tier list:");
        self.add_message(&food_id, &raccoon_id, "S tier: Whole Foods, Trader Joe's\nA tier: Italian restaurants, bakeries\nB tier: Fast food chains\nC tier: Gas stations\nF tier: The place that puts locks on their bins 😤");
        self.add_message(&food_id, &stoat_id, "This is genuinely impressive and also deeply concerning.");

        // DMs
        self.add_message(&dm_id, &stoat_id, "Hey Raccoon, want to raid the compost bin tonight?");
        self.add_message(&dm_id, &raccoon_id, "Obviously. Meet at midnight?");
        self.add_message(&dm_id, &stoat_id, "Deal. Bring your tiny hands.");
        self.add_message(&dm_id, &raccoon_id, "My hands are DEXTEROUS not tiny 😤");
        self.add_message(&dm_id, &stoat_id, "Sure thing, little grabby paws.");
        self.add_message(&dm_id, &raccoon_id, "I will open every jar in your burrow while you sleep.");

        // Lemming joins the conversation in The Burrow #general
        self.add_message(&gen1_id, &lemming_id, "Hi everyone! New here. Just followed the others over.");
        self.add_message(&gen1_id, &stoat_id, "Followed who exactly?");
        self.add_message(&gen1_id, &lemming_id, "I... honestly don't remember. There were a lot of us.");
        self.add_message(&gen1_id, &raccoon_id, "Classic lemming energy 🐭");

        // Lemming in #memes
        self.add_message(&memes1_id, &lemming_id, "ok but has anyone else just walked off a cliff for no reason");
        self.add_message(&memes1_id, &raccoon_id, "...is that a joke or");
        self.add_message(&memes1_id, &lemming_id, "yes. mostly.");
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
        self.bans.clear();
        self.member_mod.clear();
        self.inspect.clear();
        tracing::info!("reset Stoat state to empty");
    }

    fn create_channel(&self, id: &str, name: &str, description: Option<&str>, server_id: Option<&str>, channel_type: &str) {
        self.channels.insert(
            id.to_string(),
            Channel {
                id: id.to_string(),
                name: name.to_string(),
                description: description.map(std::string::ToString::to_string),
                server_id: server_id.map(std::string::ToString::to_string),
                channel_type: channel_type.to_string(),
                recipients: vec![],
                last_message_id: None,
            },
        );
        self.messages.insert(id.to_string(), Vec::new());
    }

    /// Helper: add a message to a channel. Returns the new message ID,
    /// which seed code may discard (mutation is the primary effect).
    // lint-allow-unused: returned id is convenience for callers; seed callers intentionally drop it
    #[allow(clippy::must_use_candidate)]
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
