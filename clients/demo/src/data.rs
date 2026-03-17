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
#[cfg(feature = "native")]
use dioxus::prelude::*;
use poly_client::*;
use rand::distr::{Alphanumeric, SampleString};

/// Encode SVG markup as a data URI so demo artwork is self-contained.
fn svg_data_uri(svg: String) -> String {
    let encoded = svg
        .replace('%', "%25")
        .replace('#', "%23")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('"', "%22")
        .replace('\'', "%27")
        .replace('{', "%7B")
        .replace('}', "%7D")
        .replace('|', "%7C")
        .replace('\\', "%5C")
        .replace('^', "%5E")
        .replace('`', "%60")
        .replace("\n", "")
        .replace("\r", "")
        .replace(' ', "%20");
    format!("data:image/svg+xml;utf8,{encoded}")
}

/// Animal-style demo avatar.
fn animal_avatar(emoji: &str, primary: &str, secondary: &str) -> String {
    svg_data_uri(format!(
        r#"
<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 96 96'>
    <defs>
        <linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>
            <stop offset='0%' stop-color='{primary}'/>
            <stop offset='100%' stop-color='{secondary}'/>
        </linearGradient>
    </defs>
    <circle cx='48' cy='48' r='46' fill='url(#g)'/>
    <circle cx='48' cy='48' r='32' fill='rgba(10,12,24,0.14)'/>
    <text x='48' y='58' text-anchor='middle' font-size='34'>{emoji}</text>
</svg>
                "#
    ))
}

/// Server icon matched to the server name/theme.
fn themed_server_icon(symbol: &str, primary: &str, secondary: &str) -> String {
    svg_data_uri(format!(
        r#"
<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 96 96'>
    <defs>
        <linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>
            <stop offset='0%' stop-color='{primary}'/>
            <stop offset='100%' stop-color='{secondary}'/>
        </linearGradient>
    </defs>
    <rect x='4' y='4' width='88' height='88' rx='26' fill='url(#g)'/>
    <rect x='10' y='10' width='76' height='76' rx='22' fill='rgba(255,255,255,0.10)'/>
    <text x='48' y='58' text-anchor='middle' font-size='32'>{symbol}</text>
</svg>
                "#
    ))
}

/// Wide banner artwork matched to the server theme.
fn themed_banner(primary: &str, secondary: &str, accent: &str, symbol: &str) -> String {
    svg_data_uri(format!(
        r#"
<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 960 240'>
    <defs>
        <linearGradient id='bg' x1='0' y1='0' x2='1' y2='1'>
            <stop offset='0%' stop-color='{primary}'/>
            <stop offset='100%' stop-color='{secondary}'/>
        </linearGradient>
    </defs>
    <rect width='960' height='240' fill='url(#bg)'/>
    <circle cx='132' cy='70' r='130' fill='rgba(255,255,255,0.08)'/>
    <circle cx='836' cy='42' r='110' fill='rgba(255,255,255,0.06)'/>
    <path d='M0 188 C170 134 312 226 468 170 S764 92 960 150' stroke='{accent}' stroke-opacity='0.55' stroke-width='14' fill='none' stroke-linecap='round'/>
    <rect x='0' y='170' width='960' height='70' fill='rgba(8,10,22,0.24)'/>
    <text x='812' y='148' text-anchor='middle' font-size='86' fill='rgba(255,255,255,0.18)'>{symbol}</text>
</svg>
                "#
    ))
}

/// Bundled cat avatar image for the cat demo account.
/// On native (Dioxus) builds, uses the asset system. On WASM plugin builds,
/// falls back to a placeholder string.
#[cfg(feature = "native")]
pub const DEMO_CAT_AVATAR: Asset = asset!("assets/cat.png");
/// Bundled dog avatar image for the dog demo account.
#[cfg(feature = "native")]
pub const DEMO_DOG_AVATAR: Asset = asset!("assets/dog.png");

/// Cat avatar as a plain string for WASM plugin builds.
#[cfg(not(feature = "native"))]
pub const DEMO_CAT_AVATAR: &str = "/assets/cat.png";
/// Dog avatar as a plain string for WASM plugin builds.
#[cfg(not(feature = "native"))]
pub const DEMO_DOG_AVATAR: &str = "/assets/dog.png";

/// The demo account ID used for all demo data (cat account).
pub const DEMO_ACCOUNT_ID: &str = "demo-cat";

/// The demo account display name.
pub const DEMO_ACCOUNT_NAME: &str = "Cat (demo)";

/// The second demo account ID (dog account).
pub const DEMO2_ACCOUNT_ID: &str = "demo-dog";

/// The second demo account display name.
pub const DEMO2_ACCOUNT_NAME: &str = "Dog (demo)";

/// The shared demo instance ID — both demo accounts live on this virtual instance.
pub const DEMO_INSTANCE_ID: &str = "demo";

/// Generate a demo session for the cat account (demo-cat).
pub fn demo_session() -> Session {
    Session {
        id: "demo-session-1".to_string(),
        user: User {
            id: "demo-user-self".to_string(),
            display_name: "Cat (demo)".to_string(),
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
        instance_id: DEMO_INSTANCE_ID.to_string(),
        backend_url: None,
    }
}

/// Generate a demo session for the dog account (demo-dog).
pub fn demo2_session() -> Session {
    Session {
        id: "demo2-session-1".to_string(),
        user: User {
            id: "demo2-user-self".to_string(),
            display_name: "Dog (demo)".to_string(),
            avatar_url: Some(DEMO_DOG_AVATAR.to_string()),
            presence: PresenceStatus::Online,
            backend: BackendType::Demo,
        },
        token: "demo2-token-not-real".to_string(),
        backend: BackendType::Demo,
        icon_emoji: Some("🐶".to_string()),
        instance_id: DEMO_INSTANCE_ID.to_string(),
        backend_url: None,
    }
}

/// Generate a list of demo users.
///
/// Users are assigned deterministic avatar images via the DiceBear API so the
/// UI shows illustrated portraits rather than colored-letter placeholders.
/// Emoji prefixes have been removed from display names — the avatars convey
/// identity visually.
pub fn demo_users() -> Vec<User> {
    let names = [
        (
            "user-alice",
            "Alice",
            animal_avatar("🐱", "#ff7eb6", "#8f5bff"),
        ),
        ("user-bob", "Bob", animal_avatar("🐶", "#f6c453", "#f58b54")),
        (
            "user-charlie",
            "Charlie",
            animal_avatar("🦊", "#ff8c5a", "#ff4f81"),
        ),
        (
            "user-diana",
            "Diana",
            animal_avatar("🐰", "#f7a8ff", "#7d7cff"),
        ),
        ("user-eve", "Eve", animal_avatar("🦦", "#50d2c2", "#2274a5")),
        (
            "user-frank",
            "Frank",
            animal_avatar("🐻", "#b67f52", "#6d4c41"),
        ),
        (
            "user-grace",
            "Grace",
            animal_avatar("🦌", "#55d6be", "#3478f6"),
        ),
        (
            "user-henry",
            "Henry",
            animal_avatar("🐺", "#94a3b8", "#475569"),
        ),
        (
            "user-iris",
            "Iris",
            animal_avatar("🐼", "#e5e7eb", "#7c3aed"),
        ),
        (
            "user-jack",
            "Jack",
            animal_avatar("🐯", "#fb923c", "#ef4444"),
        ),
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
        .map(|((id, name, avatar_url), presence)| User {
            id: id.to_string(),
            display_name: name.to_string(),
            avatar_url: Some(avatar_url.clone()),
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
            icon_url: Some(themed_server_icon("⌘", "#6d5efc", "#2ac3ff")),
            banner_url: Some(themed_banner("#0f1f52", "#182f7a", "#5eead4", "⌘")),
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
            mention_count: 2,
            account_id: DEMO_ACCOUNT_ID.to_string(),
            account_display_name: DEMO_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-gaming".to_string(),
            name: "Gaming Lounge".to_string(),
            icon_url: Some(themed_server_icon("🎮", "#6d28d9", "#ec4899")),
            banner_url: Some(themed_banner("#2f174d", "#601a7f", "#f472b6", "🎮")),
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
            mention_count: 0,
            account_id: DEMO_ACCOUNT_ID.to_string(),
            account_display_name: DEMO_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-music".to_string(),
            name: "Music Enthusiasts".to_string(),
            icon_url: Some(themed_server_icon("♪", "#0ea5e9", "#14b8a6")),
            banner_url: Some(themed_banner("#0f3150", "#125f73", "#f8fafc", "♪")),
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
            mention_count: 0,
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
            icon_url: Some(themed_server_icon("⎇", "#22c55e", "#0ea5e9")),
            banner_url: Some(themed_banner("#113a24", "#0f4c64", "#86efac", "⎇")),
            categories: vec![
                Category {
                    id: "cat-projects".to_string(),
                    name: "Projects".to_string(),
                    channel_ids: vec![
                        "ch2-general".to_string(),
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
            unread_count: 14,
            mention_count: 3,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
            account_display_name: DEMO2_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-bookclub".to_string(),
            name: "Book Club".to_string(),
            icon_url: Some(themed_server_icon("📚", "#f59e0b", "#f97316")),
            banner_url: Some(themed_banner("#5b3213", "#8a4d18", "#fde68a", "📚")),
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
            mention_count: 0,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
            account_display_name: DEMO2_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-cooking".to_string(),
            name: "Cooking Corner".to_string(),
            icon_url: Some(themed_server_icon("🍳", "#f97316", "#ef4444")),
            banner_url: Some(themed_banner("#5f2213", "#8b2d2d", "#fdba74", "🍳")),
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
            mention_count: 0,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
            account_display_name: DEMO2_ACCOUNT_NAME.to_string(),
        },
        Server {
            id: "server-fitness".to_string(),
            name: "Fitness Crew".to_string(),
            icon_url: Some(themed_server_icon("💪", "#10b981", "#0f766e")),
            banner_url: Some(themed_banner("#0f3d31", "#105248", "#6ee7b7", "💪")),
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
            mention_count: 1,
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
                id: "ch2-general".to_string(),
                name: "general".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 11,
                mention_count: 3,
                last_message_id: Some("msg2-general-559".to_string()),
            },
            Channel {
                id: "ch2-announcements".to_string(),
                name: "announcements".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 1,
                mention_count: 0,
                last_message_id: Some("msg2-1".to_string()),
            },
            Channel {
                id: "ch2-contributions".to_string(),
                name: "contributions".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 2,
                mention_count: 0,
                last_message_id: Some("msg2-2".to_string()),
            },
            Channel {
                id: "ch2-help".to_string(),
                name: "help".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
            },
            Channel {
                id: "ch2-voice-oss".to_string(),
                name: "Dev Chat".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
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
                mention_count: 0,
                last_message_id: Some("msg2-10".to_string()),
            },
            Channel {
                id: "ch2-recommendations".to_string(),
                name: "recommendations".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 2,
                mention_count: 0,
                last_message_id: Some("msg2-11".to_string()),
            },
            Channel {
                id: "ch2-voice-book".to_string(),
                name: "Reading Night".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
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
                mention_count: 0,
                last_message_id: Some("msg2-20".to_string()),
            },
            Channel {
                id: "ch2-techniques".to_string(),
                name: "techniques".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
            },
            Channel {
                id: "ch2-show-your-dish".to_string(),
                name: "show-your-dish".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
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
                mention_count: 1,
                last_message_id: Some("msg2-30".to_string()),
            },
            Channel {
                id: "ch2-nutrition".to_string(),
                name: "nutrition".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 1,
                mention_count: 0,
                last_message_id: Some("msg2-31".to_string()),
            },
            Channel {
                id: "ch2-voice-workout".to_string(),
                name: "Workout Call".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
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
                reply_to: None,
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
                reply_to: None,
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
            reply_to: None,
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
            reply_to: None,
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
                mention_count: 2,
                last_message_id: Some("msg-10".to_string()),
            },
            Channel {
                id: "ch-off-topic".to_string(),
                name: "off-topic".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 2,
                mention_count: 0,
                last_message_id: Some("msg-20".to_string()),
            },
            Channel {
                id: "ch-rust".to_string(),
                name: "rust-help".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
                last_message_id: Some("msg-30".to_string()),
            },
            Channel {
                id: "ch-dioxus".to_string(),
                name: "dioxus".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
            },
            Channel {
                id: "ch-voice-dev".to_string(),
                name: "Dev Voice".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
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
                mention_count: 0,
                last_message_id: Some("msg-40".to_string()),
            },
            Channel {
                id: "ch-valorant".to_string(),
                name: "valorant".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 5,
                mention_count: 0,
                last_message_id: Some("msg-50".to_string()),
            },
            Channel {
                id: "ch-voice-gaming".to_string(),
                name: "Gaming Voice".to_string(),
                channel_type: ChannelType::Voice,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
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
                mention_count: 0,
                last_message_id: Some("msg-60".to_string()),
            },
            Channel {
                id: "ch-production".to_string(),
                name: "production".to_string(),
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
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
                reply_to: None,
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
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-ch-general-2".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Has anyone tried the new **Dioxus 0.7** hot-reload? It's blazing fast!\n\n- hot patches in seconds\n- keeps router state alive\n- makes UI iteration way less painful"
                        .to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(2) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🔥".to_string(), count: 4, me: true }],
                reply_to: None,
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
                reply_to: None,
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
                    Attachment::remote(
                        "att-screenshot-1".to_string(),
                        "theme-preview.png".to_string(),
                        "image/png".to_string(),
                        "https://picsum.photos/seed/theme/400/250".to_string(),
                        245_760,
                    ),
                ],
                reactions: vec![
                    Reaction { emoji: "😍".to_string(), count: 3, me: false },
                    Reaction { emoji: "👍".to_string(), count: 2, me: true },
                ],
                reply_to: None,
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
                    Attachment::remote(
                        "att-diagram-1".to_string(),
                        "architecture.png".to_string(),
                        "image/png".to_string(),
                        "https://picsum.photos/seed/arch/500/300".to_string(),
                        512_000,
                    ),
                ],
                reactions: vec![],
                reply_to: None,
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
                reply_to: None,
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
                reply_to: None,
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
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-ch-general-9".to_string(),
                author: users[9].clone(),
                content: MessageContent::Text(
                    "We should test that early. It's flagged as a risk in the plan.\n\n| Target | Status | Note |\n| --- | --- | --- |\n| Android | ⚠️ Pending | Need emulator pass |\n| iOS | ⚠️ Pending | Need device build |\n| Web | ✅ Green | WASM check already passes |\n\nI can try spinning up the Android emulator this afternoon to test. 📱"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(1) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🙏".to_string(), count: 2, me: false }],
                reply_to: None,
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
                    Attachment::remote(
                        "att-doc-1".to_string(),
                        "surrealkv-mobile-notes.pdf".to_string(),
                        "application/pdf".to_string(),
                        "https://example.com/surrealkv-notes.pdf".to_string(),
                        1_048_576,
                    ),
                ],
                reactions: vec![],
                reply_to: None,
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
                    Attachment::remote(
                        "att-sunset".to_string(),
                        "sunset.jpg".to_string(),
                        "image/jpeg".to_string(),
                        "https://picsum.photos/seed/sunset/600/400".to_string(),
                        2_097_152,
                    ),
                ],
                reactions: vec![
                    Reaction { emoji: "😍".to_string(), count: 4, me: true },
                    Reaction { emoji: "📸".to_string(), count: 2, me: false },
                ],
                reply_to: None,
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
                reply_to: None,
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
                reply_to: None,
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
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-ch-minecraft-1".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text("I'm in! What time?".to_string()),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-ch-minecraft-2".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text("Let's do 8pm EST".to_string()),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(40),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👍".to_string(), count: 2, me: false }],
                reply_to: None,
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
                    Attachment::remote(
                        "att-minecraft".to_string(),
                        "redstone-build.png".to_string(),
                        "image/png".to_string(),
                        "https://picsum.photos/seed/minecraft/400/300".to_string(),
                        384_000,
                    ),
                ],
                reactions: vec![
                    Reaction { emoji: "🤯".to_string(), count: 5, me: true },
                    Reaction { emoji: "❤️".to_string(), count: 2, me: false },
                ],
                reply_to: None,
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
                reply_to: None,
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
                reply_to: None,
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
                reply_to: None,
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
                reply_to: None,
        edited: false,
            },
        ],
    }
}

/// Generate a demo sent message.
pub fn demo_sent_message(_channel_id: &str, content: MessageContent) -> Message {
    let attachments = match &content {
        MessageContent::Text(_) => Vec::new(),
        MessageContent::WithAttachments { attachments, .. } => attachments.clone(),
    };

    Message {
        id: format!(
            "msg-sent-{}",
            Alphanumeric.sample_string(&mut rand::rng(), 16)
        ),
        author: demo_session().user,
        content,
        timestamp: Utc::now(),
        attachments,
        reactions: vec![],
        reply_to: None,
        edited: false,
    }
}

/// Generate a demo sent reply message.
pub fn demo_sent_reply_message(
    channel_id: &str,
    reply_to_message_id: &str,
    content: MessageContent,
) -> Message {
    let mut sent = demo_sent_message(channel_id, content);
    let preview = demo_messages_query(
        channel_id,
        &MessageQuery {
            before: None,
            after: None,
            around: None,
            limit: Some(100),
        },
    )
    .into_iter()
    .find(|m| m.id == reply_to_message_id)
    .map(|msg| MessageReplyPreview {
        message_id: msg.id,
        author_id: msg.author.id,
        author_display_name: msg.author.display_name,
        author_avatar_url: msg.author.avatar_url,
        snippet: match msg.content {
            MessageContent::Text(text) => text,
            MessageContent::WithAttachments { text, .. } => text,
        }
        .chars()
        .take(80)
        .collect(),
    });
    sent.reply_to = preview;
    sent
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
                reply_to: None,
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
                reply_to: None,
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO_ACCOUNT_ID.to_string(),
        },
    ]
}

/// Generate demo DM channels for cat account.
pub fn demo_dm_channels() -> Vec<DmChannel> {
    let users = demo_users();
    let now = Utc::now();
    let mut channels: Vec<DmChannel> = users
        .iter()
        .take(5)
        .enumerate()
        .map(|(i, user)| {
            let is_alice = user.id == "user-alice";
            DmChannel {
                id: format!("dm-{}", user.id),
                user: user.clone(),
                last_message: Some(Message {
                    id: format!("msg-dm-{i}"),
                    author: user.clone(),
                    content: MessageContent::Text(
                        if is_alice {
                            "Just ran it — compiles clean! The hot-reload with subsecond patches is incredible.".to_string()
                        } else {
                            "Hey, how's it going?".to_string()
                        }
                    ),
                    timestamp: now - Duration::hours(i as i64 * 2),
                    attachments: vec![],
                    reactions: vec![],
                    reply_to: None,
        edited: false,
                }),
                unread_count: if i < 2 { 1 } else { 0 },
                backend: BackendType::Demo,
                account_id: DEMO_ACCOUNT_ID.to_string(),
            }
        })
        .collect();

    // Add cross-account DM: cat sees dog
    channels.push(DmChannel {
        id: "dm-demo-dog".to_string(),
        user: User {
            id: "demo-dog-user".to_string(),
            display_name: "🐶 Dog (demo)".to_string(),
            avatar_url: Some(DEMO_DOG_AVATAR.to_string()),
            presence: PresenceStatus::Online,
            backend: BackendType::Demo,
        },
        last_message: Some(Message {
            id: "msg-dm-dog-latest".to_string(),
            author: User {
                id: "demo-dog-user".to_string(),
                display_name: "🐶 Dog (demo)".to_string(),
                avatar_url: Some(DEMO_DOG_AVATAR.to_string()),
                presence: PresenceStatus::Online,
                backend: BackendType::Demo,
            },
            content: MessageContent::Text(
                "bark bark! 🐕 haha, your pull request is almost as good as my naps 😼".to_string(),
            ),
            timestamp: now - Duration::hours(1),
            attachments: vec![],
            reactions: vec![],
            reply_to: None,
            edited: false,
        }),
        unread_count: 1,
        backend: BackendType::Demo,
        account_id: DEMO_ACCOUNT_ID.to_string(),
    });

    channels
}

/// Generate demo notifications.
pub fn demo_notifications() -> Vec<Notification> {
    let now = Utc::now();
    vec![
        // — @mention in a channel —
        Notification {
            id: "notif-1".to_string(),
            kind: NotificationKind::Mention {
                channel_id: "ch-general".to_string(),
                message_id: "msg-ch-general-2".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::minutes(5),
            read: false,
            preview: "Alice mentioned you in #general: \"@Cat have you tried Dioxus 0.7 hot-reload?\"".to_string(),
        },
        // — @mention in a server channel —
        Notification {
            id: "notif-4".to_string(),
            kind: NotificationKind::Mention {
                channel_id: "ch-rust".to_string(),
                message_id: "msg-30".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::minutes(20),
            read: false,
            preview: "Charlie mentioned you in #rust-help: \"@Cat can you help debug this lifetime error?\"".to_string(),
        },
        // — Friend request from Iris —
        Notification {
            id: "notif-2".to_string(),
            kind: NotificationKind::FriendRequest {
                from_user_id: "user-iris".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::minutes(45),
            read: false,
            preview: "Iris sent you a friend request".to_string(),
        },
        // — Friend request from Jack —
        Notification {
            id: "notif-5".to_string(),
            kind: NotificationKind::FriendRequest {
                from_user_id: "user-jack".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::hours(2),
            read: false,
            preview: "Jack sent you a friend request".to_string(),
        },
        // — Server invite —
        Notification {
            id: "notif-3".to_string(),
            kind: NotificationKind::ServerInvite {
                server_id: "server-new".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::hours(3),
            read: false,
            preview: "Diana invited you to join Rust Community".to_string(),
        },
        // — Another server invite —
        Notification {
            id: "notif-6".to_string(),
            kind: NotificationKind::ServerInvite {
                server_id: "server-art".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::hours(6),
            read: false,
            preview: "Grace invited you to join Digital Art Hub".to_string(),
        },
        // — Voice channel invite —
        Notification {
            id: "notif-7".to_string(),
            kind: NotificationKind::VoiceChannelInvite {
                server_id: "server-poly-dev".to_string(),
                channel_id: "ch-voice-dev".to_string(),
                channel_name: "Dev Voice".to_string(),
                inviter_user_id: "user-bob".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::hours(1),
            read: false,
            preview: "Bob is calling you to join Dev Voice in Poly Development".to_string(),
        },
        // — Another voice invite —
        Notification {
            id: "notif-8".to_string(),
            kind: NotificationKind::VoiceChannelInvite {
                server_id: "server-gaming".to_string(),
                channel_id: "ch-voice-gaming".to_string(),
                channel_name: "Gaming Voice".to_string(),
                inviter_user_id: "user-diana".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::hours(4),
            read: true,
            preview: "Diana invited you to Gaming Voice in Gaming Lounge".to_string(),
        },
        // — Unread message mention (already read) —
        Notification {
            id: "notif-9".to_string(),
            kind: NotificationKind::Mention {
                channel_id: "ch-off-topic".to_string(),
                message_id: "msg-ch-off-topic-0".to_string(),
            },
            backend: BackendType::Demo,
            account_id: "demo".to_string(),
            timestamp: now - Duration::hours(8),
            read: true,
            preview: "Eve mentioned you in #off-topic".to_string(),
        },
    ]
}

/// Generate a default demo content policy for settings preview.
pub fn demo_content_policy() -> ContentPolicy {
    ContentPolicy {
        sensitive_content_dm_friends: SensitiveContentLevel::Hide,
        sensitive_content_dm_others: SensitiveContentLevel::Hide,
        sensitive_content_server_channels: SensitiveContentLevel::Hide,
        dm_spam_filter: DmSpamFilterLevel::FilterNonFriends,
        allow_age_restricted_servers: false,
        allow_age_restricted_commands_in_dms: false,
        allow_dms_from_server_members: true,
        allow_message_requests: true,
        friend_request_from_everyone: true,
        friend_request_from_friends_of_friends: true,
        friend_request_from_server_members: true,
    }
}

/// Generate demo blocked users for the content & social settings page.
pub fn demo_blocked_users() -> Vec<BlockedUser> {
    vec![
        BlockedUser {
            user_id: "user-blocked-1".to_string(),
            display_name: "SpamBot9000".to_string(),
            avatar_url: None,
        },
        BlockedUser {
            user_id: "user-blocked-2".to_string(),
            display_name: "TrollUser42".to_string(),
            avatar_url: None,
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
        // Dog account (demo2) voice channels
        "ch2-voice-book" => vec![
            VoiceParticipant {
                user: users[5].clone(), // Frank
                is_muted: false,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: true,
            },
            VoiceParticipant {
                user: users[7].clone(), // Henry
                is_muted: true,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            },
        ],
        "ch2-voice-oss" => vec![
            VoiceParticipant {
                user: users[0].clone(), // Alice
                is_muted: false,
                is_deafened: false,
                is_streaming: true,
                is_video_on: false,
                is_speaking: true,
            },
            VoiceParticipant {
                user: users[2].clone(), // Charlie
                is_muted: false,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            },
        ],
        "ch2-voice-workout" => vec![VoiceParticipant {
            user: users[6].clone(), // Grace
            is_muted: false,
            is_deafened: false,
            is_streaming: false,
            is_video_on: false,
            is_speaking: true,
        }],
        _ => vec![],
    }
}

// ── DM Messages ──────────────────────────────────────────────────────────────

/// Generate unique DM conversation messages for a DM channel.
///
/// Each DM contact gets a personalised conversation thread instead of
/// the generic "Hey, how's it going?" placeholder.
pub fn demo_dm_messages(dm_channel_id: &str) -> Vec<Message> {
    let users = demo_users();
    let now = Utc::now();

    match dm_channel_id {
        // Alice — project excitement / code review
        "dm-user-alice" => vec![
            Message {
                id: "dm-alice-0".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Hey! Just saw your PR for the SurrealDB integration — looks really solid 🎉"
                        .to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👀".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-alice-1".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Thanks! Took me a while to figure out the SurrealKV path handling on Linux. Did you get a chance to test it on your side?".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(4) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-alice-2".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Just ran it — compiles clean! The hot-reload with subsecond patches is incredible.\n\n> One small nit: the error messages for auth failure could be more descriptive.\n\n```rust\nErr(ClientError::AuthFailed(message.to_string()))\n```".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(4) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-alice-3".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Good catch! I'll add proper error context in the next commit. Wrapping everything in `ClientError::AuthFailed` with the server message.".to_string(),
                ),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👍".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-alice-4".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text("Perfect. Merging once you push that 🚀".to_string()),
                timestamp: now - Duration::hours(2),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],

        // Bob — casual / gaming weekend
        "dm-user-bob" => vec![
            Message {
                id: "dm-bob-0".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "You playing anything this weekend? We're doing a Minecraft survival session Saturday night 🎮".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(8),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-bob-1".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Oh nice! What time? I might be free after 8pm.".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(7) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-bob-2".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "8pm EST works perfectly. Diana and Jack are also joining. We built a whole underground base last time 😄".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(7) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🙌".to_string(), count: 1, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-bob-3".to_string(),
                author: demo_session().user,
                content: MessageContent::Text("I'm in! See you then 🏰".to_string()),
                timestamp: now - Duration::days(2) - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-bob-4".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "Also — did you see the new Minecraft snapshot? The new biomes look wild.".to_string(),
                ),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],

        // Charlie — Rust help / lifetimes
        "dm-user-charlie" => vec![
            Message {
                id: "dm-charlie-0".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Hey, quick Rust question — I'm hitting a lifetime error I don't understand 😅\n\n`error[E0502]: cannot borrow 'data' as mutable because it is also borrowed as immutable`".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-charlie-1".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Ah classic! Can you share the snippet? Usually it's because you're holding a `&data` reference while trying to call a `&mut` method.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(55),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-charlie-2".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "```rust\nlet view = data.iter().find(|x| x.id == id);\ndata.push(new_item); // ← borrow error here\n```\n\nI need both at the same time 😬".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(40),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-charlie-3".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "You need to clone the result from `find` before calling `push`:\n\n```rust\nlet view = data.iter().find(|x| x.id == id).cloned();\ndata.push(new_item); // ✓ now works\n```\n\nThe `find` returns a `&T` which keeps the borrow alive unless you clone it.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(20),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🙏".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-charlie-4".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "That fixed it! Rust ownership never stops surprising me... but I'm getting there 💪".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(4),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🦀".to_string(), count: 2, me: true }],
                reply_to: None,
        edited: false,
            },
        ],

        // Diana — design / theme feedback
        "dm-user-diana" => vec![
            Message {
                id: "dm-diana-0".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "Hey! Wanted to get your opinion on the new purple theme variant. Does it feel too Discord-like?".to_string(),
                ),
                timestamp: now - Duration::hours(10),
                attachments: vec![
                    Attachment::remote(
                        "att-theme-preview".to_string(),
                        "purple-theme-preview.png".to_string(),
                        "image/png".to_string(),
                        "https://picsum.photos/seed/purple-theme/400/250".to_string(),
                        275_000,
                    ),
                ],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-diana-1".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Honestly I like it! The saturation is a bit lower than Discord's so it feels distinct. Maybe bump up the contrast on the sidebar icons slightly?".to_string(),
                ),
                timestamp: now - Duration::hours(9) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-diana-2".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "Good idea. I'll also add a subtle gradient on the server sidebar. The flat single-color bg feels a bit stark in dark mode.".to_string(),
                ),
                timestamp: now - Duration::hours(9) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "✨".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-diana-3".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Yeah, a subtle 5-10% gradient would help. Keep it as a CSS custom property in the theme file so users can override it.".to_string(),
                ),
                timestamp: now - Duration::hours(8),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],

        // Eve — meeting reminder
        "dm-user-eve" => vec![
            Message {
                id: "dm-eve-0".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text(
                    "Just a reminder — we have the voice/video architecture call tomorrow at 2pm UTC 📅".to_string(),
                ),
                timestamp: now - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-eve-1".to_string(),
                author: demo_session().user,
                content: MessageContent::Text("Thanks! I've got it blocked. Agenda finalized?".to_string()),
                timestamp: now - Duration::hours(5) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-eve-2".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text(
                    "Yes — three items:\n1. WebRTC crate selection (webrtc-rs vs browser native)\n2. Mobile camera/mic bindings\n3. Voice UI component layout\n\nShould take ~45 min.".to_string(),
                ),
                timestamp: now - Duration::hours(5) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👌".to_string(), count: 1, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-eve-3".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Perfect. I'll have some notes on the `webrtc` crate ready — been reading through its ICE/DTLS setup.".to_string(),
                ),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-eve-4".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text("🙌 Perfect. See you then!".to_string()),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],

        // Cat ↔ Dog playful banter (unhinged cute chaos) 
        "dm-demo-dog" => vec![
            Message {
                id: "dm-cat-dog-0".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Meow? I'm the BEST programmer here 🐱✨\n\nYour Rust code is pedestrian. Mine is *chef's kiss* 😼"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-cat-dog-1".to_string(),
                author: User {
                    id: "demo-dog-user".to_string(),
                    display_name: "🐶 Dog (demo)".to_string(),
                    avatar_url: Some(DEMO_DOG_AVATAR.to_string()),
                    presence: PresenceStatus::Online,
                    backend: BackendType::Demo,
                },
                content: MessageContent::Text(
                    "bark bark!! 🐕 LOL pedestrian?? My PRs actually PASS CI cat!! unlike SOME libraries we know 😏"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(5) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-cat-dog-2".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "oh PLEASE 😹 remember that time your Tokio buffer overflowed? i was THERE, i saw it with my OWN EYES"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(5) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-cat-dog-3".to_string(),
                author: User {
                    id: "demo-dog-user".to_string(),
                    display_name: "🐶 Dog (demo)".to_string(),
                    avatar_url: Some(DEMO_DOG_AVATAR.to_string()),
                    presence: PresenceStatus::Online,
                    backend: BackendType::Demo,
                },
                content: MessageContent::Text(
                    "that was ONE TIME and it was CLEARLY a testing artifact!! unlike your hot-reload that breaks EVERY TUESDAY 🙄"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(5),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-cat-dog-4".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "ok but seriously, your DioxusEVENTs are kinda sus. have you benchmarked them? 👀"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-cat-dog-5".to_string(),
                author: User {
                    id: "demo-dog-user".to_string(),
                    display_name: "🐶 Dog (demo)".to_string(),
                    avatar_url: Some(DEMO_DOG_AVATAR.to_string()),
                    presence: PresenceStatus::Online,
                    backend: BackendType::Demo,
                },
                content: MessageContent::Text(
                    "not better than YOUR message queueing!! at least mine have RTFM docs 📚 you just have vibes and prayer 😼💀"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(3) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "💀".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-cat-dog-6".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "fair! 😹 but you have to admit the feature flag organization is *clean* even if it's stolen from my 2023 design"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-cat-dog-7".to_string(),
                author: User {
                    id: "demo-dog-user".to_string(),
                    display_name: "🐶 Dog (demo)".to_string(),
                    avatar_url: Some(DEMO_DOG_AVATAR.to_string()),
                    presence: PresenceStatus::Online,
                    backend: BackendType::Demo,
                },
                content: MessageContent::Text(
                    "bark bark! 🐕 haha, your pull request is almost as good as my naps 😼"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(1),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],

        // Cat ↔ Dog from dog's perspective (demo2)
        "dm-demo-cat" => vec![
            Message {
                id: "dm-dog-cat-0".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Meow? I'm the BEST programmer here 🐱✨\n\nYour Rust code is pedestrian. Mine is *chef's kiss* 😼"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-dog-cat-1".to_string(),
                author: demo2_session().user,
                content: MessageContent::Text(
                    "bark bark!! 🐕 LOL pedestrian?? My PRs actually PASS CI cat!! unlike SOME libraries we know 😏"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(5) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-dog-cat-2".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "oh PLEASE 😹 remember that time your Tokio buffer overflowed? i was THERE, i saw it with my OWN EYES"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(5) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-dog-cat-3".to_string(),
                author: demo2_session().user,
                content: MessageContent::Text(
                    "that was ONE TIME and it was CLEARLY a testing artifact!! unlike your hot-reload that breaks EVERY TUESDAY 🙄"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(5),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-dog-cat-4".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "ok but seriously, your DioxusEVENTs are kinda sus. have you benchmarked them? 👀"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-dog-cat-5".to_string(),
                author: demo2_session().user,
                content: MessageContent::Text(
                    "not better than YOUR message queueing!! at least mine have RTFM docs 📚 you just have vibes and prayer 😼💀"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(3) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "💀".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-dog-cat-6".to_string(),
                author: demo_session().user,
                content: MessageContent::Text(
                    "fair! 😹 but you have to admit the feature flag organization is *clean* even if it's stolen from my 2023 design"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "dm-dog-cat-7".to_string(),
                author: demo2_session().user,
                content: MessageContent::Text(
                    "bark bark! 🐕 haha, your pull request is almost as good as my naps 😼"
                        .to_string(),
                ),
                timestamp: now - Duration::hours(1),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],

        // Fallback for any other dm- ID
        _ => vec![
            Message {
                id: format!("{dm_channel_id}-msg-0"),
                author: demo_session().user,
                content: MessageContent::Text("Hey there! How are you doing?".to_string()),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],
    }
}

// ── Group Data ────────────────────────────────────────────────────────────────

/// Generate themed group DMs for the cat (demo) account.
///
/// Returns 4 groups, each aligned to a community from the cat account's servers.
pub fn demo_groups_v2() -> Vec<Group> {
    let users = demo_users();
    let now = Utc::now();

    vec![
        Group {
            id: "group-rust-study".to_string(),
            name: Some("Rust Study Group".to_string()),
            members: vec![users[0].clone(), users[1].clone(), users[2].clone()],
            last_message: Some(Message {
                id: "group-rust-study-last".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Found a great blog post on async Rust patterns".to_string(),
                ),
                timestamp: now - Duration::hours(1),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO_ACCOUNT_ID.to_string(),
        },
        Group {
            id: "group-weekend-warriors".to_string(),
            name: Some("Weekend Warriors".to_string()),
            members: vec![users[3].clone(), users[4].clone(), users[5].clone()],
            last_message: Some(Message {
                id: "group-weekend-warriors-last".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text("Saturday 8pm — everyone good?".to_string()),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO_ACCOUNT_ID.to_string(),
        },
        Group {
            id: "group-midnight-jams".to_string(),
            name: Some("Midnight Jams".to_string()),
            members: vec![users[6].clone(), users[7].clone()],
            last_message: Some(Message {
                id: "group-midnight-jams-last".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text("New lo-fi playlist just dropped 🎵".to_string()),
                timestamp: now - Duration::hours(5),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO_ACCOUNT_ID.to_string(),
        },
        Group {
            id: "group-team-poly".to_string(),
            name: Some("Poly Core Team".to_string()),
            members: vec![
                users[0].clone(),
                users[1].clone(),
                users[2].clone(),
                users[3].clone(),
            ],
            last_message: Some(Message {
                id: "group-team-poly-last".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text("Sprint planning tomorrow, 10am UTC".to_string()),
                timestamp: now - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO_ACCOUNT_ID.to_string(),
        },
    ]
}

/// Generate themed group DMs for the dog (demo2) account.
pub fn demo2_groups() -> Vec<Group> {
    let users = demo_users();
    let now = Utc::now();

    vec![
        Group {
            id: "group2-oss-contributors".to_string(),
            name: Some("OSS Contributors".to_string()),
            members: vec![users[0].clone(), users[1].clone(), users[2].clone()],
            last_message: Some(Message {
                id: "group2-oss-last".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text("PR #342 needs a second review 👀".to_string()),
                timestamp: now - Duration::hours(2),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
        },
        Group {
            id: "group2-bookworms".to_string(),
            name: Some("Bookworms".to_string()),
            members: vec![users[3].clone(), users[4].clone()],
            last_message: Some(Message {
                id: "group2-bookworms-last".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "Finished it last night — what a twist! 📚".to_string(),
                ),
                timestamp: now - Duration::hours(7),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
        },
        Group {
            id: "group2-meal-prep".to_string(),
            name: Some("Meal Prep Squad".to_string()),
            members: vec![users[5].clone(), users[6].clone(), users[7].clone()],
            last_message: Some(Message {
                id: "group2-meal-prep-last".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text("Batch cooked 6 meals today 🍱".to_string()),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
            }),
            backend: BackendType::Demo,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
        },
    ]
}

// ── Group Messages ────────────────────────────────────────────────────────────

/// Generate conversation messages for a demo group DM.
///
/// Each group gets a personalised thread matching its theme.
pub fn demo_group_messages(group_id: &str) -> Vec<Message> {
    let users = demo_users();
    let now = Utc::now();

    match group_id {
        "group-rust-study" => vec![
            Message {
                id: "grp-rust-0".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Hey team! I started experimenting with `async-trait` 0.2 — the new built-in async trait syntax in Rust 1.85 is a game changer.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(4),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🦀".to_string(), count: 3, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-rust-1".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "Yeah, no more `#[async_trait]` proc macro! The desugaring is so much cleaner now.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(3) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-rust-2".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Found a great blog post on async Rust patterns — heavy read but worth it:\nhttps://blog.therust.cafe/async-patterns-2025".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(2),
                attachments: vec![],
                reactions: vec![
                    Reaction { emoji: "📖".to_string(), count: 2, me: false },
                    Reaction { emoji: "🔖".to_string(), count: 1, me: true },
                ],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-rust-3".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Also — should we start looking into `polonius` borrow checker? It handles some lifetime edge cases the current NLL misses.".to_string(),
                ),
                timestamp: now - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-rust-4".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Found a great blog post on async Rust patterns".to_string(),
                ),
                timestamp: now - Duration::hours(1),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],

        "group-weekend-warriors" => vec![
            Message {
                id: "grp-ww-0".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "Who's up for a game night this Saturday? 🎮".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(5),
                attachments: vec![],
                reactions: vec![
                    Reaction { emoji: "🙋".to_string(), count: 2, me: true },
                ],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-ww-1".to_string(),
                author: users[5].clone(),
                content: MessageContent::Text("I'm in! Valorant or Minecraft?".to_string()),
                timestamp: now - Duration::days(2) - Duration::hours(4) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-ww-2".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text(
                    "Minecraft survival — let's finish that castle we started. 🏰".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(4) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🙌".to_string(), count: 3, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-ww-3".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text("Minecraft it is! 8pm EST?".to_string()),
                timestamp: now - Duration::days(2) - Duration::hours(3),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👍".to_string(), count: 2, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-ww-4".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text("Saturday 8pm — everyone good?".to_string()),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],

        "group-midnight-jams" => vec![
            Message {
                id: "grp-mj-0".to_string(),
                author: users[7].clone(),
                content: MessageContent::Text(
                    "Anyone else been listening to that new synthwave album? The \"Chromatic Bloom\" one?".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-mj-1".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "Yes!! Track 4 is pure nostalgia 🎧 Can't stop listening.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(2) - Duration::minutes(55),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🎶".to_string(), count: 2, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-mj-2".to_string(),
                author: users[7].clone(),
                content: MessageContent::Text(
                    "Made a lo-fi coding playlist that works really well for late night sessions. Want me to share it?".to_string(),
                ),
                timestamp: now - Duration::hours(8),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-mj-3".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text("New lo-fi playlist just dropped 🎵".to_string()),
                timestamp: now - Duration::hours(5),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🎵".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
        ],

        "group-team-poly" => vec![
            Message {
                id: "grp-tp-0".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Phase 2.12 shipped! Status dots and diagnostics are live. 🎉".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(8),
                attachments: vec![],
                reactions: vec![
                    Reaction { emoji: "🎉".to_string(), count: 4, me: true },
                    Reaction { emoji: "🚀".to_string(), count: 2, me: false },
                ],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-tp-1".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "Great work! The favorites persistence was a real pain point. Glad it's sorted.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(7) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-tp-2".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "2.13 scope is clear — DMs/Groups + rich demo data. Anything we should prioritise?".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-tp-3".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Group member management would be a big UX win. Even just the list panel with a remove button would go a long way.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👍".to_string(), count: 3, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp-tp-4".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Sprint planning tomorrow, 10am UTC".to_string(),
                ),
                timestamp: now - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],

        // Dog account groups
        "group2-oss-contributors" => vec![
            Message {
                id: "grp2-oss-0".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Just pushed new CI pipeline for the Open Source Hub project. Builds are 40% faster now 🚀".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🔥".to_string(), count: 3, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp2-oss-1".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Nice! I've been stuck on that weird flaky test in the integration suite. Any ideas?".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(4) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp2-oss-2".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "PR #342 needs a second review 👀\nIt's the async queue refactor — should be clean but needs eyes.".to_string(),
                ),
                timestamp: now - Duration::hours(2),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👀".to_string(), count: 1, me: true }],
                reply_to: None,
        edited: false,
            },
        ],

        "group2-bookworms" => vec![
            Message {
                id: "grp2-bw-0".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text(
                    "Okay I'm halfway through the book and I can already tell the ending is going to be devastating 😭".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(6),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "😭".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp2-bw-1".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "Oh no. I just finished it last night — what a twist! 📚\n\n(No spoilers but... chapter 23. That's all I'll say.)".to_string(),
                ),
                timestamp: now - Duration::hours(7),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👀".to_string(), count: 1, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp2-bw-2".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text(
                    "Chapter 23?! I'm only on 18 aaaaah 😱".to_string(),
                ),
                timestamp: now - Duration::hours(6) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "😱".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
        ],

        "group2-meal-prep" => vec![
            Message {
                id: "grp2-mp-0".to_string(),
                author: users[5].clone(),
                content: MessageContent::Text(
                    "Sunday meal prep done! Chicken tikka, roasted veg, overnight oats 🍱".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(12),
                attachments: vec![
                    Attachment::remote(
                        "att-meal-prep".to_string(),
                        "meal-prep-sunday.jpg".to_string(),
                        "image/jpeg".to_string(),
                        "https://picsum.photos/seed/mealprep/400/300".to_string(),
                        320_000,
                    ),
                ],
                reactions: vec![
                    Reaction { emoji: "😍".to_string(), count: 2, me: true },
                    Reaction { emoji: "🍱".to_string(), count: 1, me: false },
                ],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp2-mp-1".to_string(),
                author: users[7].clone(),
                content: MessageContent::Text(
                    "Looks incredible! Can you share the tikka recipe? I've been trying to perfect it.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(11) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "grp2-mp-2".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "Batch cooked 6 meals today 🍱\nHigh protein week — grilled salmon, lentil soup, quinoa salad.".to_string(),
                ),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "💪".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
        ],

        // Fallback for any other group ID
        _ => vec![
            Message {
                id: format!("{group_id}-msg-0"),
                author: demo_session().user,
                content: MessageContent::Text(
                    "Hey everyone! Glad we have this group. 👋".to_string(),
                ),
                timestamp: now - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],
    }
}

// ── Enriched demo2 channel messages ──────────────────────────────────────────

/// Additional rich messages for dog account channels not covered by `demo2_messages`.
///
/// These supplement/override the sparse data in `demo2_messages` for channels that
/// previously returned only minimal content.
fn demo2_general_attachment(index: usize) -> Attachment {
    Attachment::remote(
        format!("att-general-{index}"),
        format!("screenshot-{index}.jpg"),
        "image/jpeg".to_string(),
        format!("https://picsum.photos/seed/poly-general-{index}/640/420"),
        320_000 + (index as u64 * 37),
    )
}

fn demo2_opensource_general_messages() -> Vec<Message> {
    let users = demo_users();
    let authors = [
        users[0].clone(),
        users[1].clone(),
        users[2].clone(),
        users[3].clone(),
        users[4].clone(),
        users[6].clone(),
        users[7].clone(),
        users[9].clone(),
    ];
    let topics = [
        "Dioxus hot-reload",
        "release automation",
        "CI flakes",
        "accessibility review",
        "message pagination",
        "plugin sandboxing",
        "design polish",
        "onboarding docs",
    ];
    let actions = [
        "needs another pass before merge",
        "looks solid after the last review",
        "probably wants a follow-up issue",
        "is blocked on one missing screenshot",
        "got much simpler after refactoring",
        "should land behind a flag first",
        "feels ready for beta testing",
        "still needs cross-platform verification",
    ];
    let details = [
        "I tested it on Linux and the behavior is finally stable.",
        "The latest build shaved a surprising amount of time off the workflow.",
        "We should document the sharp edges before more people try it.",
        "I love how much easier it is to reason about after the cleanup.",
        "This would make a great dogfooding target for the desktop build.",
        "We should mirror the behavior more closely to Discord here.",
        "The current branch already feels much more production-like.",
        "Let's save a screenshot once this is visually locked in.",
    ];

    (0..560)
        .map(|index| {
            let topic = topics[index % topics.len()];
            let action = actions[(index / 3) % actions.len()];
            let detail = details[(index / 7) % details.len()];
            let author = authors[index % authors.len()].clone();
            let timestamp = Utc::now() - Duration::minutes(24_000)
                + Duration::minutes(i64::try_from(index).unwrap_or(0) * 43);
            let mut text = format!("Daily check-in #{index}: {topic} {action}. {detail}",);

            if index % 17 == 0 {
                text.push_str(&format!(
                    "\nhttps://github.com/polyglot-messenger/poly/issues/{}",
                    400 + index
                ));
            }

            if index % 29 == 0 {
                text.push_str(
                    "\nCan someone validate the scroll position after loading older messages?",
                );
            }

            let attachments = if index % 37 == 0 || index % 53 == 0 {
                vec![demo2_general_attachment(index)]
            } else {
                Vec::new()
            };

            let reactions = if index % 19 == 0 {
                vec![Reaction {
                    emoji: "🔥".to_string(),
                    count: 1 + u32::try_from(index % 5).unwrap_or(0),
                    me: index % 2 == 0,
                }]
            } else if index % 23 == 0 {
                vec![Reaction {
                    emoji: "✅".to_string(),
                    count: 1 + u32::try_from(index % 3).unwrap_or(0),
                    me: false,
                }]
            } else {
                Vec::new()
            };

            Message {
                id: format!("msg2-general-{index}"),
                author,
                content: MessageContent::Text(text),
                timestamp,
                attachments,
                reactions,
                reply_to: None,
                edited: index % 41 == 0,
            }
        })
        .collect()
}

pub fn demo2_messages_rich(channel_id: &str) -> Vec<Message> {
    let users = demo_users();
    let now = Utc::now();

    match channel_id {
        "ch2-general" => demo2_opensource_general_messages(),
        "ch2-contributions" => vec![
            Message {
                id: "msg2-contrib-0".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Merged 3 PRs this week! The issue tracker is looking much cleaner.".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(4),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🎉".to_string(), count: 3, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-contrib-1".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Working on the documentation overhaul. The README was last updated in 2023 😅".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(6),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-contrib-2".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "PR #342 is up! Async queue refactor — reduces memory usage by ~30%. Please review when you get a chance 🙏".to_string(),
                ),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👀".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
        ],
        "ch2-recommendations" => vec![
            Message {
                id: "msg2-rec-0".to_string(),
                author: users[4].clone(),
                content: MessageContent::Text(
                    "Just finished \"The Midnight Library\" — absolutely recommend it if you like contemplative fiction. 📖".to_string(),
                ),
                timestamp: now - Duration::days(3) - Duration::hours(5),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "📚".to_string(), count: 4, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-rec-1".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "I've been meaning to read that! Currently on \"Project Hail Mary\" — hard sci-fi that reads like a thriller.".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(3),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🚀".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-rec-2".to_string(),
                author: users[7].clone(),
                content: MessageContent::Text(
                    "For non-fiction people: \"Four Thousand Weeks\" by Oliver Burkeman is life-changing. Totally reframes time and productivity.".to_string(),
                ),
                timestamp: now - Duration::hours(8),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "⏳".to_string(), count: 3, me: true }],
                reply_to: None,
        edited: false,
            },
        ],
        "ch2-recipes" => vec![
            Message {
                id: "msg2-recipes-0".to_string(),
                author: users[5].clone(),
                content: MessageContent::Text(
                    "Made a 5-ingredient pasta last night and it turned out amazing!\n\n**Recipe:**\n- 400g spaghetti\n- 200g guanciale (or pancetta)\n- 4 egg yolks\n- 100g Pecorino Romano\n- Black pepper\n\nThe key is to use the pasta water to emulsify the sauce — no cream!".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(6),
                attachments: vec![
                    Attachment::remote(
                        "att-pasta".to_string(),
                        "carbonara.jpg".to_string(),
                        "image/jpeg".to_string(),
                        "https://picsum.photos/seed/pasta/500/350".to_string(),
                        450_000,
                    ),
                ],
                reactions: vec![
                    Reaction { emoji: "😍".to_string(), count: 5, me: true },
                    Reaction { emoji: "🍝".to_string(), count: 3, me: false },
                ],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-recipes-1".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "Classic carbonara! The guanciale makes all the difference, I can't go back to using bacon anymore.".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(5) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-recipes-2".to_string(),
                author: users[7].clone(),
                content: MessageContent::Text(
                    "Anyone have a good sourdough starter recipe? Mine keeps dying 😭 Third attempt this month...".to_string(),
                ),
                timestamp: now - Duration::hours(5),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "😭".to_string(), count: 1, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-recipes-3".to_string(),
                author: users[5].clone(),
                content: MessageContent::Text(
                    "The trick is room temperature water (never tap cold) and 12h between feedings. Also 50/50 bread flour + whole wheat feed works better than just white flour.".to_string(),
                ),
                timestamp: now - Duration::hours(4) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🙏".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
        ],
        "ch2-techniques" => vec![
            Message {
                id: "msg2-tech-0".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "Anyone else do the salt-baking technique for whole fish? Completely seals in moisture — game changer. 🐟".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(8),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🐟".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-tech-1".to_string(),
                author: users[5].clone(),
                content: MessageContent::Text(
                    "Tried it with sea bass last weekend! The crust comes off perfectly and the fish stays so juicy.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(7) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-tech-2".to_string(),
                author: users[7].clone(),
                content: MessageContent::Text(
                    "The maillard reaction discussion last week was eye-opening. I've been searing my steaks incorrectly my whole life 😅\n\nPatting dry + cast iron screaming hot = perfection".to_string(),
                ),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🥩".to_string(), count: 3, me: true }],
                reply_to: None,
        edited: false,
            },
        ],
        "ch2-nutrition" => vec![
            Message {
                id: "msg2-nutr-0".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "Week 3 of tracking macros — 150g protein/day feels more achievable than I expected. Chicken + Greek yoghurt + cottage cheese gets you there pretty easily.".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(5),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "💪".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-nutr-1".to_string(),
                author: users[8].clone(),
                content: MessageContent::Text(
                    "Protein timing matters too — spreading it across 4+ meals seems to help muscle synthesis more than 2-3 large portions.".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(4) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-nutr-2".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "Post-workout meal today: 200g salmon, roasted sweet potato, broccoli. Clean and filling 🍽️".to_string(),
                ),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🍽️".to_string(), count: 1, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-nutr-3".to_string(),
                author: users[8].clone(),
                content: MessageContent::Text(
                    "Meal logged: 3,200 kcal, 145g protein. Slightly over on carbs but had a hard leg day so it balances out.".to_string(),
                ),
                timestamp: now - Duration::hours(2) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],
        "ch2-workouts" => vec![
            Message {
                id: "msg2-wk-0".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "5K run this morning! 💪 New personal best — 22:14. Beat my previous by 45 seconds.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(7),
                attachments: vec![],
                reactions: vec![
                    Reaction { emoji: "🔥".to_string(), count: 6, me: false },
                    Reaction { emoji: "🏃".to_string(), count: 3, me: true },
                ],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-wk-1".to_string(),
                author: users[8].clone(),
                content: MessageContent::Text("That's incredible progress! What's your training plan?".to_string()),
                timestamp: now - Duration::days(1) - Duration::hours(6) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-wk-2".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "3 runs/week — one tempo, one long slow, one interval. Followed the 12-week Garmin plan. Surprisingly beginner-friendly.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(6) - Duration::minutes(30),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg2-wk-3".to_string(),
                author: users[9].clone(),
                content: MessageContent::Text(
                    "Anyone else doing the 100-day push-up challenge? Day 34 — 200 push-ups today. My arms are cooked 🔥".to_string(),
                ),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![
                    Reaction { emoji: "💪".to_string(), count: 4, me: true },
                    Reaction { emoji: "😤".to_string(), count: 2, me: false },
                ],
                reply_to: None,
        edited: false,
            },
        ],
        "ch-rust" => vec![
            Message {
                id: "msg-rust-0".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "I keep hitting E0502 when trying to use a slice and mutate the Vec at the same time. The borrow checker is not happy 😅".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(5),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-rust-1".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "Classic borrow issue! You need to use `split_at_mut` or restructure to avoid overlapping borrows. Or collect the indices first, then mutate.".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(4) - Duration::minutes(55),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🦀".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-rust-2".to_string(),
                author: users[5].clone(),
                content: MessageContent::Text(
                    "The `polonius` borrow checker (now in nightly) handles some of these NLL limitations. Worth trying on nightly if you want to unblock.".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-rust-3".to_string(),
                author: users[9].clone(),
                content: MessageContent::Text(
                    "How are people handling errors in large projects? `thiserror` for libraries + `anyhow` for binaries seems like the community standard.".to_string(),
                ),
                timestamp: now - Duration::hours(6),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👍".to_string(), count: 3, me: true }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-rust-4".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "That's exactly the pattern we use in Poly. `poly-client` uses `thiserror`, the apps use `anyhow` internally. Works great.".to_string(),
                ),
                timestamp: now - Duration::hours(5) - Duration::minutes(45),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],
        "ch-dioxus" => vec![
            Message {
                id: "msg-dioxus-0".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "Dioxus 0.7 hot-reload is genuinely magical. RSX changes reflect subsecond without losing any state. How is this even possible? 🤯".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(7),
                attachments: vec![],
                reactions: vec![
                    Reaction { emoji: "🤯".to_string(), count: 4, me: true },
                ],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-dioxus-1".to_string(),
                author: users[0].clone(),
                content: MessageContent::Text(
                    "The `subsecond` hotpatch library. It relinks only the changed functions at runtime — similar to how Swift playgrounds work. Genuinely impressive engineering.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(6) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-dioxus-2".to_string(),
                author: users[5].clone(),
                content: MessageContent::Text(
                    "One thing I've learned: keep `#[component]` fns under ~150 lines or the RSX macro gets confused during hot-reload. Works great when you respect the limit.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "✏️".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-dioxus-3".to_string(),
                author: users[2].clone(),
                content: MessageContent::Text(
                    "Signal-based state is so much cleaner than React's useState cascade. Just `use_context()` anywhere and it Just Works™.".to_string(),
                ),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "💯".to_string(), count: 3, me: true }],
                reply_to: None,
        edited: false,
            },
        ],
        "ch-production" => vec![
            Message {
                id: "msg-prod-0".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "Just got my first hardware synth! A Novation MiniNova. The learning curve is steep but the sound design possibilities are endless 🎹".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(6),
                attachments: vec![
                    Attachment::remote(
                        "att-synth".to_string(),
                        "mininova.jpg".to_string(),
                        "image/jpeg".to_string(),
                        "https://picsum.photos/seed/synth/400/280".to_string(),
                        380_000,
                    ),
                ],
                reactions: vec![
                    Reaction { emoji: "🎹".to_string(), count: 4, me: true },
                    Reaction { emoji: "🔥".to_string(), count: 2, me: false },
                ],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-prod-1".to_string(),
                author: users[7].clone(),
                content: MessageContent::Text(
                    "Beautiful! Are you going to run it through your DAW or use it standalone? I hook mine directly into Ableton and use MIDI automation heavily.".to_string(),
                ),
                timestamp: now - Duration::days(2) - Duration::hours(5) - Duration::minutes(50),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-prod-2".to_string(),
                author: users[6].clone(),
                content: MessageContent::Text(
                    "Planning to use it with FL Studio. Made a 32-bar loop last night and it sounds incredible with some saturation on the output.\n\nHere's a quick recording:".to_string(),
                ),
                timestamp: now - Duration::hours(8),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "👀".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-prod-3".to_string(),
                author: users[9].clone(),
                content: MessageContent::Text(
                    "Anyone using the new Vital synth plugin? Free version is genuinely superb — wavetable synthesis with a ton of preset starting points.".to_string(),
                ),
                timestamp: now - Duration::hours(3),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🎵".to_string(), count: 3, me: true }],
                reply_to: None,
        edited: false,
            },
        ],
        "ch-valorant" => vec![
            Message {
                id: "msg-valorant-0".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text("Just hit Diamond! Finally 💎".to_string()),
                timestamp: now - Duration::days(1) - Duration::hours(6),
                attachments: vec![],
                reactions: vec![
                    Reaction { emoji: "💎".to_string(), count: 5, me: true },
                    Reaction { emoji: "🎉".to_string(), count: 3, me: false },
                ],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-valorant-1".to_string(),
                author: users[1].clone(),
                content: MessageContent::Text(
                    "Let's goooo!! What agent did you climb with?".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(55),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-valorant-2".to_string(),
                author: users[3].clone(),
                content: MessageContent::Text(
                    "Mostly Sage and Killjoy on defense. Sentinel playstyle + info gathering wins more rounds than fragging in my experience.".to_string(),
                ),
                timestamp: now - Duration::days(1) - Duration::hours(5) - Duration::minutes(40),
                attachments: vec![],
                reactions: vec![Reaction { emoji: "🧠".to_string(), count: 2, me: false }],
                reply_to: None,
        edited: false,
            },
            Message {
                id: "msg-valorant-3".to_string(),
                author: users[9].clone(),
                content: MessageContent::Text(
                    "Killjoy is so broken right now — her turret damage got buffed and nobody's countering it properly. Expect nerfs soon.".to_string(),
                ),
                timestamp: now - Duration::hours(4),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
        edited: false,
            },
        ],
        _ => vec![],
    }
}

/// Extract plain searchable text from a demo message.
fn demo_message_text(message: &Message) -> String {
    match &message.content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::WithAttachments { text, .. } => text.clone(),
    }
}

/// Get the full message history for a demo account channel.
fn demo_account_messages(channel_id: &str, demo2: bool) -> Vec<Message> {
    if channel_id.starts_with("dm-") {
        return demo_dm_messages(channel_id);
    }
    if channel_id.starts_with("group-") || channel_id.starts_with("group2-") {
        return demo_group_messages(channel_id);
    }

    if demo2 {
        let rich = demo2_messages_rich(channel_id);
        if rich.is_empty() {
            demo2_messages(channel_id)
        } else {
            rich
        }
    } else {
        let rich = demo2_messages_rich(channel_id);
        if rich.is_empty() {
            demo_messages(channel_id)
        } else {
            rich
        }
    }
}

/// Apply a message history query to a full ordered message list.
fn apply_message_query(mut messages: Vec<Message>, query: &MessageQuery) -> Vec<Message> {
    messages.sort_by_key(|message| message.timestamp);

    if let Some(ref around_id) = query.around
        && let Some(index) = messages.iter().position(|message| &message.id == around_id)
    {
        let limit = usize::try_from(query.limit.unwrap_or(48)).unwrap_or(48);
        let before = limit / 2;
        let start = index.saturating_sub(before);
        let end = (start + limit).min(messages.len());
        return messages[start..end].to_vec();
    }

    if let Some(ref before_id) = query.before
        && let Some(index) = messages.iter().position(|message| &message.id == before_id)
    {
        let limit = usize::try_from(query.limit.unwrap_or(50)).unwrap_or(50);
        let start = index.saturating_sub(limit);
        return messages[start..index].to_vec();
    }

    if let Some(ref after_id) = query.after
        && let Some(index) = messages.iter().position(|message| &message.id == after_id)
    {
        let limit = usize::try_from(query.limit.unwrap_or(50)).unwrap_or(50);
        let start = index.saturating_add(1);
        let end = (start + limit).min(messages.len());
        return messages[start..end].to_vec();
    }

    if let Some(limit) = query.limit {
        let limit = usize::try_from(limit).unwrap_or(messages.len());
        if limit < messages.len() {
            return messages[messages.len() - limit..].to_vec();
        }
    }

    messages
}

/// Get queried messages for the cat demo account.
pub fn demo_messages_query(channel_id: &str, query: &MessageQuery) -> Vec<Message> {
    apply_message_query(demo_account_messages(channel_id, false), query)
}

/// Get queried messages for the dog demo account.
pub fn demo2_messages_query(channel_id: &str, query: &MessageQuery) -> Vec<Message> {
    apply_message_query(demo_account_messages(channel_id, true), query)
}

/// Return message IDs pinned in a given channel for the cat demo account.
fn demo_pinned_ids(channel_id: &str) -> &'static [&'static str] {
    match channel_id {
        "ch-general" => &["msg-ch-general-5", "msg-ch-general-8"],
        "ch-rust" => &["msg-rust-1"],
        "ch-dioxus" => &["msg-dioxus-2"],
        "dm-user-diana" => &["dm-diana-2"],
        "group-rust-study" => &["grp-rust-2"],
        _ => &[],
    }
}

/// Return message IDs pinned in a given channel for the dog demo account.
fn demo2_pinned_ids(channel_id: &str) -> &'static [&'static str] {
    match channel_id {
        "ch2-general" => &["msg2-general-144", "msg2-general-418"],
        "ch2-announcements" => &["msg2-1"],
        "ch2-workouts" => &["msg2-wk-0"],
        "ch2-recipes" => &["msg2-recipes-0"],
        "dm-demo-cat" => &["dm-cat-dog-2"],
        "group2-oss-contributors" => &["grp2-oss-2"],
        _ => &[],
    }
}

/// Get pinned messages for the cat demo account.
pub fn demo_pinned_messages(channel_id: &str) -> Vec<Message> {
    let ids = demo_pinned_ids(channel_id);
    demo_account_messages(channel_id, false)
        .into_iter()
        .filter(|message| ids.contains(&message.id.as_str()))
        .collect()
}

/// Get pinned messages for the dog demo account.
pub fn demo2_pinned_messages(channel_id: &str) -> Vec<Message> {
    let ids = demo2_pinned_ids(channel_id);
    demo_account_messages(channel_id, true)
        .into_iter()
        .filter(|message| ids.contains(&message.id.as_str()))
        .collect()
}

/// Search messages for the cat demo account.
pub fn demo_search_messages(query: &MessageSearchQuery) -> Vec<MessageSearchHit> {
    search_demo_messages(query, false)
}

/// Search messages for the dog demo account.
pub fn demo2_search_messages(query: &MessageSearchQuery) -> Vec<MessageSearchHit> {
    search_demo_messages(query, true)
}

/// Search messages for either demo account.
fn search_demo_messages(query: &MessageSearchQuery, demo2: bool) -> Vec<MessageSearchHit> {
    #[derive(Clone)]
    struct SearchChannelMeta {
        id: String,
        name: String,
        server_id: Option<String>,
        is_text_like: bool,
    }

    let server_channels = if demo2 {
        demo2_servers()
            .into_iter()
            .flat_map(|server| demo2_channels(&server.id))
            .map(|channel| SearchChannelMeta {
                id: channel.id,
                name: channel.name,
                server_id: Some(channel.server_id),
                is_text_like: channel.channel_type == ChannelType::Text,
            })
            .collect::<Vec<_>>()
    } else {
        demo_servers()
            .into_iter()
            .flat_map(|server| demo_channels(&server.id))
            .map(|channel| SearchChannelMeta {
                id: channel.id,
                name: channel.name,
                server_id: Some(channel.server_id),
                is_text_like: channel.channel_type == ChannelType::Text,
            })
            .collect::<Vec<_>>()
    };
    let dm_channels = if demo2 {
        let mut channels = demo_dm_channels().into_iter().take(3).collect::<Vec<_>>();
        channels.push(DmChannel {
            id: "dm-demo-cat".to_string(),
            user: User {
                id: "demo-cat-user".to_string(),
                display_name: "🐱 Cat (demo)".to_string(),
                avatar_url: Some(DEMO_CAT_AVATAR.to_string()),
                presence: PresenceStatus::Online,
                backend: BackendType::Demo,
            },
            last_message: None,
            unread_count: 0,
            backend: BackendType::Demo,
            account_id: DEMO2_ACCOUNT_ID.to_string(),
        });
        channels
            .into_iter()
            .map(|channel| SearchChannelMeta {
                id: channel.id,
                name: channel.user.display_name,
                server_id: None,
                is_text_like: true,
            })
            .collect::<Vec<_>>()
    } else {
        demo_dm_channels()
            .into_iter()
            .map(|channel| SearchChannelMeta {
                id: channel.id,
                name: channel.user.display_name,
                server_id: None,
                is_text_like: true,
            })
            .collect::<Vec<_>>()
    };
    let group_channels = if demo2 {
        demo2_groups()
            .into_iter()
            .map(|group| SearchChannelMeta {
                id: group.id,
                name: group.name.unwrap_or_else(|| "Group DM".to_string()),
                server_id: None,
                is_text_like: true,
            })
            .collect::<Vec<_>>()
    } else {
        demo_groups_v2()
            .into_iter()
            .map(|group| SearchChannelMeta {
                id: group.id,
                name: group.name.unwrap_or_else(|| "Group DM".to_string()),
                server_id: None,
                is_text_like: true,
            })
            .collect::<Vec<_>>()
    };

    let channels = server_channels
        .into_iter()
        .chain(dm_channels)
        .chain(group_channels)
        .filter(|channel| {
            if let Some(ref channel_id) = query.channel_id {
                &channel.id == channel_id
            } else if let Some(ref server_id) = query.server_id {
                channel.server_id.as_deref() == Some(server_id.as_str())
            } else {
                false
            }
        })
        .collect::<Vec<_>>();

    let text_lower = query.text.to_lowercase();
    let mut hits = channels
        .into_iter()
        .filter(|channel| channel.is_text_like)
        .flat_map(|channel| {
            let channel_name = channel.name.clone();
            let channel_id = channel.id.clone();
            let server_id = channel.server_id.clone();
            let text_lower = text_lower.clone();
            demo_account_messages(&channel.id, demo2)
                .into_iter()
                .filter(move |message| {
                    let plain_text = demo_message_text(message);
                    let plain_lower = plain_text.to_lowercase();
                    let author_matches = query.author_id.as_ref().is_none_or(|author_id| {
                        let author_lower = author_id.to_lowercase();
                        message.author.id.eq_ignore_ascii_case(author_id)
                            || message
                                .author
                                .display_name
                                .to_lowercase()
                                .contains(&author_lower)
                    });
                    let link_matches = !query.has_link
                        || plain_text.contains("http://")
                        || plain_text.contains("https://")
                        || message
                            .attachments
                            .iter()
                            .any(|attachment| attachment.url.starts_with("http"));
                    let mention_matches = query
                        .mentions_user_id
                        .as_ref()
                        .is_none_or(|mentioned| plain_lower.contains(&mentioned.to_lowercase()));
                    let text_matches = text_lower.is_empty() || plain_lower.contains(&text_lower);
                    author_matches && link_matches && mention_matches && text_matches
                })
                .map(move |message| MessageSearchHit {
                    channel_id: channel_id.clone(),
                    channel_name: Some(channel_name.clone()),
                    server_id: server_id.clone(),
                    message,
                })
        })
        .collect::<Vec<_>>();

    hits.sort_by(|left, right| right.message.timestamp.cmp(&left.message.timestamp));

    let limit = usize::try_from(query.limit.unwrap_or(25)).unwrap_or(25);
    hits.truncate(limit);
    hits
}

/// Demo slash commands available for server channels.
///
/// Returns bot and app commands for server text channels. Returns empty for DMs and group channels.
pub fn demo_channel_commands(channel_id: &str) -> Vec<ChatCommand> {
    // No slash commands in DMs or group channels in the demo
    if channel_id.starts_with("dm-") || channel_id.starts_with("group-") {
        return Vec::new();
    }

    vec![
        ChatCommand {
            name: "play".to_string(),
            description: "Play a song or URL in the voice channel".to_string(),
            provider: "MusicCat".to_string(),
            is_builtin: false,
            usage: Some("<song name or URL>".to_string()),
            scope: CommandScope::Channel,
        },
        ChatCommand {
            name: "skip".to_string(),
            description: "Skip to the next song in the queue".to_string(),
            provider: "MusicCat".to_string(),
            is_builtin: false,
            usage: None,
            scope: CommandScope::Channel,
        },
        ChatCommand {
            name: "queue".to_string(),
            description: "Show the current music queue".to_string(),
            provider: "MusicCat".to_string(),
            is_builtin: false,
            usage: None,
            scope: CommandScope::Channel,
        },
        ChatCommand {
            name: "ban".to_string(),
            description: "Ban a user from the server".to_string(),
            provider: "ModBot".to_string(),
            is_builtin: false,
            usage: Some("<@user> [reason]".to_string()),
            scope: CommandScope::Channel,
        },
        ChatCommand {
            name: "kick".to_string(),
            description: "Kick a user from the server".to_string(),
            provider: "ModBot".to_string(),
            is_builtin: false,
            usage: Some("<@user> [reason]".to_string()),
            scope: CommandScope::Channel,
        },
        ChatCommand {
            name: "timeout".to_string(),
            description: "Temporarily mute a user".to_string(),
            provider: "ModBot".to_string(),
            is_builtin: false,
            usage: Some("<@user> <duration>".to_string()),
            scope: CommandScope::Channel,
        },
        ChatCommand {
            name: "changelog".to_string(),
            description: "Show the bot's recent updates and changes".to_string(),
            provider: "AI Bot".to_string(),
            is_builtin: false,
            usage: None,
            scope: CommandScope::Global,
        },
        ChatCommand {
            name: "gif".to_string(),
            description: "Search and post an animated GIF".to_string(),
            provider: "AI Bot".to_string(),
            is_builtin: false,
            usage: Some("<search terms>".to_string()),
            scope: CommandScope::Global,
        },
    ]
}

/// Demo custom emoji catalog for a channel.
pub fn demo_available_emojis(channel_id: &str) -> Vec<CustomEmoji> {
    let source_name = if channel_id.starts_with("ch-") {
        Some("Poly Dev".to_string())
    } else {
        Some("Shared".to_string())
    };

    vec![
        CustomEmoji {
            id: "emoji-poly-wave".to_string(),
            shortcode: "poly_wave".to_string(),
            image_url: Some(
                "https://dummyimage.com/64x64/7c5cff/ffffff.png&text=%F0%9F%91%8B".to_string(),
            ),
            unicode_fallback: Some("👋".to_string()),
            animated: false,
            server_id: Some("srv-poly-dev".to_string()),
            source_name: source_name.clone(),
        },
        CustomEmoji {
            id: "emoji-rustacean-hype".to_string(),
            shortcode: "rust_hype".to_string(),
            image_url: Some(
                "https://dummyimage.com/64x64/f97316/ffffff.png&text=%F0%9F%A6%80".to_string(),
            ),
            unicode_fallback: Some("🦀".to_string()),
            animated: false,
            server_id: Some("srv-poly-dev".to_string()),
            source_name: Some("Poly Dev".to_string()),
        },
        CustomEmoji {
            id: "emoji-catjam".to_string(),
            shortcode: "catjam".to_string(),
            image_url: Some(
                "https://dummyimage.com/64x64/ec4899/ffffff.gif&text=%F0%9F%90%B1".to_string(),
            ),
            unicode_fallback: Some("🐱".to_string()),
            animated: true,
            server_id: Some("srv-gaming-lounge".to_string()),
            source_name: Some("Gaming Lounge".to_string()),
        },
    ]
}

/// Demo sticker catalog for a channel.
pub fn demo_available_stickers(channel_id: &str) -> Vec<StickerItem> {
    let source_name = if channel_id.starts_with("ch-") {
        Some("Poly Dev".to_string())
    } else {
        Some("Shared".to_string())
    };

    vec![
        StickerItem {
            id: "sticker-hype-cat".to_string(),
            name: "Hype Cat".to_string(),
            image_url: "https://dummyimage.com/320x320/111827/ffffff.png&text=Hype+Cat".to_string(),
            pack_name: Some("Larar Pack".to_string()),
            description: Some("Party sticker for celebrating wins".to_string()),
            server_id: Some("srv-poly-dev".to_string()),
            source_name: source_name.clone(),
            format: "png".to_string(),
        },
        StickerItem {
            id: "sticker-sup-dog".to_string(),
            name: "Sup?".to_string(),
            image_url: "https://dummyimage.com/320x320/1f2937/ffffff.png&text=Sup%3F".to_string(),
            pack_name: Some("Dog Pack".to_string()),
            description: Some("Casual hello sticker".to_string()),
            server_id: Some("srv-gaming-lounge".to_string()),
            source_name: Some("Gaming Lounge".to_string()),
            format: "png".to_string(),
        },
    ]
}
