//! HN-specific deserialization types.

use serde::{Deserialize, Serialize};

/// Type of a HN item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HnItemType {
    #[default]
    Story,
    Comment,
    Job,
    Poll,
    PollOpt,
}

/// A single HN item (story, comment, job, poll, or pollopt).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnItem {
    pub id: u64,
    #[serde(rename = "type", default)]
    pub item_type: HnItemType,
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub time: Option<u64>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub score: Option<u32>,
    #[serde(default)]
    pub descendants: Option<u32>,
    #[serde(default)]
    pub kids: Option<Vec<u64>>,
    #[serde(default)]
    pub parent: Option<u64>,
    #[serde(default)]
    pub dead: Option<bool>,
    #[serde(default)]
    pub deleted: Option<bool>,
}

/// A HN user profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnUser {
    pub id: String,
    pub created: u64,
    pub karma: u64,
    #[serde(default)]
    pub about: Option<String>,
    #[serde(default)]
    pub submitted: Option<Vec<u64>>,
}

/// A HN feed type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HnFeed {
    Top,
    New,
    Best,
    Ask,
    Show,
    Jobs,
}

impl HnFeed {
    /// Returns the API path segment for this feed.
    pub fn path(self) -> &'static str {
        match self {
            Self::Top => "topstories.json",
            Self::New => "newstories.json",
            Self::Best => "beststories.json",
            Self::Ask => "askstories.json",
            Self::Show => "showstories.json",
            Self::Jobs => "jobstories.json",
        }
    }

    /// Returns the channel ID for this feed.
    pub fn channel_id(self) -> &'static str {
        match self {
            Self::Top => "hn-top",
            Self::New => "hn-new",
            Self::Best => "hn-best",
            Self::Ask => "hn-ask",
            Self::Show => "hn-show",
            Self::Jobs => "hn-jobs-ch",
        }
    }

    /// Returns the channel display name for this feed.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::New => "New",
            Self::Best => "Best",
            Self::Ask => "Ask HN",
            Self::Show => "Show HN",
            Self::Jobs => "Jobs",
        }
    }

    /// Parse a channel ID to a HnFeed, if known.
    pub fn from_channel_id(id: &str) -> Option<Self> {
        match id {
            "hn-top" => Some(Self::Top),
            "hn-new" => Some(Self::New),
            "hn-best" => Some(Self::Best),
            "hn-ask" => Some(Self::Ask),
            "hn-show" => Some(Self::Show),
            "hn-jobs-ch" => Some(Self::Jobs),
            _ => None,
        }
    }
}

/// Response from `/v0/updates.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnUpdates {
    #[serde(default)]
    pub items: Vec<u64>,
    #[serde(default)]
    pub profiles: Vec<String>,
}
