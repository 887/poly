//! In-memory state for the mock Discord server.
//!
//! IDs use twilight-model's typed `Id<Marker>` newtypes (ISC-licensed). IDs are
//! nonzero u64 snowflakes, matching real Discord. Seed IDs are small round
//! numbers (1, 2, 100, 200…) for readability — not real snowflakes.

use dashmap::DashMap;
use poly_test_common::{AuthState, EventBus};
use std::sync::Arc;
use tokio::sync::RwLock;
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
    /// Gateway thread lifecycle events (Phase 6.5).
    ThreadCreate { thread: serde_json::Value },
    ThreadUpdate { thread: serde_json::Value },
    ThreadDelete { thread_id: String, guild_id: String, parent_id: String },
    ThreadListSync { guild_id: String, threads: Vec<serde_json::Value> },
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
            gateway_url: Arc::new(RwLock::new("ws://localhost:9102".to_string())),
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

        // Guilds — 100, 101 (updated with new channel IDs)
        self.guilds.insert(Id::new(100), Guild {
            id: Id::new(100),
            name: "Australiana".into(),
            owner_id: Id::new(1),
            channels: vec![
                Id::new(200), Id::new(201),
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
