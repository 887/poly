//! Discord API v10 wire types.
//!
//! These are deserialize-friendly wrappers over the subset of fields we read
//! from Discord / Spacebar responses. Typed fields (`Id<>`, `ChannelType`)
//! come from `twilight-model` (ISC) — no AGPL code from Spacebar/Fosscord.
//!
//! We don't use `twilight_model::user::User`, `::guild::Guild`, etc. directly
//! because those require ~30–45 fields per struct (the full Discord
//! representation), which is impractical for Spacebar compatibility and for
//! our mock server. Our wrapper uses `#[serde(default)]` on optional fields
//! so Spacebar can omit them freely.

use serde::{Deserialize, Serialize};
use twilight_model::channel::ChannelType;
use twilight_model::id::marker::{ChannelMarker, GuildMarker, MessageMarker, UserMarker};
use twilight_model::id::Id;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordUser {
    pub id: Id<UserMarker>,
    pub username: String,
    #[serde(default)]
    pub discriminator: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub global_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordGuild {
    pub id: Id<GuildMarker>,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordChannel {
    pub id: Id<ChannelMarker>,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: ChannelType,
    #[serde(default)]
    pub guild_id: Option<Id<GuildMarker>>,
    #[serde(default)]
    pub parent_id: Option<Id<ChannelMarker>>,
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordMessage {
    pub id: Id<MessageMarker>,
    pub content: String,
    pub author: DiscordUser,
    pub channel_id: Id<ChannelMarker>,
    pub timestamp: String,
    #[serde(default)]
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub referenced_message: Option<Box<DiscordMessage>>,
}
