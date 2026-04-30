//! In-memory state for the mock Teams/Graph API server.

use dashmap::DashMap;
use poly_test_common::{AuthState, EventBus, HeaderInspectBuffer};
use std::sync::Arc;

/// Events delivered to subscribed clients via change notifications.
///
/// Teams uses a subscription model: clients register a webhook/callback URL,
/// and the server POSTs change notifications to it. In our mock, we use an
/// EventBus so polling or WebSocket-based notification endpoints can deliver
/// events to connected clients.
#[derive(Clone, Debug)]
pub enum TeamsEvent {
    /// New message in a channel or chat.
    MessageCreated {
        resource_id: String,
        message: serde_json::Value,
    },
    /// Message updated.
    MessageUpdated {
        resource_id: String,
        message: serde_json::Value,
    },
    /// Message deleted.
    MessageDeleted {
        resource_id: String,
        message_id: String,
    },
    /// User presence changed.
    PresenceChanged {
        user_id: String,
        availability: String,
    },
}

/// All mock Teams state: users, teams, channels, chats, messages, tokens, broadcast bus.
#[derive(Clone)]
pub struct TeamsState {
    pub auth: AuthState,
    pub users: DashMap<String, User>,
    pub teams: DashMap<String, Team>,
    /// Team memberships keyed by `"team_id"`. Each value is a list of
    /// [`TeamMembership`] records; the `id` field is the membership ID used
    /// by the kick (DELETE /teams/{t}/members/{membership_id}) endpoint.
    pub memberships: DashMap<String, Vec<TeamMembership>>,
    pub channels: DashMap<String, Channel>,
    pub chats: DashMap<String, Chat>,
    pub messages: DashMap<String, Vec<Message>>,
    /// Event bus for real-time delivery via change notifications.
    pub events: EventBus<TeamsEvent>,
    /// Ring buffer of recent inbound request headers (Phase E inspection endpoint).
    pub inspect: Arc<HeaderInspectBuffer>,
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

/// A team membership record — mirrors Graph's `aadUserConversationMember`.
///
/// `id` is the membership ID (used as the path segment for DELETE /members/{id}).
/// `user_id` is the AAD object ID of the member.
/// `roles` is empty for regular members, `["owner"]` for owners.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TeamMembership {
    pub id: String,
    pub user_id: String,
    pub display_name: String,
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    pub id: String,
    pub display_name: String,
    pub team_id: String,
    #[serde(default)]
    pub description: Option<String>,
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
    #[serde(default)]
    pub last_modified_date_time: Option<String>,
    #[serde(default)]
    pub deleted_date_time: Option<String>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Reaction {
    pub user_id: String,
    pub reaction_type: String,
    pub created_date_time: String,
}

impl Default for TeamsState {
    fn default() -> Self {
        Self::new()
    }
}

impl TeamsState {
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            teams: DashMap::new(),
            memberships: DashMap::new(),
            channels: DashMap::new(),
            chats: DashMap::new(),
            messages: DashMap::new(),
            events: EventBus::new(),
            inspect: Arc::new(HeaderInspectBuffer::new()),
        }
    }

    /// Seed demo data: Sheep + Walrus, 2 teams, channels, chats, messages.
    /// Idempotent — skips if data already present.
    pub fn seed(&self) {
        if !self.users.is_empty() {
            return;
        }
        tracing::info!("seeding Teams demo data");

        // Avatar hashes map to GET /v1.0/users/{id}/photo/$value (Graph profile photo path).
        self.users.insert("U001".into(), User {
            id: "U001".into(),
            display_name: "Sheep".into(),
            email: "sheep@contoso.com".into(),
            avatar_url: Some("sheep".into()),
            password: "testpass123".into(),
        });
        self.users.insert("U002".into(), User {
            id: "U002".into(),
            display_name: "Walrus".into(),
            email: "walrus@contoso.com".into(),
            avatar_url: Some("walrus".into()),
            password: "testpass123".into(),
        });

        self.channels.insert("CH001".into(), Channel {
            id: "CH001".into(),
            display_name: "General".into(),
            team_id: "T001".into(),
            description: None,
        });
        self.channels.insert("CH002".into(), Channel {
            id: "CH002".into(),
            display_name: "Engineering".into(),
            team_id: "T001".into(),
            description: None,
        });
        self.channels.insert("CH003".into(), Channel {
            id: "CH003".into(),
            display_name: "General".into(),
            team_id: "T002".into(),
            description: None,
        });
        // Test Arena channel — dedicated clean space for back-and-forth tests
        self.channels.insert("CH_ARENA".into(), Channel {
            id: "CH_ARENA".into(),
            display_name: "test-arena".into(),
            team_id: "T001".into(),
            description: Some("Dedicated back-and-forth test channel".into()),
        });
        self.messages.insert("CH_ARENA".into(), vec![]);

        self.teams.insert("T001".into(), Team {
            id: "T001".into(),
            display_name: "Contoso Corp".into(),
            description: Some("Main company team".into()),
            members: vec!["U001".into(), "U002".into()],
        });
        self.teams.insert("T002".into(), Team {
            id: "T002".into(),
            display_name: "Project Alpha".into(),
            description: Some("Project Alpha team".into()),
            members: vec!["U001".into()],
        });

        // Membership records for T001 — U001 is owner, U002 is member.
        self.memberships.insert("T001".into(), vec![
            TeamMembership {
                id: "MEMB001".into(),
                user_id: "U001".into(),
                display_name: "Sheep".into(),
                roles: vec!["owner".into()],
            },
            TeamMembership {
                id: "MEMB002".into(),
                user_id: "U002".into(),
                display_name: "Walrus".into(),
                roles: vec![],
            },
        ]);
        self.memberships.insert("T002".into(), vec![
            TeamMembership {
                id: "MEMB003".into(),
                user_id: "U001".into(),
                display_name: "Sheep".into(),
                roles: vec!["owner".into()],
            },
        ]);

        self.chats.insert("CHAT001".into(), Chat {
            id: "CHAT001".into(),
            chat_type: "oneOnOne".into(),
            members: vec!["U001".into(), "U002".into()],
        });

        self.messages.insert("CH001".into(), vec![
            Message {
                id: "MSG001".into(),
                body_content: "Good morning team!".into(),
                from_user_id: "U001".into(),
                channel_or_chat_id: "CH001".into(),
                created_date_time: "2026-04-05T09:00:00Z".into(),
                last_modified_date_time: None,
                deleted_date_time: None,
                reactions: vec![],
            },
            Message {
                id: "MSG002".into(),
                body_content: "Morning! Ready for the standup?".into(),
                from_user_id: "U002".into(),
                channel_or_chat_id: "CH001".into(),
                created_date_time: "2026-04-05T09:01:00Z".into(),
                last_modified_date_time: None,
                deleted_date_time: None,
                reactions: vec![],
            },
        ]);
    }

    /// Wipe all data to empty state.
    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.teams.clear();
        self.memberships.clear();
        self.channels.clear();
        self.chats.clear();
        self.messages.clear();
        self.inspect.clear();
        tracing::info!("reset Teams state to empty");
    }

    /// Wipe all data and re-seed. Most common operation between test runs.
    pub fn reseed(&self) {
        self.reset();
        self.seed();
    }
}
