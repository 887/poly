#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Invariant: a capability flag the demo backend *declares* must be backed by
//! the accessor that serves it.
//!
//! The demo backend is the reference `IsBackend` impl, and both per-slug
//! capability tables (`poly_client::capabilities_for_slug_static` and the
//! duplicate seed inside `ClientManager`) declare `demo` / `poly` /
//! `demo_forum` as moderation-capable. Before the `ModerationBackend` impl
//! landed, `as_moderation()` returned the `None` default, so Server Settings
//! rendered the Roles / Bans / Mod-Log tabs and every fetch failed with
//! `NotSupported`. This test pins the declaration → accessor invariant so the
//! two cannot drift apart again.

use poly_client::{
    BackendCapabilities, CommunitySearchSupport, IsBackend, capabilities_for_slug_static,
};
use poly_demo::{DemoClient, DemoClient2, DemoClient3};

fn declares_any_moderation(caps: &BackendCapabilities) -> bool {
    caps.has_roles
        || caps.has_kick
        || caps.has_ban
        || caps.has_timed_ban
        || caps.has_channel_mgmt
        || caps.has_moderation_log
}

fn assert_accessors_back_declaration(label: &str, backend: &dyn IsBackend) {
    let caps = backend.backend_capabilities();

    if declares_any_moderation(&caps) {
        assert!(
            backend.as_moderation().is_some(),
            "{label}: declares moderation flags but as_moderation() is None"
        );
    }
    if caps.has_kick || caps.has_ban || caps.has_timed_ban || caps.has_channel_mgmt {
        let moderation = backend
            .as_moderation()
            .expect("moderation accessor checked above");
        assert!(
            moderation.as_writable_moderation().is_some(),
            "{label}: declares mutating moderation flags but \
             as_writable_moderation() is None"
        );
    }
    if caps.community_search != CommunitySearchSupport::None {
        assert!(
            backend.as_discover().is_some(),
            "{label}: declares community_search but as_discover() is None"
        );
    }
    if caps.supports_comment_feed {
        assert!(
            backend.as_forum().is_some(),
            "{label}: declares supports_comment_feed but as_forum() is None"
        );
    }
}

#[test]
fn live_capabilities_are_backed_by_accessors() {
    assert_accessors_back_declaration("demo (cat)", &DemoClient::new());
    assert_accessors_back_declaration("demo (dog)", &DemoClient2::new());
    assert_accessors_back_declaration("demo_forum (platypus)", &DemoClient3::new());
}

#[test]
fn static_slug_table_moderation_flags_are_backed_by_accessors() {
    // The static table is what the UI and chat-mcp consult before a live
    // handle exists — over-declaring there is exactly the bug this guards.
    let cases: [(&str, &dyn IsBackend); 2] = [
        ("demo", &DemoClient::new()),
        ("demo_forum", &DemoClient3::new()),
    ];
    for (slug, backend) in cases {
        let table = capabilities_for_slug_static(slug);
        if declares_any_moderation(&table) {
            assert!(
                backend.as_moderation().is_some(),
                "slug '{slug}' is declared moderation-capable in \
                 capabilities_for_slug_static but the backend's \
                 as_moderation() is None"
            );
        }
    }
}

#[test]
fn live_moderation_flags_match_the_static_slug_table() {
    let cases: [(&str, BackendCapabilities); 2] = [
        ("demo", DemoClient::new().backend_capabilities()),
        ("demo_forum", DemoClient3::new().backend_capabilities()),
    ];
    for (slug, live) in cases {
        let table = capabilities_for_slug_static(slug);
        assert_eq!(live.has_roles, table.has_roles, "{slug}: has_roles drift");
        assert_eq!(live.has_kick, table.has_kick, "{slug}: has_kick drift");
        assert_eq!(live.has_ban, table.has_ban, "{slug}: has_ban drift");
        assert_eq!(
            live.has_timed_ban, table.has_timed_ban,
            "{slug}: has_timed_ban drift"
        );
        assert_eq!(
            live.has_channel_mgmt, table.has_channel_mgmt,
            "{slug}: has_channel_mgmt drift"
        );
        assert_eq!(
            live.has_moderation_log, table.has_moderation_log,
            "{slug}: has_moderation_log drift"
        );
    }
}
