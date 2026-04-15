//! Channel and chat message payloads — [Graph chatMessage resource].
//!
//! Channel messages (`/teams/{tid}/channels/{cid}/messages`) and chat
//! messages (`/chats/{id}/messages`) share the same schema.
//!
//! [Graph chatMessage resource]: https://learn.microsoft.com/en-us/graph/api/resources/chatmessage

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphMessage {
    pub id: String,
    pub body: GraphMessageBody,
    pub from: Option<GraphMessageFrom>,
    pub created_date_time: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphMessageBody {
    pub content: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphMessageFrom {
    pub user: Option<GraphFromUser>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphFromUser {
    pub id: String,
    pub display_name: Option<String>,
}
