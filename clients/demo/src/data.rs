//! Demo data generators for testing the Poly UI.
//!
//! Generates rich mock data: 3 servers for the "cat" demo account,
//! 4 servers for the "dog" demo account, 12+ channels, 10 users, messages
//! with images/reactions/edits, DMs, groups, and notifications.
//!
//! Two demo accounts are provided to illustrate multi-account support:
//! - `demo` (🐱 cat) — 3 servers (Poly Dev, Gaming Lounge, Music Enthusiasts)
//! - `demo2` (🐶 dog) — 4 servers (Open Source Hub, Book Club, Cooking Corner, Fitness Crew)
//!
//! SAFETY NOTE: indexing_slicing is allowed in this module because all indices
//! are bounded by the fixed-size `demo_users()` slice, which is compile-time
//! constant mock data. This is intentional for readability in test/demo code.
#![allow(clippy::indexing_slicing)]

use chrono::{Duration, Utc};
use dioxus::prelude::*;
use poly_client::*;
use rand::distr::{Alphanumeric, SampleString};

/// Bundled cat avatar image for the cat demo account.
const DEMO_CAT_AVATAR: Asset = asset!("assets/cat.png");
/// Bundled dog avatar image for the dog demo account.
const DEMO_DOG_AVATAR: Asset = asset!("assets/dog.png");

/// The demo account ID used for all demo data (cat account).
pub const DEMO_ACCOUNT_ID: &str = "demo";

/// The demo account display name.
pub const DEMO_ACCOUNT_NAME: &str = "Cat Demo Account";

/// The second demo account ID (dog account).
pub const DEMO2_ACCOUNT_ID: &str = "demo2";

/// The second demo account display name.
pub const DEMO2_ACCOUNT_NAME: &str = "Dog Demo Account";

/// Generate a demo session for the cat account (demo).
pub fn demo_session() -> Session {
    Session {
        id: "demo-session-1".to_string(),
        user: User {
            id: "demo-user-self".to_string(),
            display_name: "Demo User (Cat)".to_string(),
            // The bundled cat.png is served by the Dioxus asset system; storing
            // the path in avatar_url means all UI components use the generic
            // avatar_url path — no demo-specific logic needed in UI code.
            avatar_url: Some(DEMO_CAT_AVATAR.to_string()),
            presence: PresenceStatus::Online,
            backend: BackendType::Demo,
        },
        token: "demo-token-not-real".to_string(),
        backend: BackendType::Demo,
        icon_emoji: Some("🐱".to_string()),
    }
}

/// Generate a demo session for the dog account (demo2).
pub fn demo2_session() -> Session {
    Session {
        id: "demo2-session-1".to_string(),
        user: User {
            id: "demo2-user-self".to_string(),
            display_name: "Demo User (Dog)".to_string(),
            avatar_url: Some(DEMO_DOG_AVATAR.to_string()),
            presence: PresenceStatus::Online,
            backend: BackendType::Demo,
        },
        token: "demo2-token-not-real".to_string(),
        backend: BackendType::Demo,
        icon_emoji: Some("🐶".to_string()),
    }
}

/// Generate a list of demo users.
pub fn demo_users() -> Vec<User> {
    let names = [
        ("user-alice", "Alice"),
        ("user-bob", "Bob"),
        ("user-charlie", "Charlie"),
        ("user-diana", "Diana"),
        ("user-eve", "Eve"),
        ("user-frank", "Frank"),
        ("user-grace", "Grace"),
        ("user-henry", "Henry"),
        ("user-iris", "Iris"),
        ("user-jack", "Jack"),
    ];

    let presences = [
        PresenceStatus::Online,
        PresenceStatus::Online,
        PresenceStatus::Idle,
        PresenceStatus::Online,
        PresenceStatus::DoNotDisturb,
        PresenceStatus::Offline,
        PresenceStatus::Online,
        PresenceStatus::Idle,
        PresenceStatus::Offline,
        PresenceStatus::Online,
    ];

    names
        .iter()
        .zip(presences.iter())
        .map(|((id, name), presence)| User {
            id: id.to_string(),
            display_name: name.to_string(),
            avatar_url: None,
            presence: *presence,
            backend: BackendType::Demo,
        })
        .collect()
}

/// Generate demo servers with categories.
///
/// Returns 3 servers from the demo account, each with multiple categories.
pub fn demo_servers() -> Vec<Server> {
    vec![
        Server {
            id: "server-poly-dev".to_string(),
            name: "Poly Development".to_string(),
            icon_url: None,
            categories: vec![
                Category {
                    id: "cat-general".to_string(),
                    name: "General".to_string(),
                    channel_ids: vec!["ch-general".to_string(), "ch-off-topic".to_string()],
                },
                Category {
                    id: "cat-dev".to_string(),
                    name: "Development".to_string(),
                    channel_ids: vec![
                        "ch-rust".to_string(),
                        "ch-dioxus".to_string(),
                        "ch-voice-dev".to_string(),
                    ],
                },
            ],
            backend: BackendType::Demo,
            unread_count: 5,
            account_id: DEMO_ACCOUNT_ID.to_string(),
            account_display_name: DEMO_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-gaming".to_string(),
            name: "Gaming Lounge".to_string(),
            icon_url: None,
            categories: vec![Category {
                id: "cat-games".to_string(),
                name: "Games".to_string(),
                channel_ids: vec![
                    "ch-minecraft".to_string(),
                    "ch-valorant".to_string(),
                    "ch-voice-gaming".to_string(),
                ],
            }],
            backend: BackendType::Demo,
            unread_count: 12,
            account_id: DEMO_ACCOUNT_ID.to_string(),
            account_display_name: DEMO_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-music".to_string(),
            name: "Music Enthusiasts".to_string(),
            icon_url: None,
            categories: vec![Category {
                id: "cat-music".to_string(),
                name: "Music".to_string(),
                channel_ids: vec![
                    "ch-recommendations".to_string(),
                    "ch-production".to_string(),
                ],
            }],
            backend: BackendType::Demo,
            unread_count: 0,
            account_id: DEMO_ACCOUNT_ID.to_string(),
            account_display_name: DEMO_ACCOUNT_NAME.to_string(),
        },
    ]
}

/// Generate servers for the second demo account (dog 🐶 / demo2).
///
/// 4 servers to illustrate that the dog account has different communities
/// than the cat account. These appear in Bar 2 when demo2 is the active account,
/// and the favorites bar can show icons from both accounts side-by-side.
pub fn demo2_servers() -> Vec<Server> {
    vec![
        Server {
            id: "server-opensource".to_string(),
            name: "Open Source Hub".to_string(),
            icon_url: None,
            categories: vec![
                Category {
                    id: "cat-projects".to_string(),
                    name: "Projects".to_string(),
                    channel_ids: vec![
                        "ch2-announcements".to_string(),
                        "ch2-contributions".to_string(),
                    ],
                },
                Category {
                    id: "cat-support".to_string(),
                    name: "Support".to_string(),
                    channel_ids: vec!["ch2-help".to_string(), "ch2-voice-oss".to_string()],
                },
            ],
            backend: BackendType::Demo,
            unread_count: 3,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
            account_display_name: DEMO2_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-bookclub".to_string(),
            name: "Book Club".to_string(),
            icon_url: None,
            categories: vec![Category {
                id: "cat-books".to_string(),
                name: "Books".to_string(),
                channel_ids: vec![
                    "ch2-current-read".to_string(),
                    "ch2-recommendations".to_string(),
                    "ch2-voice-book".to_string(),
                ],
            }],
            backend: BackendType::Demo,
            unread_count: 7,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
            account_display_name: DEMO2_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-cooking".to_string(),
            name: "Cooking Corner".to_string(),
            icon_url: None,
            categories: vec![Category {
                id: "cat-food".to_string(),
                name: "Food".to_string(),
                channel_ids: vec![
                    "ch2-recipes".to_string(),
                    "ch2-techniques".to_string(),
                    "ch2-show-your-dish".to_string(),
                ],
            }],
            backend: BackendType::Demo,
            unread_count: 0,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
            account_display_name: DEMO2_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-fitness".to_string(),
            name: "Fitness Crew".to_string(),
            icon_url: None,
            categories: vec![Category {
                id: "cat-health".to_string(),
                name: "Health".to_string(),
                channel_ids: vec![
                    "ch2-workouts".to_string(),
                    "ch2-nutrition".to_string(),
                    "ch2-voice-workout".to_string(),
                ],
            }],
            backend: BackendType::Demo,
            unread_count: 2,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
            account_display_name: DEMO2_ACCOUNT_NAME.to_string(),
        },
    ]
}

/// Generate channels for demo2 servers.
pub fn demo2_channels(server_id: &str) -> Vec<Channel> {
    match server_id {
        "server-opensource" => vec![
            Channel {
                id: "ch2-announcements".to_string(),
                name: "announcements".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 1,
                last_message_id: Some("msg2-1".to_string()),
            },
            Channel {
                id: "ch2-contributions".to_string(),
                name: "contributions".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 2,
                last_message_id: Some("msg2-2".to_string()),
            },
            Channel {
                id: "ch2-help".to_string(),
                name: "help".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
            Channel {
                id: "ch2-voice-oss".to_string(),
                name: "Dev Chat".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
        ],
        "server-bookclub" => vec![
            Channel {
                id: "ch2-current-read".to_string(),
                name: "current-read".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 5,
                last_message_id: Some("msg2-10".to_string()),
            },
            Channel {
                id: "ch2-recommendations".to_string(),
                name: "recommendations".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 2,
                last_message_id: Some("msg2-11".to_string()),
            },
            Channel {
                id: "ch2-voice-book".to_string(),
                name: "Reading Night".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
        ],
        "server-cooking" => vec![
            Channel {
                id: "ch2-recipes".to_string(),
                name: "recipes".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: Some("msg2-20".to_string()),
            },
            Channel {
                id: "ch2-techniques".to_string(),
                name: "techniques".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
            Channel {
                id: "ch2-show-your-dish".to_string(),
                name: "show-your-dish".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
        ],
        "server-fitness" => vec![
            Channel {
                id: "ch2-workouts".to_string(),
                name: "workouts".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 1,
                last_message_id: Some("msg2-30".to_string()),
            },
            Channel {
                id: "ch2-nutrition".to_string(),
                name: "nutrition".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 1,
                last_message_id: Some("msg2-31".to_string()),
            },
            Channel {
                id: "ch2-voice-workout".to_string(),
                name: "Workout Call".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
        ],
        _ => vec![],
    }
}

/// Generate messages for demo2 servers (minimal set for UI testing).
pub fn demo2_messages(channel_id: &str) -> Vec<Message> {
    let users = demo_users();
    let now = Utc::now();
    match channel_id {
        "ch2-announcements" => vec![
            Message {
                id: "msg2-1".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Welcome to the Open Source Hub! 🎉 Check out our pinned projects.".to_string(),
                ),
                timestamp: now - Duration::hours(3),
                edited: false,
                reactions: vec![],
                attachments: vec![],
            },
            Message {
                id: "msg2-2".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "Just merged a big PR! See the contributions channel for discussion."
                        .to_string(),
                ),
                timestamp: now - Duration::hours(1),
                edited: false,
                reactions: vec![],
                attachments: vec![],
            },
        ],
        "ch2-current-read" => vec![Message {
            id: "msg2-10".to_string(),
            author: users[2].clone(),
            content: MessageContent::Text(
                "Finished chapter 12 — what does everyone think about the plot twist?".to_string(),
            ),
            timestamp: now - Duration::hours(5),
            edited: false,
            reactions: vec![Reaction {
                emoji: "📚".to_string(),
                count: 4,
                me: true,
            }],
            attachments: vec![],
        }],
        "ch2-workouts" => vec![Message {
            id: "msg2-30".to_string(),
            author: users[6].clone(),
            content: MessageContent::Text("5K run this morning! 💪 New personal best.".to_string()),
            timestamp: now - Duration::hours(2),
            edited: false,
            reactions: vec![Reaction {
                emoji: "🔥".to_string(),
                count: 6,
                me: false,
            }],
            attachments: vec![],
        }],
        _ => vec![],
    }
}

/// Generate notifications for demo2 account.
pub fn demo2_notifications() -> Vec<Notification> {
    let now = Utc::now();
    vec![
        Notification {
            id: "notif2-1".to_string(),
            kind: NotificationKind::Mention {
                channel_id: "ch2-current-read".to_string(),
                message_id: "msg2-10".to_string(),
            },
            read: false,
            timestamp: now - Duration::hours(2),
            backend: BackendType::Demo,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
            preview: "New discussion in #current-read".to_string(),
        },
        Notification {
            id: "notif2-2".to_string(),
            kind: NotificationKind::Mention {
                channel_id: "ch2-workouts".to_string(),
                message_id: "msg2-30".to_string(),
            },
            read: false,
            timestamp: now - Duration::hours(4),
            backend: BackendType::Demo,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
            preview: "Bob posted a new workout challenge".to_string(),
        },
    ]
}

/// Generate channels for a given server.
pub fn demo_channels(server_id: &str) -> Vec<Channel> {
    match server_id {
        "server-poly-dev" => vec![
            Channel {
                id: "ch-general".to_string(),
                name: "general".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 3,
                last_message_id: Some("msg-10".to_string()),
            },
            Channel {
                id: "ch-off-topic".to_string(),
                name: "off-topic".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 2,
                last_message_id: Some("msg-20".to_string()),
            },
            Channel {
                id: "ch-rust".to_string(),
                name: "rust-help".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: Some("msg-30".to_string()),
            },
            Channel {
                id: "ch-dioxus".to_string(),
                name: "dioxus".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
            Channel {
                id: "ch-voice-dev".to_string(),
                name: "Dev Voice".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
        ],
        "server-gaming" => vec![
            Channel {
                id: "ch-minecraft".to_string(),
                name: "minecraft".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 7,
                last_message_id: Some("msg-40".to_string()),
            },
            Channel {
                id: "ch-valorant".to_string(),
                name: "valorant".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 5,
                last_message_id: Some("msg-50".to_string()),
            },
            Channel {
                id: "ch-voice-gaming".to_string(),
                name: "Gaming Voice".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
        ],
        "server-music" => vec![
            Channel {
                id: "ch-recommendations".to_string(),
                name: "recommendations".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: Some("msg-60".to_string()),
            },
            Channel {
                id: "ch-production".to_string(),
                name: "production".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                last_message_id: None,
            },
        ],
        _ => vec![],
    }
}

/// Generate demo messages for a channel.
///
/// Returns messages with realistic timestamps spread across multiple days,
/// multi-line content, image attachments, reactions, and edited flags.
pub fn demo_messages(channel_id: &str) -> Vec<Message> {
    let users = demo_users();
    let now = Utc::now();

    match channel_id {
        "ch-general" => vec![
            // — Day 1: Two days ago —
            Message {
                id: "msg-ch-general-0".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Hey everyone! Welcome to the Poly Development server 👋\n\nExcited to have you all here. Let's build something amazing together!"
                        .to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(3),
                attachments: vec![],
                reactions: vec![
                    Reaction { emoji: "👋".to_string(), count: 5, me: true },
                    Reaction { emoji: "🎉".to_string(), count: 3, me: false },
                ],
                edited: false,
            },
            Message {
                id: "msg-ch-general-1".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "Thanks for having me! This project looks really cool."
                        .to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(2) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "❤️".to_string(), count: 2, me: false }],
                edited: false,
            },
            Message {
                id: "msg-ch-general-2".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Has anyone tried the new Dioxus 0.7 hot-reload? It's blazing fast!"
                        .to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(2) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🔥".to_string(), count: 4, me: true }],
                edited: false,
            },
            // Same author within 7 minutes — should be grouped
            Message {
                id: "msg-ch-general-3".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Yeah, subsecond hot-patch is a game changer for development.\nI tested it with a massive component tree and it works flawlessly."
                        .to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(2) - Duration::minutes(42),
                attachments: vec![],
                reactions: vec![],
                edited: true,
            },
            // — Day 2: Yesterday —
            Message {
                id: "msg-ch-general-4".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "I just pushed some updates to the theme engine. Check it out!"
                        .to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5),
                attachments: vec![
                    Attachment {
                        id: "att-screenshot-1".to_string(),
                        filename: "theme-preview.png".to_string(),
                        content_type: "image/png".to_string(),
                        url: "https://picsum.photos/seed/theme/400/250".to_string(),
                        size: 245_760,
                    },
                ],
                reactions: vec![
                    Reaction { emoji: "😍".to_string(), count: 3, me: false },
                    Reaction { emoji: "👍".to_string(), count: 2, me: true },
                ],
                edited: false,
            },
            Message {
                id: "msg-ch-general-5".to_string(),
                author: users[5].clone(),
                content: MessageContent::Text(
                    "The SurrealDB integration is coming along nicely.\n\nHere's the architectural diagram I drafted:"
                        .to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(4),
                attachments: vec![
                    Attachment {
                        id: "att-diagram-1".to_string(),
                        filename: "architecture.png".to_string(),
                        content_type: "image/png".to_string(),
                        url: "https://picsum.photos/seed/arch/500/300".to_string(),
                        size: 512_000,
                    },
                ],
                reactions: vec![],
                edited: false,
            },
            Message {
                id: "msg-ch-general-6".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "Anyone up for a code review session later today?"
                        .to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(2),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            },
            Message {
                id: "msg-ch-general-7".to_string(),
                author: users[7].clone(),
                content: MessageContent::Text(
                    "Sure! I'll be free around 3pm."
                        .to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(1) - Duration::minutes(55),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👍".to_string(), count: 1, me: false }],
                edited: false,
            },
            // — Today —
            Message {
                id: "msg-ch-general-8".to_string(),
                author: users[8].clone(),
                content: MessageContent::Text(
                    "Does anyone know if SurrealKV works on Android yet?"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(2),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            },
            Message {
                id: "msg-ch-general-9".to_string(),
                author: users[9].clone(),
                content: MessageContent::Text(
                    "We should test that early. It's flagged as a risk in the plan.\n\nI can try spinning up the Android emulator this afternoon to test. 📱"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(1) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🙏".to_string(), count: 2, me: false }],
                edited: false,
            },
            // Same author, within 7 min — grouped
            Message {
                id: "msg-ch-general-10".to_string(),
                author: users[9].clone(),
                content: MessageContent::Text(
                    "Also here's a doc I found on the topic:".to_string(),
                ),
                timestamp: now - Duration::hours(1) - Duration::minutes(28),
                attachments: vec![
                    Attachment {
                        id: "att-doc-1".to_string(),
                        filename: "surrealkv-mobile-notes.pdf".to_string(),
                        content_type: "application/pdf".to_string(),
                        url: "https://example.com/surrealkv-notes.pdf".to_string(),
                        size: 1_048_576,
                    },
                ],
                reactions: vec![],
                edited: false,
            },
        ],
        "ch-off-topic" => vec![
            Message {
                id: "msg-ch-off-topic-0".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text(
                    "Check out this sunset photo I took yesterday! 🌅".to_string(),
                ),
                timestamp: now - Duration::hours(8),
                attachments: vec![
                    Attachment {
                        id: "att-sunset".to_string(),
                        filename: "sunset.jpg".to_string(),
                        content_type: "image/jpeg".to_string(),
                        url: "https://picsum.photos/seed/sunset/600/400".to_string(),
                        size: 2_097_152,
                    },
                ],
                reactions: vec![
                    Reaction { emoji: "😍".to_string(), count: 4, me: true },
                    Reaction { emoji: "📸".to_string(), count: 2, me: false },
                ],
                edited: false,
            },
            Message {
                id: "msg-ch-off-topic-1".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "That's gorgeous! Where was this?"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(7) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            },
            Message {
                id: "msg-ch-off-topic-2".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text(
                    "Taken from the rooftop of my apartment building in Berlin 🇩🇪\n\nThe light was perfect around 7:30pm."
                        .to_string(),
                ),
                timestamp: now - Duration::hours(7) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🇩🇪".to_string(), count: 1, me: false }],
                edited: false,
            },
        ],
        "ch-minecraft" => vec![
            Message {
                id: "msg-ch-minecraft-0".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "Who wants to play Minecraft tonight? 🎮".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(6),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🙋".to_string(), count: 3, me: true }],
                edited: false,
            },
            Message {
                id: "msg-ch-minecraft-1".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text("I'm in! What time?".to_string()),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            },
            Message {
                id: "msg-ch-minecraft-2".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text("Let's do 8pm EST".to_string()),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(40),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👍".to_string(), count: 2, me: false }],
                edited: false,
            },
            Message {
                id: "msg-ch-minecraft-3".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "I built a massive redstone contraption, you all need to see it!"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(10),
                attachments: vec![
                    Attachment {
                        id: "att-minecraft".to_string(),
                        filename: "redstone-build.png".to_string(),
                        content_type: "image/png".to_string(),
                        url: "https://picsum.photos/seed/minecraft/400/300".to_string(),
                        size: 384_000,
                    },
                ],
                reactions: vec![
                    Reaction { emoji: "🤯".to_string(), count: 5, me: true },
                    Reaction { emoji: "❤️".to_string(), count: 2, me: false },
                ],
                edited: false,
            },
            Message {
                id: "msg-ch-minecraft-4".to_string(),
                author: users[9].clone(),
                content: MessageContent::Text(
                    "The new update is amazing, have you tried the new biomes?"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            },
        ],
        _ => vec![
            Message {
                id: format!("msg-{channel_id}-0"),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Hello from this channel!".to_string(),
                ),
                timestamp: now - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            },
            Message {
                id: format!("msg-{channel_id}-1"),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "Nice to see some activity here.".to_string(),
                ),
                timestamp: now - Duration::hours(5),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            },
            Message {
                id: format!("msg-{channel_id}-2"),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Let's keep the conversation going! 😊".to_string(),
                ),
                timestamp: now - Duration::hours(1),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "😊".to_string(), count: 1, me: false }],
                edited: false,
            },
        ],
    }
}

/// Generate a demo sent message.
pub fn demo_sent_message(_channel_id: &str, content: MessageContent) -> Message {
    Message {
        id: format!(
            "msg-sent-{}",
            Alphanumeric.sample_string(&mut rand::rng(), 16)
        ),
        author: demo_session().user,
        content,
        timestamp: Utc::now(),
        attachments: vec![],
        reactions: vec![],
        edited: false,
    }
}

/// Generate demo group chats.
pub fn demo_groups() -> Vec<Group> {
    let users = demo_users();
    vec![
        Group {
            id: "group-1".to_string(),
            members: users[..3].to_vec(),
            name: Some("Project Team".to_string()),
            last_message: Some(Message {
                id: "msg-group-1".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text("Meeting at 5pm today".to_string()),
                timestamp: Utc::now() - Duration::hours(1),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO_ACCOUNT_ID.to_string(),
        },
        Group {
            id: "group-2".to_string(),
            members: users[3..6].to_vec(),
            name: Some("Weekend Plans".to_string()),
            last_message: Some(Message {
                id: "msg-group-2".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text("How about Saturday?".to_string()),
                timestamp: Utc::now() - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO_ACCOUNT_ID.to_string(),
        },
    ]
}

/// Generate demo DM channels.
pub fn demo_dm_channels() -> Vec<DmChannel> {
    let users = demo_users();
    users
        .iter()
        .take(5)
        .enumerate()
        .map(|(i, user)| DmChannel {
            id: format!("dm-{}", user.id),
            user: user.clone(),
            last_message: Some(Message {
                id: format!("msg-dm-{i}"),
                author: user.clone(),
                content: MessageContent::Text("Hey, how's it going?".to_string()),
                timestamp: Utc::now() - Duration::hours(i as i64 * 2),
                attachments: vec![],
                reactions: vec![],
                edited: false,
            }),
            unread_count: if i < 2 { 1 } else { 0 },
            backend: BackendType::Demo,
            account_id: DEMO_ACCOUNT_ID.to_string(),
        })
        .collect()
}

/// Generate demo notifications.
pub fn demo_notifications() -> Vec<Notification> {
    let now = Utc::now();
    vec![
        Notification {
            id: "notif-1".to_string(),
            kind: NotificationKind::Mention {
                channel_id: "ch-general".to_string(),
                message_id: "msg-ch-general-2".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::minutes(10),
            read: false,
            preview: "Alice mentioned you in #general".to_string(),
        },
        Notification {
            id: "notif-2".to_string(),
            kind: NotificationKind::FriendRequest {
                from_user_id: "user-iris".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::hours(1),
            read: false,
            preview: "Iris sent you a friend request".to_string(),
        },
        Notification {
            id: "notif-3".to_string(),
            kind: NotificationKind::ServerInvite {
                server_id: "server-new".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::hours(5),
            read: true,
            preview: "You've been invited to Rust Community".to_string(),
        },
    ]
}
/// Generate demo voice participants for a given voice channel.
///
/// Returns realistic-looking participants for the two demo voice channels.
/// Real clients get this from the server; the demo client provides static data.
pub fn demo_voice_participants(channel_id: &str) -> Vec<VoiceParticipant> {
    let users = demo_users();
    match channel_id {
        "ch-voice-dev" => vec![
            VoiceParticipant {
                user: users[0].clone(), // Alice
                is_muted: false,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: true,
            },
            VoiceParticipant {
                user: users[2].clone(), // Charlie
                is_muted: true,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            },
            VoiceParticipant {
                user: users[6].clone(), // Grace
                is_muted: false,
                is_deafened: false,
                is_streaming: true,
                is_video_on: false,
                is_speaking: false,
            },
        ],
        "ch-voice-gaming" => vec![
            VoiceParticipant {
                user: users[1].clone(), // Bob
                is_muted: false,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: true,
            },
            VoiceParticipant {
                user: users[3].clone(), // Diana
                is_muted: false,
                is_deafened: true,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            },
            VoiceParticipant {
                user: users[9].clone(), // Jack
                is_muted: true,
                is_deafened: false,
                is_streaming: false,
                is_video_on: true,
                is_speaking: false,
            },
            VoiceParticipant {
                user: users[4].clone(), // Eve
                is_muted: false,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            },
        ],
        _ => vec![],
    }
}
