//! Integration tests for `poly-discord` against the mock Discord test server.
//!
//! Each test spins up a `poly_test_discord` router on a random port, seeds
//! demo data, authenticates via `/test/auth/token`, then exercises the full
//! `ClientBackend` API surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use poly_client::{
    AuthCredentials, BackendType, ChannelType, ClientBackend, ForumSortOrder, MessageContent,
    MessageQuery, PresenceStatus,
};
use poly_discord::DiscordClient;
use poly_test_discord::{DiscordState, router};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Test server helpers
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let base_url = format!("http://127.0.0.1:{port}");

        let state = Arc::new(DiscordState::new());
        state.seed();

        let app = router(Arc::clone(&state));
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { rx.await.ok(); })
                .await
                .ok();
        });
        Self { base_url, _shutdown: tx }
    }

    /// Obtain a user token via the test-only easy-signin endpoint.
    async fn token_for(&self, username: &str) -> String {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/test/auth/token", self.base_url))
            .json(&serde_json::json!({ "username": username }))
            .send()
            .await
            .expect("test_auth_token POST")
            .json()
            .await
            .expect("parse token response");
        resp["token"].as_str().expect("token field").to_string()
    }

    /// Build a `DiscordClient` and authenticate as the given username.
    async fn authenticated_client(&self, username: &str) -> DiscordClient {
        let token = self.token_for(username).await;
        let mut client = DiscordClient::with_base_url(self.base_url.clone());
        client
            .authenticate(AuthCredentials::Token(token))
            .await
            .expect("authenticate");
        client
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_authenticate_and_session() {
    let srv = TestServer::start().await;
    let token = srv.token_for("koala").await;
    let mut client = DiscordClient::with_base_url(srv.base_url.clone());

    assert!(!client.is_authenticated());
    let session = client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("authenticate");

    assert!(client.is_authenticated());
    assert_eq!(session.user.display_name, "koala");
    assert_eq!(session.backend, BackendType::from("discord"));
}

#[tokio::test]
async fn test_authenticate_invalid_token_fails() {
    let srv = TestServer::start().await;
    let mut client = DiscordClient::with_base_url(srv.base_url.clone());
    let result = client
        .authenticate(AuthCredentials::Token("not-a-real-token".to_string()))
        .await;
    assert!(result.is_err(), "bad token should fail");
}

#[tokio::test]
async fn test_logout_clears_auth() {
    let srv = TestServer::start().await;
    let mut client = srv.authenticated_client("koala").await;
    assert!(client.is_authenticated());
    client.logout().await.expect("logout");
    assert!(!client.is_authenticated());
}

#[tokio::test]
async fn test_get_servers() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let servers = client.get_servers().await.expect("get_servers");
    assert!(!servers.is_empty(), "should have at least one guild");
    let names: Vec<_> = servers.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Australiana"), "Australiana guild expected");
    assert!(names.contains(&"Wildlife Chat"), "Wildlife Chat guild expected");
    for s in &servers {
        assert_eq!(s.backend, BackendType::from("discord"));
    }
}

#[tokio::test]
async fn test_get_server_by_id() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let server = client.get_server("100").await.expect("get_server guild 100");
    assert_eq!(server.id, "100");
    assert_eq!(server.name, "Australiana");
}

#[tokio::test]
async fn test_get_channels() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let channels = client.get_channels("100").await.expect("get_channels guild 100");
    assert!(!channels.is_empty());
    let names: Vec<_> = channels.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"general"), "general channel expected");
    assert!(names.contains(&"random"), "random channel expected");
    for ch in &channels {
        // Guild 100 contains text channels, a forum channel, and a wildlife-news
        // text channel. All channel types returned by get_channels are valid
        // text-like types — no voice/thread-only types should appear.
        assert!(
            matches!(
                ch.channel_type,
                ChannelType::Text | ChannelType::Forum | ChannelType::Announcement
            ),
            "unexpected channel type {:?} for channel {}",
            ch.channel_type,
            ch.name,
        );
        assert_eq!(ch.server_id, "100");
    }
}

#[tokio::test]
async fn test_get_channel_by_id() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let ch = client.get_channel("200").await.expect("get_channel 200");
    assert_eq!(ch.id, "200");
    assert_eq!(ch.name, "general");
    assert_eq!(ch.channel_type, ChannelType::Text);
}

#[tokio::test]
async fn test_get_messages() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let msgs = client
        .get_messages("200", MessageQuery { limit: Some(10), before: None, after: None, around: None })
        .await
        .expect("get_messages");
    assert!(!msgs.is_empty(), "channel 200 should have seeded messages");
    let has_gday = msgs.iter().any(|m| {
        matches!(&m.content, MessageContent::Text(t) if t.contains("G'day"))
    });
    assert!(has_gday, "expected seeded G'day message");
}

#[tokio::test]
async fn test_send_message() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let msg = client
        .send_message("200", MessageContent::Text("Hello from test!".to_string()))
        .await
        .expect("send_message");
    assert_eq!(msg.content, MessageContent::Text("Hello from test!".to_string()));
    assert_eq!(msg.author.display_name, "koala");
}

#[tokio::test]
async fn test_send_then_read_message() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let sent = client
        .send_message("201", MessageContent::Text("Cross-channel ping!".to_string()))
        .await
        .expect("send to channel 201");

    let msgs = client
        .get_messages("201", MessageQuery { limit: Some(20), before: None, after: None, around: None })
        .await
        .expect("get_messages 201");
    assert!(
        msgs.iter().any(|m| m.id == sent.id),
        "sent message should appear in get_messages"
    );
}

#[tokio::test]
async fn test_get_dm_channels() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    // DM channels may be empty in a seeded state — just check it doesn't error
    let _dms = client.get_dm_channels().await.expect("get_dm_channels");
}

#[tokio::test]
async fn test_get_user() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let user = client.get_user("2").await.expect("get_user 2");
    assert_eq!(user.id, "2");
    assert_eq!(user.display_name, "kangaroo");
    assert_eq!(user.backend, BackendType::from("discord"));
}

#[tokio::test]
async fn test_backend_type_and_name() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    assert_eq!(client.backend_type(), BackendType::from("discord"));
    assert_eq!(client.backend_name(), "Discord");
}

#[tokio::test]
async fn test_presence_and_friends_stubs() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let presence = client.get_presence("1").await.expect("get_presence");
    assert_eq!(presence, PresenceStatus::Offline);
    let friends = client.get_friends().await.expect("get_friends");
    assert!(friends.is_empty());
}

#[tokio::test]
async fn test_concurrent_sessions_isolated() {
    let srv = TestServer::start().await;
    let mut koala = DiscordClient::with_base_url(srv.base_url.clone());
    let mut kangaroo = DiscordClient::with_base_url(srv.base_url.clone());

    let tok_k = srv.token_for("koala").await;
    let tok_r = srv.token_for("kangaroo").await;

    let sess_k = koala.authenticate(AuthCredentials::Token(tok_k)).await.expect("koala auth");
    let sess_r = kangaroo.authenticate(AuthCredentials::Token(tok_r)).await.expect("kangaroo auth");

    assert_eq!(sess_k.user.display_name, "koala");
    assert_eq!(sess_r.user.display_name, "kangaroo");
    assert_ne!(sess_k.token, sess_r.token);
}

// ─── Phase 3 — Forum channels + threads ──────────────────────────────────

/// get_channels for guild 100 includes the forum channel (id=500, type=Forum).
#[tokio::test]
async fn test_get_channels_includes_forum() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let channels = client.get_channels("100").await.expect("get_channels");
    let forum = channels.iter().find(|c| c.id == "500");
    assert!(forum.is_some(), "forum channel 500 should be returned by get_channels");
    let forum = forum.unwrap();
    assert_eq!(forum.channel_type, ChannelType::Forum);
    let tags = forum.forum_tags.as_ref().expect("forum_tags should be populated");
    assert_eq!(tags.len(), 3);
    let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"question"));
    assert!(names.contains(&"show-and-tell"));
    assert!(names.contains(&"announcement"));
}

/// get_channels for guild 101 includes the media channel (id=600, type=Forum).
#[tokio::test]
async fn test_get_channels_includes_media_as_forum() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let channels = client.get_channels("101").await.expect("get_channels guild 101");
    let media = channels.iter().find(|c| c.id == "600");
    assert!(media.is_some(), "media channel 600 should be returned");
    assert_eq!(media.unwrap().channel_type, ChannelType::Forum);
}

/// get_channel for a thread ID returns ChannelType::Thread with parent_channel_id set.
#[tokio::test]
async fn test_get_channel_thread_type() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let ch = client.get_channel("501").await.expect("get_channel 501");
    assert_eq!(ch.channel_type, ChannelType::Thread);
    assert_eq!(ch.parent_channel_id.as_deref(), Some("500"));
    let meta = ch.thread_metadata.expect("thread_metadata should be populated");
    assert!(!meta.archived);
    assert!(!meta.locked);
    assert_eq!(meta.auto_archive_minutes, 1440);
}

/// get_active_threads returns non-archived threads for guild 100.
#[tokio::test]
async fn test_get_active_threads() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let threads = client.get_active_threads("100").await.expect("get_active_threads");
    // Seeded: 501, 502, 511 are active; 503 is archived.
    let ids: Vec<&str> = threads.iter().map(|t| t.thread_id.as_str()).collect();
    assert!(ids.contains(&"501"), "thread 501 should be active");
    assert!(ids.contains(&"502"), "thread 502 should be active");
    assert!(ids.contains(&"511"), "thread 511 should be active");
    assert!(!ids.contains(&"503"), "archived thread 503 should not appear");
}

/// get_archived_threads returns archived threads for forum channel 500.
#[tokio::test]
async fn test_get_archived_threads() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let threads = client.get_archived_threads("500", None).await.expect("get_archived_threads");
    assert_eq!(threads.len(), 1, "exactly 1 archived thread under channel 500");
    assert_eq!(threads[0].thread_id, "503");
    let meta = threads[0].message_count;
    // We can't directly access thread_metadata through ThreadInfo, but the thread
    // should be the archived one (503).
    assert_eq!(meta, 1);
}

/// get_forum_posts for channel 500 returns active posts sorted by latest activity.
#[tokio::test]
async fn test_get_forum_posts_latest_activity() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let posts = client
        .get_forum_posts("500", ForumSortOrder::LatestActivity, Some(10))
        .await
        .expect("get_forum_posts");
    // Active posts: 501, 502 (503 is archived → excluded by active threads filter).
    let ids: Vec<&str> = posts.iter().map(|p| p.thread.thread_id.as_str()).collect();
    assert!(ids.contains(&"501"), "post 501 missing");
    assert!(ids.contains(&"502"), "post 502 missing");
    assert!(!ids.contains(&"503"), "archived post 503 should not appear");
}

/// get_forum_posts sorted by creation date returns newer post first.
#[tokio::test]
async fn test_get_forum_posts_creation_date_order() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let posts = client
        .get_forum_posts("500", ForumSortOrder::CreationDate, Some(10))
        .await
        .expect("get_forum_posts CreationDate");
    // 501 created 2026-04-10, 502 created 2026-04-11. Descending → 502 first.
    let ids: Vec<&str> = posts.iter().map(|p| p.thread.thread_id.as_str()).collect();
    if ids.len() >= 2 {
        assert_eq!(ids[0], "502", "502 (newer) should sort first by creation date");
        assert_eq!(ids[1], "501");
    }
}

/// Messages in wildlife-news (510) carry inline thread info for spawned threads.
#[tokio::test]
async fn test_message_thread_field() {
    let srv = TestServer::start().await;
    let client = srv.authenticated_client("koala").await;
    let msgs = client
        .get_messages("510", MessageQuery { limit: Some(10), before: None, after: None, around: None })
        .await
        .expect("get_messages 510");
    let msg520 = msgs.iter().find(|m| m.id == "520").expect("message 520 not found");
    let thread_info = msg520.thread.as_ref().expect("Message.thread should be Some for msg 520");
    assert_eq!(thread_info.thread_id, "511");
    assert_eq!(thread_info.parent_channel_id, "510");
}
