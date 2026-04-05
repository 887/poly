//! Microsoft Graph API response types for Teams.

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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphTeam {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphChannel {
    pub id: String,
    pub display_name: String,
    pub membership_type: Option<String>,
}

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphChat {
    pub id: String,
    #[serde(rename = "chatType")]
    pub chat_type: String,
}

/// Graph API collection response `{ "value": [...] }`
#[derive(Debug, Clone, Deserialize)]
pub struct GraphCollection<T> {
    pub value: Vec<T>,
}
