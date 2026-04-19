//! Per-type deserialization tests against captured Graph API JSON samples.
//!
//! Each test loads a fixture file from `tests/fixtures/`, deserializes it
//! into the appropriate `poly_teams` type, and asserts canary fields.
//! No network access — all fixtures are static JSON files.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use poly_teams::{
    auth::TokenResponse,
    types::{GraphChannel, GraphChat, GraphError, GraphMessage, GraphTeam, GraphUser, ODataResponse},
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {name}: {e}"))
}

// ---------------------------------------------------------------------------
// GraphUser — /v1.0/me and /v1.0/users/{id}
// ---------------------------------------------------------------------------

#[test]
fn deserialize_user_me() {
    let json = fixture("user_me.json");
    let user: GraphUser = serde_json::from_str(&json).expect("GraphUser deserialize");
    assert_eq!(user.id, "U001");
    assert_eq!(user.display_name, "Sheep");
    assert_eq!(user.mail.as_deref(), Some("sheep@contoso.com"));
    assert_eq!(
        user.user_principal_name.as_deref(),
        Some("sheep@contoso.com")
    );
}

// ---------------------------------------------------------------------------
// GraphTeam — single item /v1.0/teams/{id}
// ---------------------------------------------------------------------------

#[test]
fn deserialize_team() {
    let json = fixture("team.json");
    let team: GraphTeam = serde_json::from_str(&json).expect("GraphTeam deserialize");
    assert_eq!(team.id, "T001");
    assert_eq!(team.display_name, "Contoso Corp");
    assert_eq!(team.description.as_deref(), Some("Main company team"));
}

// ---------------------------------------------------------------------------
// ODataResponse<GraphTeam> — list /v1.0/me/joinedTeams
// ---------------------------------------------------------------------------

#[test]
fn deserialize_teams_list() {
    let json = fixture("teams_list.json");
    let resp: ODataResponse<GraphTeam> =
        serde_json::from_str(&json).expect("ODataResponse<GraphTeam> deserialize");
    assert_eq!(resp.value.len(), 2);
    assert_eq!(resp.value[0].id, "T001");
    assert_eq!(resp.value[1].id, "T002");
    assert!(resp.next_link.is_none(), "no nextLink in fixture");
}

// ---------------------------------------------------------------------------
// GraphChannel — single item /v1.0/teams/{id}/channels/{id}
// ---------------------------------------------------------------------------

#[test]
fn deserialize_channel() {
    let json = fixture("channel.json");
    let ch: GraphChannel = serde_json::from_str(&json).expect("GraphChannel deserialize");
    assert_eq!(ch.id, "CH001");
    assert_eq!(ch.display_name, "General");
    assert_eq!(ch.membership_type.as_deref(), Some("standard"));
}

// ---------------------------------------------------------------------------
// ODataResponse<GraphChannel> — list /v1.0/teams/{id}/channels
// ---------------------------------------------------------------------------

#[test]
fn deserialize_channels_list() {
    let json = fixture("channels_list.json");
    let resp: ODataResponse<GraphChannel> =
        serde_json::from_str(&json).expect("ODataResponse<GraphChannel> deserialize");
    assert_eq!(resp.value.len(), 2);
    assert_eq!(resp.value[0].id, "CH001");
    assert_eq!(resp.value[1].display_name, "Engineering");
}

// ---------------------------------------------------------------------------
// GraphMessage — single item from messages list
// ---------------------------------------------------------------------------

#[test]
fn deserialize_message() {
    let json = fixture("message.json");
    let msg: GraphMessage = serde_json::from_str(&json).expect("GraphMessage deserialize");
    assert_eq!(msg.id, "MSG001");
    assert_eq!(msg.body.content, "Good morning team!");
    assert_eq!(msg.body.content_type.as_deref(), Some("text"));
    assert_eq!(msg.created_date_time, "2026-04-05T09:00:00Z");
    let from = msg.from.expect("from present");
    let user = from.user.expect("from.user present");
    assert_eq!(user.id, "U001");
    assert_eq!(user.display_name.as_deref(), Some("Sheep"));
}

// ---------------------------------------------------------------------------
// ODataResponse<GraphMessage> — list /v1.0/teams/{id}/channels/{id}/messages
// ---------------------------------------------------------------------------

#[test]
fn deserialize_messages_list() {
    let json = fixture("messages_list.json");
    let resp: ODataResponse<GraphMessage> =
        serde_json::from_str(&json).expect("ODataResponse<GraphMessage> deserialize");
    assert_eq!(resp.value.len(), 2);
    assert_eq!(resp.value[0].id, "MSG002");
    assert_eq!(resp.value[0].body.content, "Morning! Ready for the standup?");
    assert_eq!(resp.value[1].id, "MSG001");
}

// ---------------------------------------------------------------------------
// GraphChat — /v1.0/me/chats item
// ---------------------------------------------------------------------------

#[test]
fn deserialize_chat() {
    let json = fixture("chat.json");
    let chat: GraphChat = serde_json::from_str(&json).expect("GraphChat deserialize");
    assert_eq!(chat.id, "CHAT001");
    assert_eq!(chat.chat_type, "oneOnOne");
    assert!(chat.topic.is_none(), "topic is null in fixture");
    assert_eq!(chat.members.len(), 2);
    assert_eq!(chat.members[0].id, "member-U001");
    assert_eq!(chat.members[0].user_id.as_deref(), Some("U001"));
    assert_eq!(chat.members[1].display_name.as_deref(), Some("Walrus"));
}

// ---------------------------------------------------------------------------
// TokenResponse — device-code / PKCE token endpoint response
// ---------------------------------------------------------------------------

#[test]
fn deserialize_token_response() {
    let json = fixture("token_response.json");
    let tok: TokenResponse = serde_json::from_str(&json).expect("TokenResponse deserialize");
    assert!(tok.access_token.starts_with("eyJ0eXAiOiJKV1Qi"));
    assert_eq!(tok.token_type, "Bearer");
    assert_eq!(tok.expires_in, 3600);
    assert!(tok.refresh_token.is_some(), "refresh_token present");
    let scope = tok.scope.expect("scope present");
    assert!(scope.contains("User.Read"), "scope contains User.Read");
    assert!(scope.contains("offline_access"), "scope contains offline_access");
}

// ---------------------------------------------------------------------------
// GraphError — 4xx error payload from Graph
// ---------------------------------------------------------------------------

#[test]
fn deserialize_graph_error() {
    let json = fixture("graph_error.json");
    let err: GraphError = serde_json::from_str(&json).expect("GraphError deserialize");
    assert_eq!(err.error.code, "InvalidAuthenticationToken");
    assert!(
        err.error.message.contains("Access token"),
        "message mentions token"
    );
}

// ---------------------------------------------------------------------------
// GraphError::into_client_error status mapping
// ---------------------------------------------------------------------------

#[test]
fn graph_error_maps_401_to_auth_failed() {
    use poly_client::ClientError;
    let err = GraphError {
        error: poly_teams::types::GraphErrorBody {
            code: "InvalidAuthenticationToken".into(),
            message: "bad token".into(),
        },
    };
    let ce = err.into_client_error(401);
    assert!(matches!(ce, ClientError::AuthFailed(_)));
}

#[test]
fn graph_error_maps_404_to_not_found() {
    use poly_client::ClientError;
    let err = GraphError {
        error: poly_teams::types::GraphErrorBody {
            code: "ResourceNotFound".into(),
            message: "not there".into(),
        },
    };
    let ce = err.into_client_error(404);
    assert!(matches!(ce, ClientError::NotFound(_)));
}

#[test]
fn graph_error_maps_429_to_network() {
    use poly_client::ClientError;
    let err = GraphError {
        error: poly_teams::types::GraphErrorBody {
            code: "TooManyRequests".into(),
            message: "slow down".into(),
        },
    };
    let ce = err.into_client_error(429);
    assert!(matches!(ce, ClientError::Network(_)));
}

// ---------------------------------------------------------------------------
// ODataResponse nextLink round-trip
// ---------------------------------------------------------------------------

#[test]
fn odata_response_preserves_next_link() {
    let json = r#"{
        "value": [{"id": "T001", "displayName": "Contoso", "description": null}],
        "@odata.nextLink": "https://graph.microsoft.com/v1.0/me/joinedTeams?$skiptoken=abc123"
    }"#;
    let resp: ODataResponse<GraphTeam> =
        serde_json::from_str(json).expect("ODataResponse nextLink deserialize");
    assert_eq!(resp.value.len(), 1);
    assert_eq!(
        resp.next_link.as_deref(),
        Some("https://graph.microsoft.com/v1.0/me/joinedTeams?$skiptoken=abc123")
    );
}
