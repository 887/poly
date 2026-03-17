//! Integration tests for the native Stoat client transport.
//!
//! These tests spin up a mock Stoat-compatible HTTP server and exercise the
//! real `poly-stoat` login/logout flow over HTTP. They are our current
//! end-to-end-style coverage for the native Stoat transport while the WASM
//! plugin guest remains a stub.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use poly_client::{
    AuthCredentials, BackendType, ChannelType, ClientBackend, ClientError, MessageContent,
    MessageQuery, PresenceStatus,
};
use poly_stoat::StoatClient;
use serde_json::{Value, json};
use tokio::net::TcpListener as TokioListener;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginMode {
    Success,
    Mfa,
    Disabled,
}

#[derive(Clone)]
struct TestState {
    mode: LoginMode,
    addr: String,
}

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start(mode: LoginMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind free port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let addr = format!("127.0.0.1:{port}");
        let base_url = format!("http://{addr}");
        let state = Arc::new(TestState {
            mode,
            addr: addr.clone(),
        });

        let app = Router::new()
            .route("/", get(root_config))
            .route("/auth/session/login", post(login))
            .route("/auth/session/logout", post(logout))
            .route("/users/@me", get(fetch_self))
            .route("/users/{target}", get(fetch_user))
            .route("/servers/{target}", get(fetch_server))
            .route("/servers/{target}/members", get(fetch_server_members))
            .route("/channels/{target}", get(fetch_channel))
            .route("/channels/{target}/messages", get(fetch_messages))
            .route("/sync/unreads", get(fetch_unreads))
            .with_state(state);

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let tcp = TokioListener::bind(&addr)
            .await
            .expect("bind tokio listener");

        tokio::spawn(async move {
            axum::serve(tcp, app)
                .with_graceful_shutdown(async {
                    rx.await.ok();
                })
                .await
                .expect("serve mock stoat");
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        Self {
            base_url,
            _shutdown: tx,
        }
    }
}

async fn root_config(State(state): State<Arc<TestState>>) -> Json<Value> {
    Json(json!({
        "revolt": "0.11.5",
        "features": {
            "captcha": { "enabled": false, "key": null },
            "email": true,
            "invite_only": false,
            "autumn": { "enabled": true, "url": format!("http://{}/autumn", state.addr) },
            "january": { "enabled": true, "url": format!("http://{}/january", state.addr) },
            "livekit": { "enabled": false, "nodes": [] }
        },
        "ws": format!("ws://{}/events", state.addr),
        "app": format!("http://{}/app", state.addr),
        "vapid": "test-vapid",
        "build": {
            "commit_sha": "deadbeef",
            "commit_timestamp": "2026-03-16T00:00:00Z",
            "semver": "0.11.5",
            "origin_url": "https://stoat.chat"
        }
    }))
}

async fn login(
    State(state): State<Arc<TestState>>,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let email = payload
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let password = payload
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if email != "alice@example.test" || password != "correct horse battery staple" {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "type": "InvalidCredentials" })),
        );
    }

    let body = match state.mode {
        LoginMode::Success => json!({
            "result": "Success",
            "_id": "session_1",
            "user_id": "user_1",
            "token": "test-session-token",
            "name": "Poly",
            "last_seen": "2026-03-16T00:00:00Z",
            "origin": null
        }),
        LoginMode::Mfa => json!({
            "result": "MFA",
            "ticket": "ticket_1",
            "allowed_methods": ["Password", "Totp"]
        }),
        LoginMode::Disabled => json!({
            "result": "Disabled",
            "user_id": "user_1"
        }),
    };

    (StatusCode::OK, Json(body))
}

async fn fetch_self(headers: HeaderMap) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if token != "test-session-token" && token != "restored-token" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "type": "InvalidSession" })),
        ));
    }

    Ok(Json(json!({
        "_id": "user_1",
        "username": "stoaty",
        "discriminator": "0001",
        "display_name": "Stoaty McStoat",
        "avatar": null,
        "status": { "presence": "Focus" },
        "relationship": "User",
        "online": true
    })))
}

async fn logout(headers: HeaderMap) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if token != "test-session-token" && token != "restored-token" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "type": "InvalidSession" })),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn fetch_user(
    headers: HeaderMap,
    Path(target): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if token != "test-session-token" && token != "restored-token" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "type": "InvalidSession" })),
        ));
    }

    match target.as_str() {
        "user_1" => Ok(Json(json!({
            "_id": "user_1",
            "username": "stoaty",
            "discriminator": "0001",
            "display_name": "Stoaty McStoat",
            "avatar": {
                "_id": "avatar-user-1",
                "tag": "avatars",
                "filename": "avatar.png",
                "content_type": "image/png",
                "size": 1234
            },
            "status": { "presence": "Focus" },
            "online": true
        }))),
        "user_2" => Ok(Json(json!({
            "_id": "user_2",
            "username": "otterpal",
            "discriminator": "0002",
            "display_name": null,
            "avatar": null,
            "status": { "presence": "Idle" },
            "online": true
        }))),
        _ => Err((StatusCode::NOT_FOUND, Json(json!({ "type": "NotFound" })))),
    }
}

async fn fetch_server(
    headers: HeaderMap,
    Path(target): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if token != "test-session-token" && token != "restored-token" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "type": "InvalidSession" })),
        ));
    }

    match target.as_str() {
        "server_1" => Ok(Json(json!({
            "_id": "server_1",
            "owner": "user_1",
            "name": "Stoat Testing Grounds",
            "description": "Mock server for native Stoat tests",
            "channels": ["ch_text", "ch_voice"],
            "categories": [
                {
                    "id": "cat_text",
                    "title": "Lobby",
                    "channels": ["ch_text", "ch_voice"]
                }
            ],
            "default_permissions": 0,
            "icon": null,
            "banner": null
        }))),
        _ => Err((StatusCode::NOT_FOUND, Json(json!({ "type": "NotFound" })))),
    }
}

async fn fetch_server_members(
    headers: HeaderMap,
    Path(target): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if token != "test-session-token" && token != "restored-token" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "type": "InvalidSession" })),
        ));
    }

    match target.as_str() {
        "server_1" => Ok(Json(json!({
            "members": [
                {
                    "_id": { "server": "server_1", "user": "user_1" },
                    "nickname": "Captain Stoat",
                    "avatar": {
                        "_id": "member-avatar-1",
                        "tag": "avatars",
                        "filename": "member.png",
                        "content_type": "image/png",
                        "size": 2345
                    }
                },
                {
                    "_id": { "server": "server_1", "user": "user_2" },
                    "nickname": null,
                    "avatar": null
                }
            ],
            "users": [
                {
                    "_id": "user_1",
                    "username": "stoaty",
                    "discriminator": "0001",
                    "display_name": "Stoaty McStoat",
                    "avatar": {
                        "_id": "avatar-user-1",
                        "tag": "avatars",
                        "filename": "avatar.png",
                        "content_type": "image/png",
                        "size": 1234
                    },
                    "status": { "presence": "Focus" },
                    "online": true
                },
                {
                    "_id": "user_2",
                    "username": "otterpal",
                    "discriminator": "0002",
                    "display_name": null,
                    "avatar": null,
                    "status": { "presence": "Idle" },
                    "online": true
                }
            ]
        }))),
        _ => Err((StatusCode::NOT_FOUND, Json(json!({ "type": "NotFound" })))),
    }
}

async fn fetch_channel(
    headers: HeaderMap,
    Path(target): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if token != "test-session-token" && token != "restored-token" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "type": "InvalidSession" })),
        ));
    }

    let body = match target.as_str() {
        "ch_text" => json!({
            "channel_type": "TextChannel",
            "_id": "ch_text",
            "server": "server_1",
            "name": "general",
            "description": "General testing chat",
            "last_message_id": "msg_2",
            "default_permissions": { "a": 0, "d": 0 },
            "role_permissions": {},
            "nsfw": false,
            "voice": null
        }),
        "ch_voice" => json!({
            "channel_type": "TextChannel",
            "_id": "ch_voice",
            "server": "server_1",
            "name": "voice lounge",
            "description": "Voice-enabled room",
            "last_message_id": null,
            "default_permissions": { "a": 0, "d": 0 },
            "role_permissions": {},
            "nsfw": false,
            "voice": { "max_users": 12 }
        }),
        "dm_1" => json!({
            "channel_type": "DirectMessage",
            "_id": "dm_1",
            "active": true,
            "recipients": ["user_1", "user_2"],
            "last_message_id": "msg_dm_1"
        }),
        _ => {
            return Err((StatusCode::NOT_FOUND, Json(json!({ "type": "NotFound" }))));
        }
    };

    Ok(Json(body))
}

async fn fetch_unreads(headers: HeaderMap) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if token != "test-session-token" && token != "restored-token" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "type": "InvalidSession" })),
        ));
    }

    Ok(Json(json!([
        {
            "_id": { "channel": "ch_text", "user": "user_1" },
            "last_id": "msg_1",
            "mentions": ["msg_2", "msg_3"]
        },
        {
            "_id": { "channel": "ch_voice", "user": "user_1" },
            "last_id": "voice_ping",
            "mentions": []
        }
    ])))
}

async fn fetch_messages(
    headers: HeaderMap,
    Path(target): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if token != "test-session-token" && token != "restored-token" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "type": "InvalidSession" })),
        ));
    }

    let body = match target.as_str() {
        "ch_text" => json!({
            "messages": [
                {
                    "_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                    "channel": "ch_text",
                    "author": "user_2",
                    "content": "Original message",
                    "user": null,
                    "member": {
                        "_id": { "server": "server_1", "user": "user_2" },
                        "nickname": "Captain Stoat",
                        "avatar": null
                    },
                    "attachments": [
                        {
                            "_id": "file_1",
                            "tag": "attachments",
                            "filename": "diagram.png",
                            "metadata": null,
                            "content_type": "image/png",
                            "size": 2048
                        }
                    ],
                    "edited": null,
                    "replies": [],
                    "reactions": { "🦦": ["user_1", "user_3"] }
                },
                {
                    "_id": "01ARZ3NDEM4YFFFG8D2M5KZG6T",
                    "channel": "ch_text",
                    "author": "user_1",
                    "content": "Reply message",
                    "user": null,
                    "member": null,
                    "attachments": [],
                    "edited": "2016-07-30T23:20:00Z",
                    "replies": ["01ARZ3NDEKTSV4RRFFQ69G5FAV"],
                    "reactions": {}
                }
            ],
            "users": [
                {
                    "_id": "user_1",
                    "username": "stoaty",
                    "discriminator": "0001",
                    "display_name": "Stoaty McStoat",
                    "avatar": null,
                    "status": { "presence": "Focus" },
                    "online": true
                },
                {
                    "_id": "user_2",
                    "username": "otterpal",
                    "discriminator": "0002",
                    "display_name": "Otter Pal",
                    "avatar": null,
                    "status": { "presence": "Online" },
                    "online": true
                }
            ],
            "members": [
                {
                    "_id": { "server": "server_1", "user": "user_2" },
                    "joined_at": "2016-07-30T23:19:00Z",
                    "nickname": "Captain Stoat",
                    "avatar": null,
                    "roles": [],
                    "can_publish": true,
                    "can_receive": true
                }
            ]
        }),
        "ch_array" => json!([
            {
                "_id": "01ARZ3NDEP6D2T6R8H5M5W4Q9Z",
                "channel": "ch_array",
                "author": "user_1",
                "content": "Array response works",
                "user": {
                    "_id": "user_1",
                    "username": "stoaty",
                    "discriminator": "0001",
                    "display_name": "Stoaty McStoat",
                    "avatar": null,
                    "status": { "presence": "Focus" },
                    "online": true
                },
                "member": null,
                "attachments": [],
                "edited": null,
                "replies": [],
                "reactions": {}
            }
        ]),
        _ => {
            return Err((StatusCode::NOT_FOUND, Json(json!({ "type": "NotFound" }))));
        }
    };

    Ok(Json(body))
}

#[tokio::test]
async fn stoat_fetch_server_config_round_trip() {
    let server = TestServer::start(LoginMode::Success).await;
    let client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    let config = client.fetch_server_config().await.expect("fetch config");
    assert_eq!(config.revolt, "0.11.5");
    assert_eq!(
        config.ws,
        format!(
            "ws://{}/events",
            server.base_url.trim_start_matches("http://")
        )
    );
}

#[tokio::test]
async fn stoat_authenticate_email_password_success() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    let session = client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await
        .expect("login succeeds");

    assert_eq!(session.id, "session_1".to_string());
    assert_eq!(session.user.id, "user_1".to_string());
    assert_eq!(session.user.display_name, "Stoaty McStoat".to_string());
    assert_eq!(session.user.presence, PresenceStatus::DoNotDisturb);
    assert_eq!(session.backend, BackendType::Stoat);
    assert_eq!(session.token, "test-session-token".to_string());
    assert_eq!(session.icon_emoji, Some("🦦".to_string()));
    assert_eq!(session.backend_url, Some(server.base_url.clone()));
    assert!(client.is_authenticated());
    assert_eq!(
        client.session_token(),
        Some("test-session-token".to_string())
    );
}

#[tokio::test]
async fn stoat_authenticate_with_token_resume() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    let session = client
        .authenticate(AuthCredentials::Token("restored-token".to_string()))
        .await
        .expect("token restore succeeds");

    assert_eq!(session.user.id, "user_1".to_string());
    assert_eq!(session.token, "restored-token".to_string());
    assert_eq!(session.id, "user_1".to_string());
    assert!(client.is_authenticated());
}

#[tokio::test]
async fn stoat_authenticate_mfa_response_returns_auth_failed() {
    let server = TestServer::start(LoginMode::Mfa).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    let result = client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await;

    assert!(matches!(
        result,
        Err(ClientError::AuthFailed(message)) if message.contains("requires MFA")
    ));
    assert!(!client.is_authenticated());
}

#[tokio::test]
async fn stoat_get_user_maps_avatar_and_presence() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url).expect("valid client");
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await
        .expect("login succeeds");

    let user = client
        .get_user("user_1")
        .await
        .expect("user fetch succeeds");

    assert_eq!(user.id, "user_1");
    assert_eq!(user.display_name, "Stoaty McStoat");
    assert_eq!(user.presence, PresenceStatus::DoNotDisturb);
    assert_eq!(
        user.avatar_url,
        Some(format!(
            "{}/autumn/avatars/avatar-user-1",
            client.base_url()
        ))
    );
}

#[tokio::test]
async fn stoat_get_presence_uses_user_status() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url).expect("valid client");
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await
        .expect("login succeeds");

    let presence = client
        .get_presence("user_2")
        .await
        .expect("presence fetch succeeds");

    assert_eq!(presence, PresenceStatus::Idle);
}

#[tokio::test]
async fn stoat_get_channel_members_uses_server_member_overrides() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url).expect("valid client");
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await
        .expect("login succeeds");

    let members = client
        .get_channel_members("ch_text")
        .await
        .expect("channel members fetch succeeds");

    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|user| {
        user.id == "user_1"
            && user.display_name == "Captain Stoat"
            && user.avatar_url
                == Some(format!(
                    "{}/autumn/avatars/member-avatar-1",
                    client.base_url()
                ))
    }));
    assert!(members.iter().any(|user| {
        user.id == "user_2"
            && user.display_name == "otterpal"
            && user.presence == PresenceStatus::Idle
    }));
}

#[tokio::test]
async fn stoat_authenticate_disabled_response_returns_auth_failed() {
    let server = TestServer::start(LoginMode::Disabled).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    let result = client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await;

    assert!(matches!(
        result,
        Err(ClientError::AuthFailed(message)) if message.contains("disabled")
    ));
    assert!(!client.is_authenticated());
}

#[tokio::test]
async fn stoat_logout_clears_native_session() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await
        .expect("login succeeds");

    client.logout().await.expect("logout succeeds");
    assert!(!client.is_authenticated());
    assert_eq!(client.session_token(), None);
}

#[tokio::test]
async fn stoat_get_server_maps_categories_and_unreads() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await
        .expect("login succeeds");

    let detail = client.get_server("server_1").await.expect("get server");

    assert_eq!(detail.id, "server_1");
    assert_eq!(detail.name, "Stoat Testing Grounds");
    assert_eq!(detail.backend, BackendType::Stoat);
    assert_eq!(detail.account_id, "user_1");
    assert_eq!(detail.account_display_name, "Stoaty McStoat");
    assert_eq!(detail.unread_count, 3);
    assert_eq!(detail.mention_count, 2);
    assert_eq!(detail.categories.len(), 1);
    let category = detail.categories.first().expect("category present");
    assert_eq!(category.id, "cat_text");
    assert_eq!(category.name, "Lobby");
    assert_eq!(category.channel_ids, vec!["ch_text", "ch_voice"]);
}

#[tokio::test]
async fn stoat_get_channels_fetches_server_channels_with_unreads() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await
        .expect("login succeeds");

    let channels = client.get_channels("server_1").await.expect("get channels");

    assert_eq!(channels.len(), 2);

    let text = channels
        .iter()
        .find(|channel| channel.id == "ch_text")
        .expect("text channel present");
    assert_eq!(text.name, "general");
    assert_eq!(text.channel_type, ChannelType::Text);
    assert_eq!(text.unread_count, 2);
    assert_eq!(text.mention_count, 2);
    assert_eq!(text.last_message_id.as_deref(), Some("msg_2"));

    let voice = channels
        .iter()
        .find(|channel| channel.id == "ch_voice")
        .expect("voice channel present");
    assert_eq!(voice.channel_type, ChannelType::Voice);
    assert_eq!(voice.unread_count, 1);
    assert_eq!(voice.mention_count, 0);
}

#[tokio::test]
async fn stoat_get_channel_fetches_single_server_channel() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    client
        .authenticate(AuthCredentials::Token("restored-token".to_string()))
        .await
        .expect("token restore succeeds");

    let channel = client.get_channel("ch_text").await.expect("get channel");
    assert_eq!(channel.server_id, "server_1");
    assert_eq!(channel.name, "general");
    assert_eq!(channel.channel_type, ChannelType::Text);
    assert_eq!(channel.unread_count, 2);
    assert_eq!(channel.mention_count, 2);
}

#[tokio::test]
async fn stoat_get_channel_rejects_dm_channels() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    client
        .authenticate(AuthCredentials::Token("restored-token".to_string()))
        .await
        .expect("token restore succeeds");

    let result = client.get_channel("dm_1").await;
    assert!(matches!(
        result,
        Err(ClientError::NotSupported(message)) if message.contains("not a server channel")
    ));
}

#[tokio::test]
async fn stoat_get_messages_maps_users_replies_attachments_and_reactions() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "alice@example.test".to_string(),
            password: "correct horse battery staple".to_string(),
        })
        .await
        .expect("login succeeds");

    let messages = client
        .get_messages(
            "ch_text",
            MessageQuery {
                limit: Some(10),
                ..Default::default()
            },
        )
        .await
        .expect("get messages");

    assert_eq!(messages.len(), 2);

    let original = messages.first().expect("first message present");
    assert_eq!(original.author.display_name, "Captain Stoat");
    assert!(matches!(
        &original.content,
        MessageContent::WithAttachments { text, attachments }
            if text == "Original message"
                && attachments.len() == 1
                && attachments
                    .first()
                    .is_some_and(|attachment| attachment.url.ends_with("/attachments/file_1"))
    ));
    assert_eq!(original.attachments.len(), 1);
    assert_eq!(original.reactions.len(), 1);
    let reaction = original.reactions.first().expect("reaction present");
    assert_eq!(reaction.emoji, "🦦");
    assert_eq!(reaction.count, 2);
    assert!(reaction.me);

    let reply = messages.get(1).expect("reply message present");
    assert_eq!(reply.author.display_name, "Stoaty McStoat");
    assert!(reply.edited);
    assert_eq!(
        reply
            .reply_to
            .as_ref()
            .map(|preview| preview.message_id.as_str()),
        Some("01ARZ3NDEKTSV4RRFFQ69G5FAV")
    );
    assert_eq!(
        reply
            .reply_to
            .as_ref()
            .map(|preview| preview.snippet.as_str()),
        Some("Original message")
    );
}

#[tokio::test]
async fn stoat_get_messages_accepts_plain_array_bulk_response() {
    let server = TestServer::start(LoginMode::Success).await;
    let mut client = StoatClient::with_base_url(server.base_url.clone()).expect("valid base url");

    client
        .authenticate(AuthCredentials::Token("restored-token".to_string()))
        .await
        .expect("token restore succeeds");

    let messages = client
        .get_messages(
            "ch_array",
            MessageQuery {
                around: Some("01ARZ3NDEP6D2T6R8H5M5W4Q9Z".to_string()),
                limit: Some(16),
                ..Default::default()
            },
        )
        .await
        .expect("array response succeeds");

    assert_eq!(messages.len(), 1);
    let message = messages.first().expect("array response message present");
    assert_eq!(message.id, "01ARZ3NDEP6D2T6R8H5M5W4Q9Z");
    assert!(matches!(
        &message.content,
        MessageContent::Text(text) if text == "Array response works"
    ));
}
