//! `/v1.0/me`, `/v1.0/users/{id}` — [Graph User resource].
//!
//! [Graph User resource]: https://learn.microsoft.com/en-us/graph/api/resources/user

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphUser {
    pub id: String,
    pub display_name: String,
    pub mail: Option<String>,
    #[serde(rename = "userPrincipalName")]
    pub user_principal_name: Option<String>,
}
