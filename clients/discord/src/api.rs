//! Discord API v10 response types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordGuild {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordChannel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: u8,
    pub guild_id: Option<String>,
    pub parent_id: Option<String>,
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordMessage {
    pub id: String,
    pub content: String,
    pub author: DiscordUser,
    pub channel_id: String,
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
    pub referenced_message: Option<Box<DiscordMessage>>,
}
