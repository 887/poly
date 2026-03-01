//! Main application layout — 4-column desktop view.

use super::channel_list::ChannelList;
use super::chat_view::ChatView;
use super::notifications::NotificationsView;
use super::server_sidebar::ServerSidebar;
use super::settings::SettingsPage;
use super::user_sidebar::UserSidebar;
use crate::state::{AppState, View};
use dioxus::prelude::*;

/// Main application layout.
///
/// Desktop: 4-column layout (servers | channels | chat | users)  
/// Mobile: 3 swipeable panels (TODO)
#[component]
pub fn MainLayout(app_state: Signal<AppState>) -> Element {
    let view = app_state.read().nav.view;
    let show_right = app_state.read().nav.right_sidebar_visible;

    rsx! {
        div { class: "main-layout",
            // Left: Server sidebar (always visible)
            ServerSidebar { app_state }

            // Middle content depends on current view
            match view {
                View::DmsFriends | View::Server => rsx! {
                    // Channel list
                    ChannelList { app_state }
                    // Chat view
                    ChatView { app_state } // Chat view
                    // Right: User sidebar
                    if show_right {
                        UserSidebar { app_state } // Right: User sidebar
                    }
                },
                View::Notifications => rsx! {
                    NotificationsView { app_state }
                },
                View::Settings => rsx! {
                    SettingsPage { app_state }
                },
                View::Setup => rsx! {
                    // Should not happen — setup wizard is shown instead
                    div { "Redirecting to setup..." }
                },
            }
        }
    }
}
