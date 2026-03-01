//! Demo data generators for testing the Poly UI.
//!
//! SAFETY NOTE: indexing_slicing is allowed in this module because all indices
//! are bounded by the fixed-size `demo_users()` slice, which is compile-time
//! constant mock data. This is intentional for readability in test/demo code.
#![allow(clippy::indexing_slicing)]

use chrono::{Duration, Utc};
use poly_client::*;
use rand::distr::{Alphanumeric, SampleString};

/// Generate a demo session for the authenticated user.
pub fn demo_session() -> Session {
    Session {
        id: "demo-session-1".to_string(),
        user: User {
            id: "demo-user-self".to_string(),
            display_name: "Demo User".to_string(),
            avatar_url: None,
            presence: PresenceStatus::Online,
            backend: BackendType::Demo,
        },
        token: "demo-token-not-real".to_string(),
        backend: BackendType::Demo,
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
pub fn demo_messages(channel_id: &str) -> Vec<Message> {
    let users = demo_users();
    let now = Utc::now();

    let texts: &[&str] = match channel_id {
        "ch-general" => &[
            "Hey everyone! Welcome to the Poly Development server 👋",
            "Thanks for having me! This project looks really cool.",
            "Has anyone tried the new Dioxus 0.7 hot-reload? It's blazing fast!",
            "Yeah, subsecond hot-patch is a game changer for development.",
            "I just pushed some updates to the theme engine. Check it out!",
            "The SurrealDB integration is coming along nicely.",
            "Anyone up for a code review session later today?",
            "Sure! I'll be free around 3pm.",
            "Does anyone know if SurrealKV works on Android yet?",
            "We should test that early. It's flagged as a risk in the plan.",
        ],
        "ch-minecraft" => &[
            "Who wants to play Minecraft tonight?",
            "I'm in! What time?",
            "Let's do 8pm EST",
            "I built a massive redstone contraption, you all need to see it",
            "The new update is amazing, have you tried the new biomes?",
        ],
        _ => &[
            "Hello from this channel!",
            "Nice to see some activity here.",
            "Let's keep the conversation going!",
        ],
    };

    texts
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let user = &users[i % users.len()];
            Message {
                id: format!("msg-{channel_id}-{i}"),
                author: user.clone(),
                content: MessageContent::Text(text.to_string()),
                timestamp: now - Duration::minutes((texts.len() - i) as i64 * 5),
                attachments: vec![],
                reactions: if i == 0 {
                    vec![Reaction {
                        emoji: "👋".to_string(),
                        count: 3,
                        me: false,
                    }]
                } else {
                    vec![]
                },
                edited: false,
            }
        })
        .collect()
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
            timestamp: now - Duration::hours(5),
            read: true,
            preview: "You've been invited to Rust Community".to_string(),
        },
    ]
}
