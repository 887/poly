//! Main application layout — 4-column desktop view.
//!
//! Uses separate `match` arms for `DmsFriends` vs `Server` so that Dioxus
//! tears down and rebuilds components when switching between these views
//! (identical component trees in a combined arm would be reused without
//! re-rendering).

use super::channel_list::ChannelList;
use super::chat_view::ChatView;
use super::notifications::NotificationsView;
use super::server_sidebar::ServerSidebar;
use super::settings::SettingsPage;
use super::user_sidebar::UserSidebar;
use crate::i18n::t;
use crate::state::{AppState, View};
use dioxus::prelude::*;

/// Main application layout.
///
/// Desktop: 4-column layout (servers | channels | chat | users)  
/// Mobile: 3 swipeable panels (TODO)
///
/// `DmsFriends` and `Server` are rendered with structurally different DOM
/// trees so the Dioxus VDOM correctly tears down / rebuilds child components
/// when the user switches between them.
#[component]
pub fn MainLayout() -> Element {
    let app_state: Signal<AppState> = use_context();
    let view = app_state.read().nav.view;
    let show_right = app_state.read().nav.right_sidebar_visible;

    rsx! {
        div { class: "main-layout",
            // Left: Server sidebar (always visible)
            ServerSidebar {}

            // Middle content depends on current view.
            // DECISION(DX): DmsFriends and Server MUST be separate arms so that
            // Dioxus sees genuinely different VDOM structures and does not reuse
            // stale component instances when the user switches views.
            match view {
                View::DmsFriends => rsx! {
                    ChannelList {}
                    // DMs view: show placeholder until a DM/group is selected.
                    // TODO(phase-2.5.9): Render ChatView for selected DM channel.
                    main { class: "chat-view",
                        div { class: "chat-header",
                            span { class: "chat-channel-name", "{t(\"nav-dms\")}" }
                        }
                        div { class: "message-list",
                            div { class: "message-empty",
                                div { class: "empty-wave", "💬" }
                                h3 { "{t(\"chat-select-conversation\")}" }
                            }
                        }
                        div { class: "message-input-area",
                            div { class: "message-input-disabled",
                                "{t(\"chat-select-conversation\")}"
                            }
                        }
                    }
                },
                View::Server => rsx! {
                    ChannelList {}
                    ChatView {}
                    if show_right {
                        UserSidebar {}
                    }
                },
                View::Notifications => rsx! {
                    NotificationsView {}
                },
                View::Settings => rsx! {
                    SettingsPage {}
                },
                View::Setup => rsx! {
                    div { "Redirecting to setup..." }
                },
            }
        }
    }
}
