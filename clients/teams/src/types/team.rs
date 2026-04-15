//! `/v1.0/me/joinedTeams`, `/v1.0/teams/{id}` — [Graph Team resource].
//!
//! A Team is backed by a Microsoft 365 group; we only model the fields the
//! Poly Server mapping needs.
//!
//! [Graph Team resource]: https://learn.microsoft.com/en-us/graph/api/resources/team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphTeam {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
}
