//! `/v1.0/me/chats`, `/v1.0/chats/{id}` — [Graph Chat resource].
//!
//! `chatType` is one of `oneOnOne`, `group`, `meeting`. 1:1 chats map to
//! Poly DMs; groups map to Poly Group DMs with the Teams icon as source.
//!
//! [Graph Chat resource]: https://learn.microsoft.com/en-us/graph/api/resources/chat

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphChat {
    pub id: String,
    #[serde(rename = "chatType")]
    pub chat_type: String,
    #[serde(rename = "topic", default)]
    pub topic: Option<String>,
    #[serde(rename = "members", default)]
    pub members: Vec<GraphChatMember>,
}

/// Member entry inside a chat payload (when expanded via `$expand=members`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphChatMember {
    pub id: String,
    pub display_name: Option<String>,
    #[serde(rename = "userId", default)]
    pub user_id: Option<String>,
}
