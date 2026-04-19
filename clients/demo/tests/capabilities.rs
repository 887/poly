#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-1 regression: Demo clients declare rich capabilities.

use poly_client::{
    BackendCapabilities, ClientBackend, DmSupport, MessagingModel, NotificationSupport,
    capabilities_for_slug,
};
use poly_demo::{DemoClient, DemoClient2, DemoClient3};

#[test]
fn demo_cat_is_full_social_chat() {
    let client = DemoClient::new();
    assert_eq!(client.backend_capabilities(), BackendCapabilities::FULL_SOCIAL_CHAT);
}

#[test]
fn demo_dog_is_full_social_chat() {
    let client = DemoClient2::new();
    assert_eq!(client.backend_capabilities(), BackendCapabilities::FULL_SOCIAL_CHAT);
}

#[test]
fn demo_forum_is_messaging_no_social() {
    let client = DemoClient3::new();
    let caps = client.backend_capabilities();
    assert_eq!(caps.messaging, MessagingModel::Full);
    assert_eq!(caps.dms, DmSupport::None);
    assert_eq!(caps.notifications, NotificationSupport::Inbox);
}

/// WP-10 — plugin-vs-slug parity for all three demo variants.
#[test]
fn demo_plugins_match_slug_lookup_table() {
    assert_eq!(
        DemoClient::new().backend_capabilities(),
        capabilities_for_slug("demo")
    );
    assert_eq!(
        DemoClient2::new().backend_capabilities(),
        capabilities_for_slug("demo")
    );
    assert_eq!(
        DemoClient3::new().backend_capabilities(),
        capabilities_for_slug("demo_forum")
    );
}
