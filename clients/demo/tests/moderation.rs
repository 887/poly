#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Behavioural coverage for the demo `ModerationBackend` /
//! `WritableModerationBackend` impls.
//!
//! Every test uses its own synthetic `server_id` (the demo moderation store is
//! process-global, like the sent-message store) so the cases stay independent
//! under cargo's parallel test threads.

use poly_client::{IsBackend, UpdateChannelParams};
use poly_demo::DemoClient;

#[tokio::test]
async fn bans_are_seeded_so_the_bans_tab_is_not_empty() {
    let client = DemoClient::new();
    let backend = client.as_moderation().expect("moderation accessor");
    let bans = backend.get_bans("srv-seed").await.expect("get_bans");
    assert_eq!(bans.len(), 2, "expected the two seeded fixture bans");
    assert!(
        bans.iter().any(|b| b.expires_at.is_none()),
        "one seeded ban should be permanent"
    );
    assert!(
        bans.iter().any(|b| b.expires_at.is_some()),
        "one seeded ban should be an active timeout"
    );
}

#[tokio::test]
async fn ban_then_unban_round_trips() {
    let client = DemoClient::new();
    let backend = client.as_moderation().expect("moderation accessor");
    let server = "srv-ban-round-trip";

    backend
        .ban_member(server, "user-zed", Some("testing"), None)
        .await
        .expect("ban_member");
    let bans = backend.get_bans(server).await.expect("get_bans");
    let banned = bans
        .iter()
        .find(|b| b.user_id == "user-zed")
        .expect("new ban present");
    assert_eq!(banned.reason.as_deref(), Some("testing"));
    assert!(banned.expires_at.is_none(), "plain ban is permanent");

    backend
        .unban_member(server, "user-zed")
        .await
        .expect("unban_member");
    let bans = backend.get_bans(server).await.expect("get_bans");
    assert!(
        !bans.iter().any(|b| b.user_id == "user-zed"),
        "ban should be lifted"
    );
}

#[tokio::test]
async fn unbanning_someone_who_is_not_banned_is_not_found() {
    let client = DemoClient::new();
    let backend = client.as_moderation().expect("moderation accessor");
    let err = backend
        .unban_member("srv-unban-miss", "user-nobody")
        .await
        .expect_err("unbanning a non-banned member must fail");
    assert!(
        matches!(err, poly_client::ClientError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}

#[tokio::test]
async fn timeout_then_untimeout_round_trips() {
    let client = DemoClient::new();
    let backend = client.as_moderation().expect("moderation accessor");
    let server = "srv-timeout";
    let until = chrono::Utc::now() + chrono::Duration::hours(6);

    backend
        .timeout_member(server, "user-zed", until, Some("cooling off"))
        .await
        .expect("timeout_member");
    let bans = backend.get_bans(server).await.expect("get_bans");
    let timed = bans
        .iter()
        .find(|b| b.user_id == "user-zed")
        .expect("timeout present");
    assert!(
        timed.expires_at.is_some(),
        "a timeout must carry an expiry so the UI can render it as temporary"
    );

    backend
        .untimeout_member(server, "user-zed")
        .await
        .expect("untimeout_member");
    assert!(
        !backend
            .get_bans(server)
            .await
            .expect("get_bans")
            .iter()
            .any(|b| b.user_id == "user-zed"),
        "timeout should be cleared"
    );

    let err = backend
        .untimeout_member(server, "user-zed")
        .await
        .expect_err("second untimeout must fail");
    assert!(matches!(err, poly_client::ClientError::NotFound(_)));
}

#[tokio::test]
async fn moderation_log_records_actions_newest_first() {
    let client = DemoClient::new();
    let backend = client.as_moderation().expect("moderation accessor");
    let server = "srv-modlog";

    backend
        .kick_member(server, "user-kicked", Some("spam"))
        .await
        .expect("kick_member");
    let log = backend
        .get_moderation_log(server, 10)
        .await
        .expect("get_moderation_log");
    assert!(!log.is_empty(), "log must not be empty");
    let newest = log.first().expect("non-empty log");
    assert_eq!(newest.action, poly_client::ModerationAction::MemberKicked);
    assert_eq!(newest.target_user_id.as_deref(), Some("user-kicked"));
    assert_eq!(newest.reason.as_deref(), Some("spam"));

    // `limit` is honoured.
    let one = backend
        .get_moderation_log(server, 1)
        .await
        .expect("get_moderation_log");
    assert_eq!(one.len(), 1);
}

#[tokio::test]
async fn roles_and_permissions_are_populated() {
    let client = DemoClient::new();
    let backend = client.as_moderation().expect("moderation accessor");

    let roles = backend
        .get_server_roles("srv-roles")
        .await
        .expect("get_server_roles");
    assert_eq!(roles.len(), 3, "Owner / Moderator / Member");
    assert!(roles.iter().any(|r| r.name == "Owner"));

    let perms = backend
        .get_my_permissions("srv-roles", None)
        .await
        .expect("get_my_permissions");
    assert!(perms.ban_members, "demo user moderates its own servers");
    assert!(perms.manage_channels);
    assert_eq!(perms.display_role, "Owner");
}

#[tokio::test]
async fn update_channel_rename_round_trips_through_get_channels() {
    let client = DemoClient::new();
    let server = client
        .get_servers()
        .await
        .expect("get_servers")
        .into_iter()
        .next()
        .expect("demo has servers");
    let channel = client
        .get_channels(&server.id)
        .await
        .expect("get_channels")
        .into_iter()
        .next()
        .expect("demo server has channels");

    let backend = client.as_moderation().expect("moderation accessor");
    backend
        .update_channel(
            &channel.id,
            UpdateChannelParams {
                name: Some("renamed-by-test".to_string()),
                ..UpdateChannelParams::default()
            },
        )
        .await
        .expect("update_channel");

    let renamed = client
        .get_channels(&server.id)
        .await
        .expect("get_channels")
        .into_iter()
        .find(|c| c.id == channel.id)
        .expect("channel still listed");
    assert_eq!(renamed.name, "renamed-by-test");

    let single = client.get_channel(&channel.id).await.expect("get_channel");
    assert_eq!(
        single.name, "renamed-by-test",
        "get_channel must see the same override as get_channels"
    );
}

#[tokio::test]
async fn delete_message_removes_it_from_get_messages() {
    let client = DemoClient::new();
    let channel_id = "ch-off-topic";
    let messages = client
        .get_messages(channel_id, poly_client::MessageQuery::default())
        .await
        .expect("get_messages");
    let Some(victim) = messages.first().cloned() else {
        return; // no fixture messages in this channel — nothing to assert
    };

    let backend = client.as_moderation().expect("moderation accessor");
    backend
        .delete_message(channel_id, &victim.id)
        .await
        .expect("delete_message");

    let after = client
        .get_messages(channel_id, poly_client::MessageQuery::default())
        .await
        .expect("get_messages");
    assert!(
        !after.iter().any(|m| m.id == victim.id),
        "deleted message must not be re-served"
    );
}
