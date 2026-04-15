//! `/v1.0/teams/{id}/channels` — [Graph Channel resource].
//!
//! `membershipType` is one of `standard`, `private`, `shared`. We don't
//! branch on it yet; captured for future ACL-aware rendering.
//!
//! [Graph Channel resource]: https://learn.microsoft.com/en-us/graph/api/resources/channel

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphChannel {
    pub id: String,
    pub display_name: String,
    pub membership_type: Option<String>,
}
