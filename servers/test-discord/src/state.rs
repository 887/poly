//! In-memory state for the mock Discord server.
//!
//! IDs use twilight-model's typed `Id<Marker>` newtypes (ISC-licensed). IDs are
//! nonzero u64 snowflakes, matching real Discord. Seed IDs are small round
//! numbers (1, 2, 100, 200…) for readability — not real snowflakes.

use dashmap::DashMap;
use poly_test_common::{AuthState, EventBus, HeaderInspectBuffer};
use std::sync::Arc;
use tokio::sync::RwLock;
use twilight_model::channel::ChannelType;
use twilight_model::id::marker::{ChannelMarker, GuildMarker, MessageMarker, RoleMarker, UserMarker};
use twilight_model::id::Id;
use std::sync::atomic::AtomicU16;

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
    /// Gateway thread lifecycle events (Phase 6.5).
    ThreadCreate { thread: serde_json::Value },
    ThreadUpdate { thread: serde_json::Value },
    ThreadDelete { thread_id: String, guild_id: String, parent_id: String },
    ThreadListSync { guild_id: String, threads: Vec<serde_json::Value> },
}

/// A guild role.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Role {
    pub id: Id<RoleMarker>,
    pub name: String,
    /// Permission bitfield (matches Discord wire format: string-encoded i64).
    pub permissions: String,
    pub position: u32,
    pub color: u32,
}

/// A ban record.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Ban {
    pub user_id: Id<UserMarker>,
    pub reason: Option<String>,
}

/// An audit log entry for moderation actions.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AuditLogEntry {
    /// Snowflake-style ID (we use incrementing u64 for test purposes).
    pub id: u64,
    /// Action type: 20=kick, 22=ban_add, 23=ban_remove, 12=channel_update, 72=msg_delete.
    pub action_type: u32,
    /// Moderator user ID.
    pub user_id: Option<Id<UserMarker>>,
    /// Target ID (user_id for kick/ban, channel_id for channel_update, message_id for msg_delete).
    pub target_id: Option<String>,
    pub reason: Option<String>,
}

/// A tag available in a forum channel.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ForumTag {
    pub id: u64,
    pub name: String,
    pub emoji_name: Option<String>,
    pub moderated: bool,
}

/// Metadata for a thread channel.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ThreadMetadata {
    pub archived: bool,
    pub locked: bool,
    pub auto_archive_duration: u32,
    pub archive_timestamp: Option<String>,
    pub create_timestamp: Option<String>,
}

/// A file attachment on a message.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Attachment {
    pub id: u64,
    pub filename: String,
    pub content_type: Option<String>,
    pub size: u64,
    pub url: String,
    pub proxy_url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Map from `(guild_id, user_id)` to the role IDs assigned to that member.
pub type MemberRolesMap = DashMap<(Id<GuildMarker>, Id<UserMarker>), Vec<Id<RoleMarker>>>;

/// All mock Discord state.
#[derive(Clone)]
pub struct DiscordState {
    pub auth: AuthState,
    pub users: DashMap<Id<UserMarker>, User>,
    pub guilds: DashMap<Id<GuildMarker>, Guild>,
    pub channels: DashMap<Id<ChannelMarker>, Channel>,
    pub messages: DashMap<Id<ChannelMarker>, Vec<Message>>,
    pub events: EventBus<DiscordEvent>,
    /// Gateway WebSocket URL returned by `GET /api/v10/gateway`.
    /// Set after the server binds so tests can use the actual port.
    pub gateway_url: Arc<RwLock<String>>,
    /// Roles per guild (guild_id → Vec<Role>).
    pub guild_roles: DashMap<Id<GuildMarker>, Vec<Role>>,
    /// Member roles per (guild, user) — role IDs assigned to that member.
    pub member_roles: MemberRolesMap,
    /// Bans per guild (guild_id → Vec<Ban>).
    pub bans: DashMap<Id<GuildMarker>, Vec<Ban>>,
    /// Audit log per guild (guild_id → Vec<AuditLogEntry>), newest first.
    pub audit_log: DashMap<Id<GuildMarker>, Vec<AuditLogEntry>>,
    /// Next audit log entry ID (incrementing counter).
    pub next_audit_id: Arc<std::sync::atomic::AtomicU64>,
    /// Ring buffer of recent inbound request headers (Phase E inspection endpoint).
    pub inspect: Arc<HeaderInspectBuffer>,
    /// UDP echo socket port bound at startup (Phase A.3 — voice mock).
    /// Zero means not yet bound.
    pub voice_udp_port: Arc<AtomicU16>,
    /// HTTP server bind address (set in post_bind). Used by op-4 handler to
    /// construct the voice WS endpoint returned in VOICE_SERVER_UPDATE.
    pub server_addr: Arc<RwLock<String>>,
    /// Last gateway-identified user_id extracted from op 2 IDENTIFY.
    /// Stored as a string for flexibility; defaults to "mock-user-1".
    pub gateway_user_id: Arc<RwLock<String>>,
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
    /// Banner URL / hash. Stored as-is (URL for test convenience, hash for
    /// real Discord CDN URLs).
    pub banner: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub id: Id<ChannelMarker>,
    pub name: String,
    pub guild_id: Option<Id<GuildMarker>>,
    pub channel_type: ChannelType,
    pub parent_id: Option<Id<ChannelMarker>>,
    /// For forum channels: available tags.
    pub available_tags: Vec<ForumTag>,
    /// For forum channels: default layout (0=not set, 1=list, 2=gallery).
    pub default_forum_layout: Option<u8>,
    /// For threads: applied tag IDs.
    pub applied_tags: Vec<u64>,
    /// For threads: thread metadata.
    pub thread_metadata: Option<ThreadMetadata>,
    /// For threads: the user ID of the thread owner.
    pub owner_id: Option<Id<UserMarker>>,
    /// For threads: message count.
    pub message_count: Option<u32>,
    /// For threads: member count.
    pub member_count: Option<u32>,
    /// For forum channels: message ID that spawned a thread (for inline thread ref).
    pub thread_message_id: Option<Id<MessageMarker>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: Id<MessageMarker>,
    pub content: String,
    pub author_id: Id<UserMarker>,
    pub channel_id: Id<ChannelMarker>,
    pub timestamp: String,
    /// Attachments on this message.
    pub attachments: Vec<Attachment>,
    /// If this message spawned a thread, the thread channel ID.
    pub thread_id: Option<Id<ChannelMarker>>,
}

impl poly_test_common::BackendHarness for DiscordState {
    const BACKEND: &'static str = "discord";

    fn new(auth: poly_test_common::AuthState) -> Self {
        let mut s = DiscordState::new();
        s.auth = auth;
        s
    }

    fn seed(&self) { DiscordState::seed(self); DiscordState::seed_moderation(self); }
    fn reset(&self) { DiscordState::reset(self); }
    fn reseed(&self) { DiscordState::reseed(self); } // includes seed_moderation()

    fn routes(state: std::sync::Arc<Self>) -> axum::Router<std::sync::Arc<Self>> {
        crate::routes_only(state)
    }

    fn inspect_buf(&self) -> std::sync::Arc<poly_test_common::HeaderInspectBuffer> {
        Arc::clone(&self.inspect)
    }

    fn post_bind(
        state: &std::sync::Arc<Self>,
        addr: std::net::SocketAddr,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let state = Arc::clone(state);
        Box::pin(async move {
            *state.gateway_url.write().await =
                format!("ws://{}/gateway/ws", addr);
            // Stash the HTTP server address for voice endpoint construction.
            *state.server_addr.write().await = addr.to_string();
            // Bind the UDP echo socket and spawn the echo loop.
            crate::routes::bind_and_spawn_udp_echo(&state).await;
        })
    }
}

impl Default for DiscordState {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscordState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            guilds: DashMap::new(),
            channels: DashMap::new(),
            messages: DashMap::new(),
            events: EventBus::new(),
            gateway_url: Arc::new(RwLock::new("ws://localhost:9102".to_string())),
            guild_roles: DashMap::new(),
            member_roles: DashMap::new(),
            bans: DashMap::new(),
            audit_log: DashMap::new(),
            next_audit_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            inspect: Arc::new(HeaderInspectBuffer::new()),
            voice_udp_port: Arc::new(AtomicU16::new(0)),
            server_addr: Arc::new(RwLock::new("127.0.0.1:9102".to_string())),
            gateway_user_id: Arc::new(RwLock::new("mock-user-1".to_string())),
        }
    }

    /// Seed demo data: Koala + Kangaroo + Wallaby, 2 guilds, channels, messages.
    /// Idempotent — skips if data already present.
    pub fn seed(&self) {
        if !self.users.is_empty() {
            return;
        }
        tracing::info!("seeding Discord demo data");

        // Users — IDs 1, 2, 3.
        // Avatar hashes map to bundled bytes served from `/avatars/{id}/{hash}.png`
        // (see `routes::serve_avatar`). "platypus" stands in for wallaby — no
        // wallaby asset ships in `clients/demo/assets/`.
        self.users.insert(Id::new(1), User {
            id: Id::new(1),
            username: "koala".into(),
            discriminator: "0001".into(),
            avatar: Some("koala".into()),
            password: "testpass123".into(),
        });
        self.users.insert(Id::new(2), User {
            id: Id::new(2),
            username: "kangaroo".into(),
            discriminator: "0002".into(),
            avatar: Some("kangaroo".into()),
            password: "testpass123".into(),
        });
        self.users.insert(Id::new(3), User {
            id: Id::new(3),
            username: "wallaby".into(),
            discriminator: "0003".into(),
            avatar: Some("platypus".into()),
            password: "testpass123".into(),
        });

        // Channels — 200..=202 (guild), 300 (DM)
        self.channels.insert(Id::new(200), Channel {
            id: Id::new(200),
            name: "general".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::GuildText,
            parent_id: None,
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![],
            thread_metadata: None,
            owner_id: None,
            message_count: None,
            member_count: None,
            thread_message_id: None,
        });
        self.channels.insert(Id::new(201), Channel {
            id: Id::new(201),
            name: "random".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::GuildText,
            parent_id: None,
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![],
            thread_metadata: None,
            owner_id: None,
            message_count: None,
            member_count: None,
            thread_message_id: None,
        });
        self.channels.insert(Id::new(202), Channel {
            id: Id::new(202),
            name: "announcements".into(),
            guild_id: Some(Id::new(101)),
            channel_type: ChannelType::GuildText,
            parent_id: None,
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![],
            thread_metadata: None,
            owner_id: None,
            message_count: None,
            member_count: None,
            thread_message_id: None,
        });
        self.channels.insert(Id::new(300), Channel {
            id: Id::new(300),
            name: "".into(),
            guild_id: None,
            channel_type: ChannelType::Private,
            parent_id: None,
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![],
            thread_metadata: None,
            owner_id: None,
            message_count: None,
            member_count: None,
            thread_message_id: None,
        });

        // -----------------------------------------------------------------------
        // Phase 6 — Forum + Thread seed channels
        // -----------------------------------------------------------------------

        // 500 — GUILD_FORUM #general-discussion (guild 100)
        //   Tags: 1=question, 2=show-and-tell, 3=announcement
        self.channels.insert(Id::new(500), Channel {
            id: Id::new(500),
            name: "general-discussion".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::GuildForum,
            parent_id: None,
            available_tags: vec![
                ForumTag { id: 1, name: "question".into(), emoji_name: Some("❓".into()), moderated: false },
                ForumTag { id: 2, name: "show-and-tell".into(), emoji_name: Some("🎨".into()), moderated: false },
                ForumTag { id: 3, name: "announcement".into(), emoji_name: Some("📢".into()), moderated: true },
            ],
            default_forum_layout: Some(1),
            applied_tags: vec![],
            thread_metadata: None,
            owner_id: None,
            message_count: None,
            member_count: None,
            thread_message_id: None,
        });

        // 501 — PUBLIC_THREAD (forum post, parent=500, tag=question, active)
        self.channels.insert(Id::new(501), Channel {
            id: Id::new(501),
            name: "How do I get started?".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::PublicThread,
            parent_id: Some(Id::new(500)),
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![1],
            thread_metadata: Some(ThreadMetadata {
                archived: false,
                locked: false,
                auto_archive_duration: 1440,
                archive_timestamp: None,
                create_timestamp: Some("2026-04-10T08:00:00.000Z".into()),
            }),
            owner_id: Some(Id::new(2)),
            message_count: Some(3),
            member_count: Some(2),
            thread_message_id: None,
        });

        // 502 — PUBLIC_THREAD (forum post, parent=500, tag=show-and-tell, active)
        self.channels.insert(Id::new(502), Channel {
            id: Id::new(502),
            name: "My wombat diorama".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::PublicThread,
            parent_id: Some(Id::new(500)),
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![2],
            thread_metadata: Some(ThreadMetadata {
                archived: false,
                locked: false,
                auto_archive_duration: 1440,
                archive_timestamp: None,
                create_timestamp: Some("2026-04-11T10:00:00.000Z".into()),
            }),
            owner_id: Some(Id::new(1)),
            message_count: Some(2),
            member_count: Some(3),
            thread_message_id: None,
        });

        // 503 — PUBLIC_THREAD (archived + locked, parent=500, tag=announcement)
        self.channels.insert(Id::new(503), Channel {
            id: Id::new(503),
            name: "Server rules update".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::PublicThread,
            parent_id: Some(Id::new(500)),
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![3],
            thread_metadata: Some(ThreadMetadata {
                archived: true,
                locked: true,
                auto_archive_duration: 10080,
                archive_timestamp: Some("2026-04-01T00:00:00.000Z".into()),
                create_timestamp: Some("2026-03-25T12:00:00.000Z".into()),
            }),
            owner_id: Some(Id::new(1)),
            message_count: Some(1),
            member_count: Some(1),
            thread_message_id: None,
        });

        // 510 — GUILD_TEXT #wildlife-news (guild 100); message 520 has inline thread ref
        self.channels.insert(Id::new(510), Channel {
            id: Id::new(510),
            name: "wildlife-news".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::GuildText,
            parent_id: None,
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![],
            thread_metadata: None,
            owner_id: None,
            message_count: None,
            member_count: None,
            thread_message_id: None,
        });

        // 511 — PUBLIC_THREAD (inline, parent=510, spawned from msg 520)
        self.channels.insert(Id::new(511), Channel {
            id: Id::new(511),
            name: "Koala sighting discussion".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::PublicThread,
            parent_id: Some(Id::new(510)),
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![],
            thread_metadata: Some(ThreadMetadata {
                archived: false,
                locked: false,
                auto_archive_duration: 1440,
                archive_timestamp: None,
                create_timestamp: Some("2026-04-12T09:30:00.000Z".into()),
            }),
            owner_id: Some(Id::new(3)),
            message_count: Some(2),
            member_count: Some(2),
            thread_message_id: Some(Id::new(520)),
        });

        // 600 — GUILD_MEDIA #media-gallery (guild 101), default_forum_layout=2 (Gallery)
        self.channels.insert(Id::new(600), Channel {
            id: Id::new(600),
            name: "media-gallery".into(),
            guild_id: Some(Id::new(101)),
            channel_type: ChannelType::GuildMedia,
            parent_id: None,
            available_tags: vec![
                ForumTag { id: 10, name: "photos".into(), emoji_name: Some("📷".into()), moderated: false },
                ForumTag { id: 11, name: "videos".into(), emoji_name: Some("🎬".into()), moderated: false },
            ],
            default_forum_layout: Some(2),
            applied_tags: vec![],
            thread_metadata: None,
            owner_id: None,
            message_count: None,
            member_count: None,
            thread_message_id: None,
        });

        // 601 — PUBLIC_THREAD (media post, parent=600, tag=photos)
        self.channels.insert(Id::new(601), Channel {
            id: Id::new(601),
            name: "Sunset at the billabong".into(),
            guild_id: Some(Id::new(101)),
            channel_type: ChannelType::PublicThread,
            parent_id: Some(Id::new(600)),
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![10],
            thread_metadata: Some(ThreadMetadata {
                archived: false,
                locked: false,
                auto_archive_duration: 4320,
                archive_timestamp: None,
                create_timestamp: Some("2026-04-13T17:00:00.000Z".into()),
            }),
            owner_id: Some(Id::new(2)),
            message_count: Some(1),
            member_count: Some(2),
            thread_message_id: None,
        });

        // Test Arena channel (id=250) in guild 100 — clean state for back-and-forth tests
        self.channels.insert(Id::new(250), Channel {
            id: Id::new(250),
            name: "test-arena".into(),
            guild_id: Some(Id::new(100)),
            channel_type: ChannelType::GuildText,
            parent_id: None,
            available_tags: vec![],
            default_forum_layout: None,
            applied_tags: vec![],
            thread_metadata: None,
            owner_id: None,
            message_count: None,
            member_count: None,
            thread_message_id: None,
        });

        // Guilds — 100, 101 (updated with new channel IDs)
        self.guilds.insert(Id::new(100), Guild {
            id: Id::new(100),
            name: "Australiana".into(),
            owner_id: Id::new(1),
            channels: vec![
                Id::new(200), Id::new(201), Id::new(250),
                Id::new(500), Id::new(501), Id::new(502), Id::new(503),
                Id::new(510), Id::new(511),
            ],
            members: vec![Id::new(1), Id::new(2), Id::new(3)],
            banner: None,
        });
        self.guilds.insert(Id::new(101), Guild {
            id: Id::new(101),
            name: "Wildlife Chat".into(),
            owner_id: Id::new(2),
            channels: vec![Id::new(202), Id::new(600), Id::new(601)],
            members: vec![Id::new(1), Id::new(2)],
            banner: None,
        });

        // Messages in channel 200
        self.messages.insert(Id::new(200), vec![
            Message {
                id: Id::new(400),
                content: "G'day everyone!".into(),
                author_id: Id::new(1),
                channel_id: Id::new(200),
                timestamp: "2026-04-05T10:00:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
            Message {
                id: Id::new(401),
                content: "Crikey, it's good to be here!".into(),
                author_id: Id::new(2),
                channel_id: Id::new(200),
                timestamp: "2026-04-05T10:01:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
            Message {
                id: Id::new(402),
                content: "Has anyone seen my joey?".into(),
                author_id: Id::new(3),
                channel_id: Id::new(200),
                timestamp: "2026-04-05T10:02:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
        ]);
        self.messages.insert(Id::new(202), vec![
            Message {
                id: Id::new(410),
                content: "Welcome to Wildlife Chat!".into(),
                author_id: Id::new(2),
                channel_id: Id::new(202),
                timestamp: "2026-04-05T09:00:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
        ]);

        // Messages in forum threads (501, 502, 503)
        self.messages.insert(Id::new(501), vec![
            Message {
                id: Id::new(5010),
                content: "Hey everyone, just joined! Where do I start?".into(),
                author_id: Id::new(2),
                channel_id: Id::new(501),
                timestamp: "2026-04-10T08:00:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
            Message {
                id: Id::new(5011),
                content: "Check out the wiki first, it has all the basics.".into(),
                author_id: Id::new(1),
                channel_id: Id::new(501),
                timestamp: "2026-04-10T08:05:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
            Message {
                id: Id::new(5012),
                content: "Thanks, the wiki was super helpful!".into(),
                author_id: Id::new(2),
                channel_id: Id::new(501),
                timestamp: "2026-04-10T09:00:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
        ]);
        self.messages.insert(Id::new(502), vec![
            Message {
                id: Id::new(5020),
                content: "I made a wombat diorama out of recycled materials!".into(),
                author_id: Id::new(1),
                channel_id: Id::new(502),
                timestamp: "2026-04-11T10:00:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
            Message {
                id: Id::new(5021),
                content: "That looks amazing, well done!".into(),
                author_id: Id::new(3),
                channel_id: Id::new(502),
                timestamp: "2026-04-11T10:30:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
        ]);
        self.messages.insert(Id::new(503), vec![
            Message {
                id: Id::new(5030),
                content: "Server rules have been updated. Please read before posting.".into(),
                author_id: Id::new(1),
                channel_id: Id::new(503),
                timestamp: "2026-03-25T12:00:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
        ]);

        // Messages in #wildlife-news (510) — msg 520 has inline thread ref
        self.messages.insert(Id::new(510), vec![
            Message {
                id: Id::new(520),
                content: "Spotted a koala near the river this morning!".into(),
                author_id: Id::new(3),
                channel_id: Id::new(510),
                timestamp: "2026-04-12T09:00:00.000Z".into(),
                attachments: vec![],
                thread_id: Some(Id::new(511)),
            },
        ]);

        // Messages in inline thread (511)
        self.messages.insert(Id::new(511), vec![
            Message {
                id: Id::new(5110),
                content: "Which part of the river? I want to go see!".into(),
                author_id: Id::new(2),
                channel_id: Id::new(511),
                timestamp: "2026-04-12T09:30:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
            Message {
                id: Id::new(5111),
                content: "Just past the old timber bridge, near the gum trees.".into(),
                author_id: Id::new(3),
                channel_id: Id::new(511),
                timestamp: "2026-04-12T09:45:00.000Z".into(),
                attachments: vec![],
                thread_id: None,
            },
        ]);

        // Messages in media thread (601) — OP has an attachment
        self.messages.insert(Id::new(601), vec![
            Message {
                id: Id::new(6010),
                content: "Golden hour at the billabong last night 📸".into(),
                author_id: Id::new(2),
                channel_id: Id::new(601),
                timestamp: "2026-04-13T17:00:00.000Z".into(),
                attachments: vec![
                    Attachment {
                        id: 90001,
                        filename: "billabong_sunset.jpg".into(),
                        content_type: Some("image/jpeg".into()),
                        size: 204800,
                        url: "https://cdn.discordapp.com/attachments/601/90001/billabong_sunset.jpg".into(),
                        proxy_url: "https://media.discordapp.net/attachments/601/90001/billabong_sunset.jpg".into(),
                        width: Some(1920),
                        height: Some(1080),
                    },
                ],
                thread_id: None,
            },
        ]);
    }

    /// Seed roles and member-role assignments.
    ///
    /// Guild 100 (Australiana): koala (user 1) is owner + has "Admin" role (id 10).
    /// Guild 101 (Wildlife Chat): kangaroo (user 2) is owner.
    pub fn seed_moderation(&self) {
        if !self.guild_roles.is_empty() {
            return;
        }

        // Roles for guild 100.
        // Permission bits: ADMINISTRATOR (1<<3=8) covers all.
        self.guild_roles.insert(Id::new(100), vec![
            Role {
                id: Id::new(100), // @everyone role shares guild ID
                name: "@everyone".into(),
                permissions: "0".into(),
                position: 0,
                color: 0,
            },
            Role {
                id: Id::new(10),
                name: "Admin".into(),
                // ADMINISTRATOR = 1<<3 = 8; represented as string per Discord wire format.
                permissions: "8".into(),
                position: 1,
                color: 0xFF5765,
            },
        ]);

        // Roles for guild 101.
        self.guild_roles.insert(Id::new(101), vec![
            Role {
                id: Id::new(101), // @everyone
                name: "@everyone".into(),
                permissions: "0".into(),
                position: 0,
                color: 0,
            },
        ]);

        // koala (user 1) has the Admin role in guild 100.
        self.member_roles
            .insert((Id::new(100), Id::new(1)), vec![Id::new(10)]);

        // kangaroo (user 2) has no extra roles in guild 100 (non-owner member).
        self.member_roles
            .insert((Id::new(100), Id::new(2)), vec![]);

        // Seed audit log with some test entries for guild 100.
        let next_id = self.next_audit_id.fetch_add(3, std::sync::atomic::Ordering::Relaxed);
        self.audit_log.insert(Id::new(100), vec![
            AuditLogEntry {
                id: next_id.saturating_add(2),
                action_type: 22, // ban_add
                user_id: Some(Id::new(1)),
                target_id: Some("3".to_string()),
                reason: Some("spamming".into()),
            },
            AuditLogEntry {
                id: next_id.saturating_add(1),
                action_type: 20, // kick
                user_id: Some(Id::new(1)),
                target_id: Some("2".to_string()),
                reason: None,
            },
            AuditLogEntry {
                id: next_id,
                action_type: 72, // msg_delete
                user_id: Some(Id::new(1)),
                target_id: Some("400".to_string()),
                reason: None,
            },
        ]);
    }

    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.guilds.clear();
        self.channels.clear();
        self.messages.clear();
        self.guild_roles.clear();
        self.member_roles.clear();
        self.bans.clear();
        self.audit_log.clear();
        self.next_audit_id.store(1, std::sync::atomic::Ordering::Relaxed);
        self.inspect.clear();
        // gateway_user_id resets lazily — each IDENTIFY overwrites it.
        // voice_udp_port is bound once at startup and not reset.
        tracing::info!("reset Discord state to empty");
    }

    pub fn reseed(&self) {
        self.reset();
        self.seed();
        self.seed_moderation();
    }
}
