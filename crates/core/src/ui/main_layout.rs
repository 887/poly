//! Main application layout — 4-column desktop view.
//!
//! Uses separate `match` arms for `DmsFriends` vs `Server` so that Dioxus
//! tears down and rebuilds components when switching between these views
//! (identical component trees in a combined arm would be reused without
//! re-rendering).
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use super::account_bar::AccountBar;
use super::account_switcher::AccountSwitcher;
use super::channel_list::ChannelList;
use super::chat_view::ChatView;
use super::friends_panel::FriendsPanel;
use super::notifications::NotificationsView;
use super::server_sidebar::ServerSidebar;
use super::settings::SettingsPage;
use super::user_sidebar::UserSidebar;
use super::voice_banner::VoiceBanner;
use super::voice_bar::VoiceBar;
use super::voice_view::VoiceChannelView;
use crate::i18n::t;
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;
use poly_client::ChannelType;

/// Navigation bar component — only renders on native platforms (desktop/mobile).
/// On web, the browser's native back/forward buttons are used, so we don't need
/// the reserved space or UI buttons.
#[component]
fn NavBar() -> Element {
    #[cfg(feature = "native-nav")]
    {
        let mut app_state: Signal<AppState> = use_context();
        let can_back = app_state.read().can_go_back();
        let can_forward = app_state.read().can_go_forward();

        return rsx! {
            div { class: "nav-bar-top",
                button {
                    class: if can_back { "nav-btn" } else { "nav-btn disabled" },
                    disabled: !can_back,
                    onclick: move |_| {
                        app_state.write().nav_back();
                    },
                    title: "{t(\"nav-back\")}",
                    "◀"
                }
                button {
                    class: if can_forward { "nav-btn" } else { "nav-btn disabled" },
                    disabled: !can_forward,
                    onclick: move |_| {
                        app_state.write().nav_forward();
                    },
                    title: "{t(\"nav-forward\")}",
                    "▶"
                }
            }
        };
    }

    #[cfg(not(feature = "native-nav"))]
    {
        return rsx! {
            Fragment {}
        };
    }
}

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
    let chat_data: Signal<ChatData> = use_context();
    let view = app_state.read().nav.view;
    let show_right = app_state.read().nav.right_sidebar_visible;

    // Determine if the currently selected channel is a voice/video channel
    let is_voice_channel = chat_data
        .read()
        .current_channel
        .as_ref()
        .is_some_and(|ch| matches!(ch.channel_type, ChannelType::Voice | ChannelType::Video));

    rsx! {
        div { class: "main-layout",
            // Voice connection banner — spans full width when connected
            VoiceBanner {}
            // Main body: nav + columns
            div { class: "main-layout-body",
                // Back/Forward navigation — only on native platforms (not web)
                NavBar {}
                // Left: Server sidebar (always visible)
                ServerSidebar {}

                // Middle content depends on current view.
                // DECISION(DX): DmsFriends and Server MUST be separate arms so that
                // Dioxus sees genuinely different VDOM structures and does not reuse
                // stale component instances when the user switches views.
                match view {
                    View::DmsFriends => rsx! {
                        // Channel list panel — AccountSwitcher for multi-account access
                        div { class: "channel-list-wrapper",
                            ChannelList {}
                            VoiceBar {}
                            AccountSwitcher {}
                        }
                        // Placeholder until a conversation is selected
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
                                div { class: "message-input-disabled", "{t(\"chat-select-conversation\")}" }
                            }
                        }
                    },
                    View::Server => rsx! {
                        // Channel list panel — AccountBar at bottom
                        div { class: "channel-list-wrapper",
                            ChannelList {}
                            VoiceBar {}
                            AccountBar {}
                        }
                        // Show voice view for voice/video channels, chat view for text
                        if is_voice_channel {
                            VoiceChannelView {}
                        } else {
                            ChatView {}
                        }
                        if show_right && !is_voice_channel {
                            UserSidebar {}
                        }
                    },
                    View::Notifications => rsx! {
                        NotificationsView {}
                    },
                    View::Friends => rsx! {
                        FriendsPanel {}
                    },
                    View::Settings => rsx! {
                        SettingsPage {}
                    },
                    View::Setup => rsx! {
                        div { "Redirecting to setup..." }
                    },
                }
            } // end main-layout-body
        }
    }
}
