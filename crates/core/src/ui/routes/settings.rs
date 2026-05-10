//! Settings-domain route adapter components.
//!
//! Covers the app-level settings route, account-scoped settings, server settings,
//! and per-channel settings.

use crate::ui::account::{AccountSettingsPage, ChannelSettingsPage, ServerSettingsPage};
use crate::ui::settings::SettingsPage;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Settings page — app-level, not account-scoped.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn SettingsRoute() -> Element {
    rsx! {
        SettingsPage {}
    }
}

/// Settings page with a specific section pre-selected via URL.
///
/// `/settings/:section` deep-links directly into a settings section.
/// `sync_route_to_app_state` parses the `section` slug and writes it to
/// `AppState.settings_section`, so `SettingsPage` renders the correct content.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn SettingsSectionRoute(section: String) -> Element {
    // Navigation was already handled by sync_route_to_app_state;
    // SettingsPage reads settings_section from AppState.
    let _ = section; // consumed by router; state already synced
    rsx! {
        SettingsPage {}
    }
}

/// Account settings — scoped to a specific backend account.
///
/// Passes the account context to AccountSettingsPage so it shows only
/// account-relevant settings (notifications). Global settings (theme,
/// identity, backup) remain in the app-level SettingsRoute.
///
/// AccountSettingsPage renders its own channel-list-wrapper (with settings nav
/// + AccountBar) and settings-content sibling, matching the normal layout.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn AccountSettingsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        AccountSettingsPage { backend, account_id }
    }
}

/// Server settings — notifications, profile, and general for a specific server.
///
/// Routes to the server-scoped settings page which provides notification levels,
/// per-server profile (nickname/avatar), and general options including leave server.
///
/// ServerSettingsPage renders its own channel-list-wrapper (with settings nav
/// + AccountBar) and settings-content sibling, matching the normal layout.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ServerSettingsRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    rsx! {
        ServerSettingsPage {
            backend,
            instance_id,
            account_id,
            server_id,
            section: "overview".to_string(),
        }
    }
}

/// Server settings for a specific section of one server.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ServerSettingsSectionRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    section: String,
) -> Element {
    rsx! {
        ServerSettingsPage {
            backend,
            instance_id,
            account_id,
            server_id,
            section,
        }
    }
}

/// Per-channel settings — Pack C.3 / P19.
///
/// Delegates to [`ChannelSettingsPage`] which renders the plugin-declared
/// `PerChannel` settings sections (empty-state message if the backend
/// declares none).
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ChannelSettingsRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    rsx! {
        ChannelSettingsPage {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        }
    }
}
