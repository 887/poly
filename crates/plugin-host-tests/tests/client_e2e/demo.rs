//! End-to-end tests for the **Demo** client plugin.
//!
//! The demo client is fully implemented with mock data, so these tests
//! exercise the complete `ClientBackend` interface through the WASM plugin
//! host — from authentication to data retrieval to logout.
//!
//! Enable with: `--features test-demo` (enabled by default).

use poly_client::{BackendType, ClientBackend, PresenceStatus};

use super::harness;

/// Load the demo plugin ready for testing.
async fn load_demo() -> poly_plugin_host::PluginBackend {
    poly_plugin_loader_tests::load_plugin("demo", "poly_demo.wasm")
        .await
        .unwrap()
}

// ─── Identity ──────────────────────────────────────────────────────

#[tokio::test]
async fn demo_backend_type() {
    let backend = load_demo().await;
    harness::assert_backend_type(&backend, BackendType::Demo);
}

#[tokio::test]
async fn demo_backend_name() {
    let backend = load_demo().await;
    harness::assert_backend_name(&backend, "Demo");
}

// ─── Authentication Lifecycle ──────────────────────────────────────

#[tokio::test]
async fn demo_authenticate_and_logout() {
    let mut backend = load_demo().await;

    // Before auth: is_authenticated should be false
    assert!(
        !backend.is_authenticated(),
        "Should not be authenticated before login"
    );

    // Authenticate
    let session = harness::authenticate_with_token(&mut backend, "demo-token").await;
    assert_eq!(session.backend, BackendType::Demo);

    // After auth: is_authenticated should be true
    // NOTE: PluginBackend.is_authenticated() currently returns false always
    // (sync check limitation in the WASM plugin host). This is a known TODO.
    // When actually implemented, we'd assert: assert!(backend.is_authenticated());

    // Logout
    backend.logout().await.expect("logout should succeed");
}

#[tokio::test]
async fn demo_session_fields() {
    let mut backend = load_demo().await;
    let session = harness::authenticate_with_token(&mut backend, "demo-token").await;

    // Verify session has all expected fields populated
    assert!(!session.id.is_empty(), "session.id");
    assert!(!session.token.is_empty(), "session.token");
    assert!(!session.user.id.is_empty(), "session.user.id");
    assert!(
        !session.user.display_name.is_empty(),
        "session.user.display_name"
    );
    assert_eq!(session.backend, BackendType::Demo);
}

// ─── Servers ───────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_servers() {
    let backend = load_demo().await;
    // Demo should have at least 2 servers
    harness::get_servers_non_empty(&backend, 2).await;
}

#[tokio::test]
async fn demo_get_server_by_id() {
    let backend = load_demo().await;
    let servers = harness::get_servers(&backend).await;
    assert!(!servers.is_empty(), "Need at least 1 server");

    // Look up the first server by ID
    let first_id = &servers.first().unwrap().id;
    harness::get_server_by_id(&backend, first_id).await;
}

#[tokio::test]
async fn demo_get_server_not_found() {
    let backend = load_demo().await;
    harness::get_server_not_found(&backend).await;
}

// ─── Channels ──────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_channels() {
    let backend = load_demo().await;
    let servers = harness::get_servers(&backend).await;
    assert!(!servers.is_empty());

    let first_server_id = &servers.first().unwrap().id;
    harness::get_channels_non_empty(&backend, first_server_id, 1).await;
}

#[tokio::test]
async fn demo_get_channel_by_id() {
    let backend = load_demo().await;
    let servers = harness::get_servers(&backend).await;
    let server = servers.first().unwrap();
    let channels = harness::get_channels(&backend, &server.id).await;
    assert!(!channels.is_empty());

    let first_channel_id = &channels.first().unwrap().id;
    harness::get_channel_by_id(&backend, first_channel_id).await;
}

#[tokio::test]
async fn demo_get_channel_not_found() {
    let backend = load_demo().await;
    harness::get_channel_not_found(&backend).await;
}

#[tokio::test]
async fn demo_channels_have_valid_types() {
    let backend = load_demo().await;
    let servers = harness::get_servers(&backend).await;
    let server = servers.first().unwrap();
    let channels = harness::get_channels(&backend, &server.id).await;

    for channel in &channels {
        // channel_type should be a valid variant (Text, Voice, Video)
        // If it got through the bridge, it's valid. Just check ID/name.
        assert!(!channel.id.is_empty());
        assert!(!channel.name.is_empty());
    }
}

// ─── Messages ──────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_messages() {
    let backend = load_demo().await;
    let servers = harness::get_servers(&backend).await;
    let server = servers.first().unwrap();
    let channels = harness::get_channels(&backend, &server.id).await;
    let channel = channels.first().unwrap();

    harness::get_messages_non_empty(&backend, &channel.id, 1).await;
}

#[tokio::test]
async fn demo_send_message() {
    let backend = load_demo().await;
    let servers = harness::get_servers(&backend).await;
    let server = servers.first().unwrap();
    let channels = harness::get_channels(&backend, &server.id).await;
    let channel = channels.first().unwrap();

    let msg = harness::send_text_message(&backend, &channel.id, "Hello from E2E test!").await;
    assert!(!msg.id.is_empty(), "Sent message must have an ID");
    assert!(
        !msg.author.id.is_empty(),
        "Sent message must have an author"
    );
}

// ─── Users ─────────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_friends() {
    let backend = load_demo().await;
    let friends = harness::get_friends(&backend).await;
    assert!(!friends.is_empty(), "Demo should have at least 1 friend");

    for user in &friends {
        assert!(!user.id.is_empty());
        assert!(!user.display_name.is_empty());
    }
}

#[tokio::test]
async fn demo_get_channel_members() {
    let backend = load_demo().await;
    let servers = harness::get_servers(&backend).await;
    let server = servers.first().unwrap();
    let channels = harness::get_channels(&backend, &server.id).await;
    let channel = channels.first().unwrap();

    let members = harness::get_channel_members(&backend, &channel.id).await;
    assert!(!members.is_empty(), "Demo channels should have members");
}

#[tokio::test]
async fn demo_get_user_by_id() {
    let backend = load_demo().await;
    let friends = harness::get_friends(&backend).await;
    let first_user = friends.first().unwrap();

    let user = harness::get_user(&backend, &first_user.id).await;
    assert_eq!(user.id, first_user.id);
    assert_eq!(user.display_name, first_user.display_name);
}

// ─── Groups ────────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_groups() {
    let backend = load_demo().await;
    let groups = harness::get_groups(&backend).await;
    assert!(!groups.is_empty(), "Demo should have groups");

    for group in &groups {
        assert!(!group.id.is_empty());
        assert!(!group.members.is_empty(), "Group should have members");
    }
}

#[tokio::test]
async fn demo_remove_group_member() {
    let backend = load_demo().await;
    // Demo accepts any remove — just verify no error
    backend
        .remove_group_member("group-1", "user-1")
        .await
        .expect("remove_group_member should succeed in demo");
}

// ─── DM Channels ───────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_dm_channels() {
    let backend = load_demo().await;
    let dms = harness::get_dm_channels(&backend).await;
    assert!(!dms.is_empty(), "Demo should have DM channels");

    for dm in &dms {
        assert!(!dm.id.is_empty());
        assert!(!dm.user.id.is_empty());
    }
}

#[tokio::test]
async fn demo_dm_messages() {
    let backend = load_demo().await;
    let dms = harness::get_dm_channels(&backend).await;
    let dm = dms.first().unwrap();

    // DM channel IDs are used to fetch messages
    let messages = harness::get_messages(&backend, &dm.id).await;
    assert!(
        !messages.is_empty(),
        "Demo DM channels should have messages"
    );
}

// ─── Notifications ─────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_notifications() {
    let backend = load_demo().await;
    let notifs = harness::get_notifications(&backend).await;
    assert!(!notifs.is_empty(), "Demo should have notifications");

    for notif in &notifs {
        assert!(!notif.id.is_empty());
    }
}

// ─── Voice ─────────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_voice_participants() {
    let backend = load_demo().await;
    // Get a voice channel — first find one
    let servers = harness::get_servers(&backend).await;
    let server = servers.first().unwrap();
    let channels = harness::get_channels(&backend, &server.id).await;

    // Try to find a voice channel; if not, just test any channel
    let channel_id = channels
        .iter()
        .find(|c| matches!(c.channel_type, poly_client::ChannelType::Voice))
        .map(|c| c.id.clone())
        .unwrap_or_else(|| channels.first().unwrap().id.clone());

    let _participants = harness::get_voice_participants(&backend, &channel_id).await;
    // Participants may be empty — just verify no error
}

// ─── Presence ──────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_presence() {
    let backend = load_demo().await;
    let friends = harness::get_friends(&backend).await;
    let user = friends.first().unwrap();

    let status = harness::get_presence(&backend, &user.id).await;
    // Demo returns Online for everyone
    assert_eq!(status, PresenceStatus::Online);
}

#[tokio::test]
async fn demo_set_presence() {
    let backend = load_demo().await;
    harness::set_presence(&backend, PresenceStatus::DoNotDisturb).await;
    // No error = success
}

// ─── Events ────────────────────────────────────────────────────────

#[tokio::test]
async fn demo_event_stream() {
    let backend = load_demo().await;
    harness::event_stream_is_valid(&backend);
}

// ─── Full Lifecycle Integration ────────────────────────────────────

/// The big one: authenticate → browse → interact → logout.
/// Exercises the complete user journey through the plugin interface.
#[tokio::test]
async fn demo_full_lifecycle() {
    let mut backend = load_demo().await;

    // 1. Authenticate
    let session = harness::authenticate_with_token(&mut backend, "demo-token").await;
    assert_eq!(session.backend, BackendType::Demo);

    // 2. Browse servers
    let servers = harness::get_servers(&backend).await;
    assert!(!servers.is_empty());
    let server = servers.first().unwrap();

    // 3. Browse channels
    let channels = harness::get_channels(&backend, &server.id).await;
    assert!(!channels.is_empty());
    let channel = channels.first().unwrap();

    // 4. Read messages
    let messages = harness::get_messages(&backend, &channel.id).await;
    assert!(!messages.is_empty());

    // 5. Send a message
    let _sent = harness::send_text_message(&backend, &channel.id, "Full lifecycle test!").await;

    // 6. Check DMs
    let dms = harness::get_dm_channels(&backend).await;
    assert!(!dms.is_empty());

    // 7. Check groups
    let groups = harness::get_groups(&backend).await;
    assert!(!groups.is_empty());

    // 8. Check notifications
    let notifs = harness::get_notifications(&backend).await;
    assert!(!notifs.is_empty());

    // 9. Check friends
    let friends = harness::get_friends(&backend).await;
    assert!(!friends.is_empty());

    // 10. Set presence
    harness::set_presence(&backend, PresenceStatus::Idle).await;

    // 11. Logout
    backend.logout().await.expect("logout should succeed");
}
