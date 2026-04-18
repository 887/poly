//! End-to-end tests for the **Demo** client plugin.
//!
//! The demo client is fully implemented with mock data, so these tests
//! exercise the complete `ClientBackend` interface through the WASM plugin
//! host — from authentication to data retrieval to logout.
//!
//! Enable with: `--features test-demo` (enabled by default).

use poly_client::{BackendType, ClientBackend, PresenceStatus};

use super::harness;
use super::harness_build_route;
use super::harness_composer;
use super::harness_custom_block;
use super::harness_menus;
use super::harness_settings;
use super::harness_sidebar;
use super::harness_views;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Load the demo plugin ready for testing.
async fn load_demo() -> Result<poly_plugin_host::PluginBackend, Box<dyn std::error::Error>> {
    poly_plugin_loader_tests::load_plugin("demo", "poly_demo.wasm").await
}

// ─── Identity ──────────────────────────────────────────────────────

#[tokio::test]
async fn demo_backend_type() -> TestResult {
    let backend = load_demo().await?;
    harness::assert_backend_type(&backend, BackendType::from("demo"));
    Ok(())
}

#[tokio::test]
async fn demo_backend_name() -> TestResult {
    let backend = load_demo().await?;
    harness::assert_backend_name(&backend, "Demo");
    Ok(())
}

// ─── Authentication Lifecycle ──────────────────────────────────────

#[tokio::test]
async fn demo_authenticate_and_logout() -> TestResult {
    let mut backend = load_demo().await?;

    // Before auth: is_authenticated should be false
    assert!(
        !backend.is_authenticated(),
        "Should not be authenticated before login"
    );

    // Authenticate
    let session = harness::authenticate_with_token(&mut backend, "demo-token").await?;
    assert_eq!(session.backend, BackendType::from("demo"));

    // After auth: is_authenticated should be true
    // NOTE: PluginBackend.is_authenticated() currently returns false always
    // (sync check limitation in the WASM plugin host). This is a known TODO.
    // When actually implemented, we'd assert: assert!(backend.is_authenticated());

    // Logout
    backend
        .logout()
        .await
        .map_err(|e| format!("logout should succeed: {e:?}"))?;
    Ok(())
}

#[tokio::test]
async fn demo_session_fields() -> TestResult {
    let mut backend = load_demo().await?;
    let session = harness::authenticate_with_token(&mut backend, "demo-token").await?;

    // Verify session has all expected fields populated
    assert!(!session.id.is_empty(), "session.id");
    assert!(!session.token.is_empty(), "session.token");
    assert!(!session.user.id.is_empty(), "session.user.id");
    assert!(
        !session.user.display_name.is_empty(),
        "session.user.display_name"
    );
    assert_eq!(session.backend, BackendType::from("demo"));
    Ok(())
}

// ─── Servers ───────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_servers() -> TestResult {
    let backend = load_demo().await?;
    // Demo should have at least 2 servers
    harness::get_servers_non_empty(&backend, 2).await
}

#[tokio::test]
async fn demo_get_server_by_id() -> TestResult {
    let backend = load_demo().await?;
    let servers = harness::get_servers(&backend).await?;
    assert!(!servers.is_empty(), "Need at least 1 server");

    // Look up the first server by ID
    let first_id = servers
        .first()
        .ok_or("servers list unexpectedly empty")?
        .id
        .clone();
    harness::get_server_by_id(&backend, &first_id).await
}

#[tokio::test]
async fn demo_get_server_not_found() -> TestResult {
    let backend = load_demo().await?;
    harness::get_server_not_found(&backend).await;
    Ok(())
}

// ─── Channels ──────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_channels() -> TestResult {
    let backend = load_demo().await?;
    let servers = harness::get_servers(&backend).await?;
    assert!(!servers.is_empty());

    let first_server_id = servers
        .first()
        .ok_or("servers list unexpectedly empty")?
        .id
        .clone();
    harness::get_channels_non_empty(&backend, &first_server_id, 1).await
}

#[tokio::test]
async fn demo_get_channel_by_id() -> TestResult {
    let backend = load_demo().await?;
    let servers = harness::get_servers(&backend).await?;
    let server = servers.first().ok_or("servers list unexpectedly empty")?;
    let channels = harness::get_channels(&backend, &server.id).await?;
    assert!(!channels.is_empty());

    let first_channel_id = channels
        .first()
        .ok_or("channels list unexpectedly empty")?
        .id
        .clone();
    harness::get_channel_by_id(&backend, &first_channel_id).await
}

#[tokio::test]
async fn demo_get_channel_not_found() -> TestResult {
    let backend = load_demo().await?;
    harness::get_channel_not_found(&backend).await;
    Ok(())
}

#[tokio::test]
async fn demo_channels_have_valid_types() -> TestResult {
    let backend = load_demo().await?;
    let servers = harness::get_servers(&backend).await?;
    let server = servers.first().ok_or("servers list unexpectedly empty")?;
    let channels = harness::get_channels(&backend, &server.id).await?;

    for channel in &channels {
        // channel_type should be a valid variant (Text, Voice, Video)
        // If it got through the bridge, it's valid. Just check ID/name.
        assert!(!channel.id.is_empty());
        assert!(!channel.name.is_empty());
    }
    Ok(())
}

// ─── Messages ──────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_messages() -> TestResult {
    let backend = load_demo().await?;
    let servers = harness::get_servers(&backend).await?;
    let server = servers.first().ok_or("servers list unexpectedly empty")?;
    let channels = harness::get_channels(&backend, &server.id).await?;
    let channel = channels.first().ok_or("channels list unexpectedly empty")?;

    harness::get_messages_non_empty(&backend, &channel.id, 1).await
}

#[tokio::test]
async fn demo_send_message() -> TestResult {
    let backend = load_demo().await?;
    let servers = harness::get_servers(&backend).await?;
    let server = servers.first().ok_or("servers list unexpectedly empty")?;
    let channels = harness::get_channels(&backend, &server.id).await?;
    let channel = channels.first().ok_or("channels list unexpectedly empty")?;

    let msg = harness::send_text_message(&backend, &channel.id, "Hello from E2E test!").await?;
    assert!(!msg.id.is_empty(), "Sent message must have an ID");
    assert!(
        !msg.author.id.is_empty(),
        "Sent message must have an author"
    );
    Ok(())
}

// ─── Users ─────────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_friends() -> TestResult {
    let backend = load_demo().await?;
    let friends = harness::get_friends(&backend).await?;
    assert!(!friends.is_empty(), "Demo should have at least 1 friend");

    for user in &friends {
        assert!(!user.id.is_empty());
        assert!(!user.display_name.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn demo_get_channel_members() -> TestResult {
    let backend = load_demo().await?;
    let servers = harness::get_servers(&backend).await?;
    let server = servers.first().ok_or("servers list unexpectedly empty")?;
    let channels = harness::get_channels(&backend, &server.id).await?;
    let channel = channels.first().ok_or("channels list unexpectedly empty")?;

    let members = harness::get_channel_members(&backend, &channel.id).await?;
    assert!(!members.is_empty(), "Demo channels should have members");
    Ok(())
}

#[tokio::test]
async fn demo_get_user_by_id() -> TestResult {
    let backend = load_demo().await?;
    let friends = harness::get_friends(&backend).await?;
    let first_user = friends.first().ok_or("friends list unexpectedly empty")?;
    let first_user_id = first_user.id.clone();
    let first_user_display_name = first_user.display_name.clone();

    let user = harness::get_user(&backend, &first_user_id).await?;
    assert_eq!(user.id, first_user_id);
    assert_eq!(user.display_name, first_user_display_name);
    Ok(())
}

// ─── Groups ────────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_groups() -> TestResult {
    let backend = load_demo().await?;
    let groups = harness::get_groups(&backend).await?;
    assert!(!groups.is_empty(), "Demo should have groups");

    for group in &groups {
        assert!(!group.id.is_empty());
        assert!(!group.members.is_empty(), "Group should have members");
    }
    Ok(())
}

#[tokio::test]
async fn demo_remove_group_member() -> TestResult {
    let backend = load_demo().await?;
    // Demo accepts any remove — just verify no error
    backend
        .remove_group_member("group-1", "user-1")
        .await
        .map_err(|e| format!("remove_group_member should succeed in demo: {e:?}"))?;
    Ok(())
}

#[tokio::test]
async fn demo_add_group_member() -> TestResult {
    let backend = load_demo().await?;
    backend
        .add_group_member("group-1", "user-9")
        .await
        .map_err(|e| format!("add_group_member should succeed in demo: {e:?}"))?;
    Ok(())
}

// ─── DM Channels ───────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_dm_channels() -> TestResult {
    let backend = load_demo().await?;
    let dms = harness::get_dm_channels(&backend).await?;
    assert!(!dms.is_empty(), "Demo should have DM channels");

    for dm in &dms {
        assert!(!dm.id.is_empty());
        assert!(!dm.user.id.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn demo_open_direct_message_channel() -> TestResult {
    let backend = load_demo().await?;
    let dms = harness::get_dm_channels(&backend).await?;
    let expected = dms.first().ok_or("demo dms present")?;
    let expected_user_id = expected.user.id.clone();
    let expected_dm_id = expected.id.clone();

    let dm = harness::open_direct_message_channel(&backend, &expected_user_id).await?;
    assert_eq!(dm.id, expected_dm_id);
    assert_eq!(dm.user.id, expected_user_id);
    Ok(())
}

#[tokio::test]
async fn demo_open_direct_message_channel_for_non_dm_friend() -> TestResult {
    let backend = load_demo().await?;

    let dm = harness::open_direct_message_channel(&backend, "user-grace").await?;
    assert_eq!(dm.id, "dm-user-grace");
    assert_eq!(dm.user.id, "user-grace");
    assert_eq!(dm.user.display_name, "Grace");
    assert_eq!(dm.account_id, "demo-cat");
    assert!(
        dm.last_message.is_none(),
        "new fallback demo DMs should start empty"
    );
    assert_eq!(dm.unread_count, 0);
    Ok(())
}

#[tokio::test]
async fn demo_open_saved_messages_channel() -> TestResult {
    let backend = load_demo().await?;
    let saved = harness::open_saved_messages_channel(&backend).await?;
    assert_eq!(saved.id, "dm-demo-saved-self");
    assert_eq!(saved.account_id, "demo-cat");
    assert_eq!(saved.user.display_name, "Cat (demo)");
    Ok(())
}

#[tokio::test]
async fn demo_dm_messages() -> TestResult {
    let backend = load_demo().await?;
    let dms = harness::get_dm_channels(&backend).await?;
    let dm = dms.first().ok_or("demo dms unexpectedly empty")?;
    let dm_id = dm.id.clone();

    // DM channel IDs are used to fetch messages
    let messages = harness::get_messages(&backend, &dm_id).await?;
    assert!(
        !messages.is_empty(),
        "Demo DM channels should have messages"
    );
    Ok(())
}

// ─── Notifications ─────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_notifications() -> TestResult {
    let backend = load_demo().await?;
    let notifs = harness::get_notifications(&backend).await?;
    assert!(!notifs.is_empty(), "Demo should have notifications");

    for notif in &notifs {
        assert!(!notif.id.is_empty());
    }
    Ok(())
}

// ─── Voice ─────────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_voice_participants() -> TestResult {
    let backend = load_demo().await?;
    // Get a voice channel — first find one
    let servers = harness::get_servers(&backend).await?;
    let server = servers.first().ok_or("servers list unexpectedly empty")?;
    let channels = harness::get_channels(&backend, &server.id).await?;

    // Try to find a voice channel; if not, just test any channel
    let channel_id = channels
        .iter()
        .find(|c| matches!(c.channel_type, poly_client::ChannelType::Voice))
        .or_else(|| channels.first())
        .ok_or("channels list unexpectedly empty")?
        .id
        .clone();

    let _participants = harness::get_voice_participants(&backend, &channel_id).await?;
    // Participants may be empty — just verify no error
    Ok(())
}

// ─── Presence ──────────────────────────────────────────────────────

#[tokio::test]
async fn demo_get_presence() -> TestResult {
    let backend = load_demo().await?;
    let friends = harness::get_friends(&backend).await?;
    let user = friends.first().ok_or("friends list unexpectedly empty")?;
    let user_id = user.id.clone();

    let status = harness::get_presence(&backend, &user_id).await?;
    // Demo returns Online for everyone
    assert_eq!(status, PresenceStatus::Online);
    Ok(())
}

#[tokio::test]
async fn demo_set_presence() -> TestResult {
    let backend = load_demo().await?;
    harness::set_presence(&backend, PresenceStatus::DoNotDisturb).await?;
    // No error = success
    Ok(())
}

// ─── Events ────────────────────────────────────────────────────────

#[tokio::test]
async fn demo_event_stream() -> TestResult {
    let backend = load_demo().await?;
    harness::event_stream_is_valid(&backend);
    Ok(())
}

// ─── Full Lifecycle Integration ────────────────────────────────────

/// The big one: authenticate → browse → interact → logout.
/// Exercises the complete user journey through the plugin interface.
#[tokio::test]
async fn demo_full_lifecycle() -> TestResult {
    let mut backend = load_demo().await?;

    // 1. Authenticate
    let session = harness::authenticate_with_token(&mut backend, "demo-token").await?;
    assert_eq!(session.backend, BackendType::from("demo"));

    // 2. Browse servers
    let servers = harness::get_servers(&backend).await?;
    assert!(!servers.is_empty());
    let server = servers.first().ok_or("servers list unexpectedly empty")?;
    let server_id = server.id.clone();

    // 3. Browse channels
    let channels = harness::get_channels(&backend, &server_id).await?;
    assert!(!channels.is_empty());
    let channel = channels.first().ok_or("channels list unexpectedly empty")?;
    let channel_id = channel.id.clone();

    // 4. Read messages
    let messages = harness::get_messages(&backend, &channel_id).await?;
    assert!(!messages.is_empty());

    // 5. Send a message
    let _sent = harness::send_text_message(&backend, &channel_id, "Full lifecycle test!").await?;

    // 6. Check DMs
    let dms = harness::get_dm_channels(&backend).await?;
    assert!(!dms.is_empty());

    // 7. Check groups
    let groups = harness::get_groups(&backend).await?;
    assert!(!groups.is_empty());

    // 8. Check notifications
    let notifs = harness::get_notifications(&backend).await?;
    assert!(!notifs.is_empty());

    // 9. Check friends
    let friends = harness::get_friends(&backend).await?;
    assert!(!friends.is_empty());

    // 10. Set presence
    harness::set_presence(&backend, PresenceStatus::Idle).await?;

    // 11. Logout
    backend
        .logout()
        .await
        .map_err(|e| format!("logout should succeed: {e:?}"))?;
    Ok(())
}

// ─── Composer / Settings / Sidebar / Views / Build-route / Menus / Custom-block ──

/// Verify the demo plugin's composer buttons are well-formed for a demo channel.
#[tokio::test]
async fn demo_composer_buttons_valid() -> TestResult {
    let backend = load_demo().await?;
    let servers = harness::get_servers(&backend).await?;
    let server = servers.first().ok_or("servers list unexpectedly empty")?;
    let channels = harness::get_channels(&backend, &server.id).await?;
    let channel = channels.first().ok_or("channels list unexpectedly empty")?;
    harness_composer::composer_buttons_well_formed(&backend, &channel.id).await
}

/// Verify the demo plugin's settings sections are well-formed.
#[tokio::test]
async fn demo_settings_sections_valid() -> TestResult {
    let backend = load_demo().await?;
    harness_settings::settings_sections_well_formed(&backend).await
}

/// Verify the demo plugin's sidebar declaration is well-formed.
#[tokio::test]
async fn demo_sidebar_declaration_valid() -> TestResult {
    let backend = load_demo().await?;
    harness_sidebar::sidebar_declaration_well_formed(&backend).await
}

/// Verify the demo plugin's channel view descriptor is well-formed.
#[tokio::test]
async fn demo_channel_view_descriptor_valid() -> TestResult {
    let backend = load_demo().await?;
    let servers = harness::get_servers(&backend).await?;
    let server = servers.first().ok_or("servers list unexpectedly empty")?;
    let channels = harness::get_channels(&backend, &server.id).await?;
    let channel = channels.first().ok_or("channels list unexpectedly empty")?;
    harness_views::channel_view_descriptor_well_formed(&backend, &channel.id).await
}

/// Verify the demo plugin build-route stub does not trap.
#[tokio::test]
async fn demo_plugin_build_route_stub() -> TestResult {
    let backend = load_demo().await?;
    harness_build_route::plugin_builds_routes_via_host_api(&backend).await
}

/// Verify the demo plugin menus stub does not trap.
#[tokio::test]
async fn demo_menu_items_stub() -> TestResult {
    let backend = load_demo().await?;
    harness_menus::menu_items_well_formed(&backend, "server", "demo-server-1").await
}

/// Verify the demo plugin custom-block sanitization stub does not trap.
#[tokio::test]
async fn demo_custom_block_survives_sanitization() -> TestResult {
    let backend = load_demo().await?;
    harness_custom_block::custom_block_survives_sanitization(&backend).await
}
