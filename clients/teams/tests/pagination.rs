//! `@odata.nextLink` following.
//!
//! Microsoft Graph pages every list endpoint. Taking only `value` from the
//! first response silently truncates the team / chat / member lists, which in
//! turn makes owner-detection in the moderation surface report `false` for a
//! real owner who lands on page 2. These tests stand up a tiny two-page Graph
//! stub and assert the client walks the cursor.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use poly_client::{DmsAndGroupsBackend, IsBackend, ModerationBackend};
use poly_teams::TeamsClient;
use tokio::net::TcpListener;

/// Two-page stub for the handful of OData list endpoints the client walks.
struct PagedServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

#[derive(Clone)]
struct StubState {
    base_url: Arc<std::sync::Mutex<String>>,
}

impl StubState {
    fn origin(&self) -> String {
        self.base_url
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

fn page(values: serde_json::Value, next: Option<String>) -> Json<serde_json::Value> {
    let mut body = serde_json::Map::new();
    let _inserted_value = body.insert("value".into(), values);
    if let Some(link) = next {
        let _inserted_link = body.insert("@odata.nextLink".into(), serde_json::Value::String(link));
    }
    Json(serde_json::Value::Object(body))
}

fn team(id: &str) -> serde_json::Value {
    serde_json::json!({ "id": id, "displayName": format!("Team {id}") })
}

fn member(id: &str, roles: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "id": format!("m-{id}"),
        "userId": id,
        "displayName": format!("User {id}"),
        "roles": roles,
    })
}

async fn joined_teams_p1(State(st): State<StubState>) -> Json<serde_json::Value> {
    page(
        serde_json::json!([team("T1")]),
        Some(format!("{}/page2/joinedTeams", st.origin())),
    )
}

async fn joined_teams_p2() -> Json<serde_json::Value> {
    page(serde_json::json!([team("T2")]), None)
}

async fn team_members_p1(State(st): State<StubState>) -> Json<serde_json::Value> {
    page(
        serde_json::json!([member("U-other", &[])]),
        Some(format!("{}/page2/members", st.origin())),
    )
}

async fn team_members_p2() -> Json<serde_json::Value> {
    // The caller ("me") is an owner, but only on the SECOND page — exactly the
    // case that made get_my_permissions report a non-owner before paging.
    page(serde_json::json!([member("ME", &["owner"])]), None)
}

async fn chats_p1(State(st): State<StubState>) -> Json<serde_json::Value> {
    page(
        serde_json::json!([{ "id": "C1", "chatType": "oneOnOne", "topic": null, "members": [] }]),
        Some(format!("{}/page2/chats", st.origin())),
    )
}

async fn chats_p2() -> Json<serde_json::Value> {
    page(
        serde_json::json!([{ "id": "C2", "chatType": "oneOnOne", "topic": null, "members": [] }]),
        None,
    )
}

/// A server that echoes its own cursor. The client must not spin forever.
async fn self_referential(State(st): State<StubState>) -> Json<serde_json::Value> {
    page(
        serde_json::json!([{ "id": "CH1", "displayName": "General", "description": null }]),
        Some(format!("{}/v1.0/teams/T1/channels", st.origin())),
    )
}

async fn me() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": "ME", "displayName": "Me", "mail": "me@example.test" }))
}

impl PagedServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let base_url = format!("http://127.0.0.1:{port}");

        let state = StubState {
            base_url: Arc::new(std::sync::Mutex::new(base_url.clone())),
        };

        let app = Router::new()
            .route("/v1.0/me", get(me))
            .route("/v1.0/me/joinedTeams", get(joined_teams_p1))
            .route("/page2/joinedTeams", get(joined_teams_p2))
            .route("/v1.0/teams/{team_id}/members", get(team_members_p1))
            .route("/page2/members", get(team_members_p2))
            .route("/v1.0/me/chats", get(chats_p1))
            .route("/page2/chats", get(chats_p2))
            .route("/v1.0/teams/{team_id}/channels", get(self_referential))
            .with_state(state);

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let _server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _closed = rx.await;
                })
                .await
                .ok();
        });
        Self {
            base_url,
            _shutdown: tx,
        }
    }
}

fn client(base_url: &str) -> TeamsClient {
    TeamsClient::with_base_url(base_url.to_string())
}

#[tokio::test]
async fn get_servers_follows_next_link() {
    let srv = PagedServer::start().await;
    let c = client(&srv.base_url);
    let servers = c.get_servers().await.expect("get_servers");
    let ids: Vec<_> = servers.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["T1", "T2"], "second joinedTeams page was dropped");
}

#[tokio::test]
async fn owner_on_second_member_page_is_still_an_owner() {
    let srv = PagedServer::start().await;
    let mut c = client(&srv.base_url);
    c.authenticate(poly_client::AuthCredentials::Token("t".into()))
        .await
        .expect("authenticate");

    let perms = c
        .get_my_permissions("T1", None)
        .await
        .expect("get_my_permissions");
    assert!(
        perms.manage_channels,
        "owner listed on page 2 of /members was reported as a plain member"
    );
}

#[tokio::test]
async fn get_dm_channels_follows_next_link() {
    let srv = PagedServer::start().await;
    let c = client(&srv.base_url);
    let dms = c.get_dm_channels().await.expect("get_dm_channels");
    assert_eq!(dms.len(), 2, "second /me/chats page was dropped");
}

#[tokio::test]
async fn self_referential_next_link_terminates() {
    let srv = PagedServer::start().await;
    let c = client(&srv.base_url);
    // Would hang forever without the self-link guard; the test harness would
    // simply never finish, so reaching the assert at all is the assertion.
    let channels = c.get_channels("T1").await.expect("get_channels");
    assert_eq!(channels.len(), 1);
}
