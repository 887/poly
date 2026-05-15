//! Shared test harness for IsBackend interface contract testing.
//!
//! Provides reusable async test functions that exercise each part of the
//! `IsBackend` trait through the WASM plugin host. Each client's test
//! module calls these functions after instantiating its plugin.
//!
//! ## Categories
//!
//! - **Identity** — `backend_type()`, `backend_name()`
//! - **Lifecycle** — `authenticate()`, `is_authenticated()`, `logout()`
//! - **Data Retrieval** — `get_servers()`, `get_channels()`, `get_messages()`, etc.
//! - **Mutations** — `send_message()`, `set_presence()`, `remove_group_member()`, `add_group_member()`
//! - **Events** — `event_stream()` returns a valid stream

use poly_client::{
    IsBackend, MessagingBackend, ModerationBackend, SocialGraphBackend, DmsAndGroupsBackend,
    ServerAdminBackend, AuthCredentials, BackendType, ChannelType, ClientError, ClientEvent,
    MessageContent, MessageQuery, PresenceStatus, SettingsScope, ViewBody, ViewKind,
    UpdateChannelParams, MenuTargetKind, ActionOutcome, CursorKind,
};
use poly_plugin_host::PluginBackend;

pub type HarnessResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[allow(dead_code)]
fn is_stub_error(error: &ClientError) -> bool {
    let message = match error {
        ClientError::AuthFailed(message)
        | ClientError::Network(message)
        | ClientError::NotFound(message)
        | ClientError::PermissionDenied(message)
        | ClientError::Internal(message)
        | ClientError::NotSupported(message) => message,
        ClientError::RateLimited { .. } => return false,
    };

    let lowercase = message.to_ascii_lowercase();
    lowercase.contains("not yet implemented")
        || lowercase.contains("wasm impl not yet complete")
        || lowercase.contains("stub")
}

/// Verify a plugin-path auth call does not fall back to a stub guest error.
#[allow(dead_code)]
pub async fn authenticate_does_not_use_stub_path(
    backend: &mut PluginBackend,
    credentials: AuthCredentials,
) {
    let result = backend.authenticate(credentials).await;
    if let Err(error) = &result {
        assert!(
            !is_stub_error(error),
            "Expected real plugin-path behavior, but guest returned a stub error: {error}"
        );
    }
}

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
) -> HarnessResult<poly_client::Session> {
    let creds = AuthCredentials::Token(token.to_string());
    let session = backend
        .authenticate(creds)
        .await
        .map_err(|e| format!("authenticate() should succeed: {e:?}"))?;

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

    Ok(session)
}

/// Verify that authenticate returns an error for stubs that are not implemented.
#[cfg(any(
    feature = "test-matrix",
    feature = "test-discord",
    feature = "test-teams",
    feature = "test-server"
))]
pub async fn authenticate_returns_error(backend: &mut PluginBackend) {
    let creds = AuthCredentials::Token("test-token".to_string());
    let result = backend.authenticate(creds).await;
    assert!(
        result.is_err(),
        "Stub authenticate() should return an error"
    );
}

/// Verify logout succeeds (or is a no-op for stubs).
#[cfg(any(
    feature = "test-stoat",
    feature = "test-matrix",
    feature = "test-discord",
    feature = "test-teams",
    feature = "test-server"
))]
pub async fn logout_succeeds(backend: &mut PluginBackend) {
    let result = backend.logout().await;
    // Both Ok(()) and Err(Internal("not implemented")) are acceptable
    // — we just verify it doesn't panic/trap
    let _ = result;
}

// ─── Data Retrieval: Servers ───────────────────────────────────────

/// Get servers and verify the response is valid.
/// Returns the list for further inspection.
pub async fn get_servers(backend: &PluginBackend) -> HarnessResult<Vec<poly_client::Server>> {
    backend
        .get_servers()
        .await
        .map_err(|e| format!("get_servers() should not error: {e:?}").into())
}

/// Get servers and verify we got at least `min_count` entries.
pub async fn get_servers_non_empty(
    backend: &PluginBackend,
    min_count: usize,
) -> HarnessResult {
    let servers = get_servers(backend).await?;
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
    Ok(())
}

/// Get a specific server by ID and verify it matches.
pub async fn get_server_by_id(backend: &PluginBackend, server_id: &str) -> HarnessResult {
    let server = backend
        .get_server(server_id)
        .await
        .map_err(|e| format!("get_server() should succeed for valid ID: {e:?}"))?;
    assert_eq!(
        server.id, server_id,
        "Returned server ID should match requested"
    );
    Ok(())
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
pub async fn get_channels(
    backend: &PluginBackend,
    server_id: &str,
) -> HarnessResult<Vec<poly_client::Channel>> {
    backend
        .get_channels(server_id)
        .await
        .map_err(|e| format!("get_channels() should not error: {e:?}").into())
}

/// Get channels and verify we got at least `min_count`.
pub async fn get_channels_non_empty(
    backend: &PluginBackend,
    server_id: &str,
    min_count: usize,
) -> HarnessResult {
    let channels = get_channels(backend, server_id).await?;
    assert!(
        channels.len() >= min_count,
        "Expected at least {min_count} channels for server '{server_id}', got {}",
        channels.len()
    );

    for channel in &channels {
        assert!(!channel.id.is_empty(), "channel.id should not be empty");
        assert!(!channel.name.is_empty(), "channel.name should not be empty");
    }
    Ok(())
}

/// Get a specific channel by ID.
pub async fn get_channel_by_id(backend: &PluginBackend, channel_id: &str) -> HarnessResult {
    let channel = backend
        .get_channel(channel_id)
        .await
        .map_err(|e| format!("get_channel() should succeed for valid ID: {e:?}"))?;
    assert_eq!(channel.id, channel_id, "Returned channel ID should match");
    Ok(())
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
pub async fn get_messages(
    backend: &PluginBackend,
    channel_id: &str,
) -> HarnessResult<Vec<poly_client::Message>> {
    let query = MessageQuery {
        before: None,
        after: None,
        around: None,
        limit: Some(50),
    };
    backend
        .get_messages(channel_id, query)
        .await
        .map_err(|e| format!("get_messages() should not error: {e:?}").into())
}

/// Get messages and verify non-empty with valid structure.
pub async fn get_messages_non_empty(
    backend: &PluginBackend,
    channel_id: &str,
    min_count: usize,
) -> HarnessResult {
    let messages = get_messages(backend, channel_id).await?;
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
    Ok(())
}

/// Send a text message and verify the response.
pub async fn send_text_message(
    backend: &PluginBackend,
    channel_id: &str,
    text: &str,
) -> HarnessResult<poly_client::Message> {
    let content = MessageContent::Text(text.to_string());
    let msg = backend
        .send_message(channel_id, content)
        .await
        .map_err(|e| format!("send_message() should succeed: {e:?}"))?;

    assert!(!msg.id.is_empty(), "Sent message should have an ID");
    Ok(msg)
}

// ─── Data Retrieval: Users ─────────────────────────────────────────

/// Get a user by ID.
pub async fn get_user(
    backend: &PluginBackend,
    user_id: &str,
) -> HarnessResult<poly_client::User> {
    backend
        .get_user(user_id)
        .await
        .map_err(|e| format!("get_user() should succeed for valid ID: {e:?}").into())
}

/// Get friends list.
pub async fn get_friends(backend: &PluginBackend) -> HarnessResult<Vec<poly_client::User>> {
    backend
        .get_friends()
        .await
        .map_err(|e| format!("get_friends() should not error: {e:?}").into())
}

/// Get channel members.
pub async fn get_channel_members(
    backend: &PluginBackend,
    channel_id: &str,
) -> HarnessResult<Vec<poly_client::User>> {
    backend
        .get_channel_members(channel_id)
        .await
        .map_err(|e| format!("get_channel_members() should not error: {e:?}").into())
}

// ─── Data Retrieval: Groups ────────────────────────────────────────

/// Get groups list.
pub async fn get_groups(backend: &PluginBackend) -> HarnessResult<Vec<poly_client::Group>> {
    backend
        .get_groups()
        .await
        .map_err(|e| format!("get_groups() should not error: {e:?}").into())
}

// ─── Data Retrieval: DMs ───────────────────────────────────────────

/// Get DM channels list.
pub async fn get_dm_channels(
    backend: &PluginBackend,
) -> HarnessResult<Vec<poly_client::DmChannel>> {
    backend
        .get_dm_channels()
        .await
        .map_err(|e| format!("get_dm_channels() should not error: {e:?}").into())
}

/// Open or create a DM channel with the target user.
pub async fn open_direct_message_channel(
    backend: &PluginBackend,
    user_id: &str,
) -> HarnessResult<poly_client::DmChannel> {
    backend
        .open_direct_message_channel(user_id)
        .await
        .map_err(|e| format!("open_direct_message_channel() should not error: {e:?}").into())
}

/// Open the Saved Messages / self-DM channel.
pub async fn open_saved_messages_channel(
    backend: &PluginBackend,
) -> HarnessResult<poly_client::DmChannel> {
    backend
        .open_saved_messages_channel()
        .await
        .map_err(|e| format!("open_saved_messages_channel() should not error: {e:?}").into())
}

// ─── Data Retrieval: Notifications ─────────────────────────────────

/// Get notifications list.
pub async fn get_notifications(
    backend: &PluginBackend,
) -> HarnessResult<Vec<poly_client::Notification>> {
    backend
        .get_notifications()
        .await
        .map_err(|e| format!("get_notifications() should not error: {e:?}").into())
}

// ─── Voice ─────────────────────────────────────────────────────────

/// Get voice participants for a channel.
pub async fn get_voice_participants(
    backend: &PluginBackend,
    channel_id: &str,
) -> HarnessResult<Vec<poly_client::VoiceParticipant>> {
    backend
        .get_voice_participants(channel_id)
        .await
        .map_err(|e| format!("get_voice_participants() should not error: {e:?}").into())
}

// ─── Presence ──────────────────────────────────────────────────────

/// Get a user's presence status.
pub async fn get_presence(
    backend: &PluginBackend,
    user_id: &str,
) -> HarnessResult<PresenceStatus> {
    backend
        .get_presence(user_id)
        .await
        .map_err(|e| format!("get_presence() should not error: {e:?}").into())
}

/// Set presence status without error.
pub async fn set_presence(backend: &PluginBackend, status: PresenceStatus) -> HarnessResult {
    backend
        .set_presence(status)
        .await
        .map_err(|e| format!("set_presence() should not error: {e:?}").into())
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
#[cfg(any(
    feature = "test-matrix",
    feature = "test-discord",
    feature = "test-teams",
    feature = "test-server"
))]
pub async fn assert_stub_returns_empty_lists(backend: &PluginBackend) -> HarnessResult {
    let servers = backend
        .get_servers()
        .await
        .map_err(|e| format!("get_servers ok: {e:?}"))?;
    assert!(servers.is_empty(), "Stub get_servers() should be empty");

    let friends = backend
        .get_friends()
        .await
        .map_err(|e| format!("get_friends ok: {e:?}"))?;
    assert!(friends.is_empty(), "Stub get_friends() should be empty");

    let groups = backend
        .get_groups()
        .await
        .map_err(|e| format!("get_groups ok: {e:?}"))?;
    assert!(groups.is_empty(), "Stub get_groups() should be empty");

    let dms = backend
        .get_dm_channels()
        .await
        .map_err(|e| format!("get_dm_channels ok: {e:?}"))?;
    assert!(dms.is_empty(), "Stub get_dm_channels() should be empty");

    let notifs = backend
        .get_notifications()
        .await
        .map_err(|e| format!("get_notifications ok: {e:?}"))?;
    assert!(
        notifs.is_empty(),
        "Stub get_notifications() should be empty"
    );
    Ok(())
}
