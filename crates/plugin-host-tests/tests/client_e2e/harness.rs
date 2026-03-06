//! Shared test harness for ClientBackend interface contract testing.
//!
//! Provides reusable async test functions that exercise each part of the
//! `ClientBackend` trait through the WASM plugin host. Each client's test
//! module calls these functions after instantiating its plugin.
//!
//! ## Categories
//!
//! - **Identity** — `backend_type()`, `backend_name()`
//! - **Lifecycle** — `authenticate()`, `is_authenticated()`, `logout()`
//! - **Data Retrieval** — `get_servers()`, `get_channels()`, `get_messages()`, etc.
//! - **Mutations** — `send_message()`, `set_presence()`, `remove_group_member()`
//! - **Events** — `event_stream()` returns a valid stream

use poly_client::{
    AuthCredentials, BackendType, ClientBackend, ClientError, MessageContent, MessageQuery,
    PresenceStatus,
};
use poly_plugin_host::PluginBackend;

// ─── Identity Tests ────────────────────────────────────────────────

/// Verify the plugin reports the expected backend type.
pub fn assert_backend_type(backend: &PluginBackend, expected: BackendType) {
    assert_eq!(backend.backend_type(), expected, "backend_type() mismatch");
}

/// Verify the plugin reports the expected backend name.
pub fn assert_backend_name(backend: &PluginBackend, expected: &str) {
    assert_eq!(backend.backend_name(), expected, "backend_name() mismatch");
}

// ─── Lifecycle Tests ───────────────────────────────────────────────

/// Authenticate with token credentials and verify the session is valid.
///
/// Returns the resulting `Session` for further inspection.
pub async fn authenticate_with_token(
    backend: &mut PluginBackend,
    token: &str,
) -> poly_client::Session {
    let creds = AuthCredentials::Token(token.to_string());
    let session = backend
        .authenticate(creds)
        .await
        .expect("authenticate() should succeed");

    // Session must have non-empty fields
    assert!(!session.id.is_empty(), "session.id should not be empty");
    assert!(
        !session.user.id.is_empty(),
        "session.user.id should not be empty"
    );
    assert!(
        !session.user.display_name.is_empty(),
        "session.user.display_name should not be empty"
    );

    session
}

/// Verify that authenticate returns an error for stubs that are not implemented.
pub async fn authenticate_returns_error(backend: &mut PluginBackend) {
    let creds = AuthCredentials::Token("test-token".to_string());
    let result = backend.authenticate(creds).await;
    assert!(
        result.is_err(),
        "Stub authenticate() should return an error"
    );
}

/// Verify logout succeeds (or is a no-op for stubs).
pub async fn logout_succeeds(backend: &mut PluginBackend) {
    let result = backend.logout().await;
    // Both Ok(()) and Err(Internal("not implemented")) are acceptable
    // — we just verify it doesn't panic/trap
    let _ = result;
}

// ─── Data Retrieval: Servers ───────────────────────────────────────

/// Get servers and verify the response is valid.
/// Returns the list for further inspection.
pub async fn get_servers(backend: &PluginBackend) -> Vec<poly_client::Server> {
    backend
        .get_servers()
        .await
        .expect("get_servers() should not error")
}

/// Get servers and verify we got at least `min_count` entries.
pub async fn get_servers_non_empty(backend: &PluginBackend, min_count: usize) {
    let servers = get_servers(backend).await;
    assert!(
        servers.len() >= min_count,
        "Expected at least {min_count} servers, got {}",
        servers.len()
    );

    // Each server should have non-empty id and name
    for server in &servers {
        assert!(!server.id.is_empty(), "server.id should not be empty");
        assert!(!server.name.is_empty(), "server.name should not be empty");
    }
}

/// Get a specific server by ID and verify it matches.
pub async fn get_server_by_id(backend: &PluginBackend, server_id: &str) {
    let server = backend
        .get_server(server_id)
        .await
        .expect("get_server() should succeed for valid ID");
    assert_eq!(
        server.id, server_id,
        "Returned server ID should match requested"
    );
}

/// Get a non-existent server and verify we get NotFound.
pub async fn get_server_not_found(backend: &PluginBackend) {
    let result = backend.get_server("nonexistent-server-99999").await;
    match result {
        Err(ClientError::NotFound(_)) => {} // Expected
        Err(other) => unreachable!("Expected NotFound, got: {other:?}"),
        Ok(s) => unreachable!("Expected NotFound error, got server: {}", s.name),
    }
}

// ─── Data Retrieval: Channels ──────────────────────────────────────

/// Get channels for a server and verify the response.
pub async fn get_channels(backend: &PluginBackend, server_id: &str) -> Vec<poly_client::Channel> {
    backend
        .get_channels(server_id)
        .await
        .expect("get_channels() should not error")
}

/// Get channels and verify we got at least `min_count`.
pub async fn get_channels_non_empty(backend: &PluginBackend, server_id: &str, min_count: usize) {
    let channels = get_channels(backend, server_id).await;
    assert!(
        channels.len() >= min_count,
        "Expected at least {min_count} channels for server '{server_id}', got {}",
        channels.len()
    );

    for channel in &channels {
        assert!(!channel.id.is_empty(), "channel.id should not be empty");
        assert!(!channel.name.is_empty(), "channel.name should not be empty");
    }
}

/// Get a specific channel by ID.
pub async fn get_channel_by_id(backend: &PluginBackend, channel_id: &str) {
    let channel = backend
        .get_channel(channel_id)
        .await
        .expect("get_channel() should succeed for valid ID");
    assert_eq!(channel.id, channel_id, "Returned channel ID should match");
}

/// Get a non-existent channel and verify NotFound.
pub async fn get_channel_not_found(backend: &PluginBackend) {
    let result = backend.get_channel("nonexistent-channel-99999").await;
    match result {
        Err(ClientError::NotFound(_)) => {} // Expected
        Err(other) => unreachable!("Expected NotFound, got: {other:?}"),
        Ok(c) => unreachable!("Expected NotFound error, got channel: {}", c.name),
    }
}

// ─── Data Retrieval: Messages ──────────────────────────────────────

/// Get messages for a channel with default query.
pub async fn get_messages(backend: &PluginBackend, channel_id: &str) -> Vec<poly_client::Message> {
    let query = MessageQuery {
        before: None,
        after: None,
        limit: Some(50),
    };
    backend
        .get_messages(channel_id, query)
        .await
        .expect("get_messages() should not error")
}

/// Get messages and verify non-empty with valid structure.
pub async fn get_messages_non_empty(backend: &PluginBackend, channel_id: &str, min_count: usize) {
    let messages = get_messages(backend, channel_id).await;
    assert!(
        messages.len() >= min_count,
        "Expected at least {min_count} messages in '{channel_id}', got {}",
        messages.len()
    );

    for msg in &messages {
        assert!(!msg.id.is_empty(), "message.id should not be empty");
        assert!(
            !msg.author.id.is_empty(),
            "message.author.id should not be empty"
        );
    }
}

/// Send a text message and verify the response.
pub async fn send_text_message(
    backend: &PluginBackend,
    channel_id: &str,
    text: &str,
) -> poly_client::Message {
    let content = MessageContent::Text(text.to_string());
    let msg = backend
        .send_message(channel_id, content)
        .await
        .expect("send_message() should succeed");

    assert!(!msg.id.is_empty(), "Sent message should have an ID");
    msg
}

// ─── Data Retrieval: Users ─────────────────────────────────────────

/// Get a user by ID.
pub async fn get_user(backend: &PluginBackend, user_id: &str) -> poly_client::User {
    backend
        .get_user(user_id)
        .await
        .expect("get_user() should succeed for valid ID")
}

/// Get friends list.
pub async fn get_friends(backend: &PluginBackend) -> Vec<poly_client::User> {
    backend
        .get_friends()
        .await
        .expect("get_friends() should not error")
}

/// Get channel members.
pub async fn get_channel_members(
    backend: &PluginBackend,
    channel_id: &str,
) -> Vec<poly_client::User> {
    backend
        .get_channel_members(channel_id)
        .await
        .expect("get_channel_members() should not error")
}

// ─── Data Retrieval: Groups ────────────────────────────────────────

/// Get groups list.
pub async fn get_groups(backend: &PluginBackend) -> Vec<poly_client::Group> {
    backend
        .get_groups()
        .await
        .expect("get_groups() should not error")
}

// ─── Data Retrieval: DMs ───────────────────────────────────────────

/// Get DM channels list.
pub async fn get_dm_channels(backend: &PluginBackend) -> Vec<poly_client::DmChannel> {
    backend
        .get_dm_channels()
        .await
        .expect("get_dm_channels() should not error")
}

// ─── Data Retrieval: Notifications ─────────────────────────────────

/// Get notifications list.
pub async fn get_notifications(backend: &PluginBackend) -> Vec<poly_client::Notification> {
    backend
        .get_notifications()
        .await
        .expect("get_notifications() should not error")
}

// ─── Voice ─────────────────────────────────────────────────────────

/// Get voice participants for a channel.
pub async fn get_voice_participants(
    backend: &PluginBackend,
    channel_id: &str,
) -> Vec<poly_client::VoiceParticipant> {
    backend
        .get_voice_participants(channel_id)
        .await
        .expect("get_voice_participants() should not error")
}

// ─── Presence ──────────────────────────────────────────────────────

/// Get a user's presence status.
pub async fn get_presence(backend: &PluginBackend, user_id: &str) -> PresenceStatus {
    backend
        .get_presence(user_id)
        .await
        .expect("get_presence() should not error")
}

/// Set presence status without error.
pub async fn set_presence(backend: &PluginBackend, status: PresenceStatus) {
    backend
        .set_presence(status)
        .await
        .expect("set_presence() should not error");
}

// ─── Events ────────────────────────────────────────────────────────

/// Verify the event stream can be created without panicking.
/// We don't consume events here — just verify the stream object is valid.
pub fn event_stream_is_valid(backend: &PluginBackend) {
    let _stream = backend.event_stream();
    // Stream created successfully — that's all we verify.
    // Consuming events would block in stubs that return None forever.
}

// ─── Stub Convenience: verify empty-list methods ───────────────────

/// For stub backends: verify all list-returning methods return empty lists.
pub async fn assert_stub_returns_empty_lists(backend: &PluginBackend) {
    let servers = backend.get_servers().await.expect("get_servers ok");
    assert!(servers.is_empty(), "Stub get_servers() should be empty");

    let friends = backend.get_friends().await.expect("get_friends ok");
    assert!(friends.is_empty(), "Stub get_friends() should be empty");

    let groups = backend.get_groups().await.expect("get_groups ok");
    assert!(groups.is_empty(), "Stub get_groups() should be empty");

    let dms = backend.get_dm_channels().await.expect("get_dm_channels ok");
    assert!(dms.is_empty(), "Stub get_dm_channels() should be empty");

    let notifs = backend
        .get_notifications()
        .await
        .expect("get_notifications ok");
    assert!(
        notifs.is_empty(),
        "Stub get_notifications() should be empty"
    );
}
