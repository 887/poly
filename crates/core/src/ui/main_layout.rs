//! Main application layout — 4-column desktop view.
//!
//! Uses separate `match` arms for `DmsFriends` vs `Server` so that Dioxus
//! tears down and rebuilds components when switching between these views
//! (identical component trees in a combined arm would be reused without
//! re-rendering).

use super::account_bar::AccountBar;
use super::channel_list::ChannelList;
use super::chat_view::ChatView;
use super::notifications::NotificationsView;
use super::server_sidebar::ServerSidebar;
use super::settings::SettingsPage;
use super::user_sidebar::UserSidebar;
use super::voice_bar::VoiceBar;
use super::voice_view::VoiceChannelView;
use crate::i18n::t;
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;
use poly_client::ChannelType;

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
    let mut app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let view = app_state.read().nav.view;
    let show_right = app_state.read().nav.right_sidebar_visible;

    // Determine if the currently selected channel is a voice/video channel
    let is_voice_channel = chat_data
        .read()
        .current_channel
        .as_ref()
        .is_some_and(|ch| matches!(ch.channel_type, ChannelType::Voice | ChannelType::Video));

    let can_back = app_state.read().can_go_back();
    let can_forward = app_state.read().can_go_forward();

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
                    // Channel list panel — nav-bar at top, NO AccountBar.
                    // Multi-account: there is no single "current user" in DMs.
                    div { class: "channel-list-wrapper",
                        div { class: "nav-bar",
                            button {
                                class: if can_back { "nav-btn" } else { "nav-btn disabled" },
                                disabled: !can_back,
                                onclick: move |_| { app_state.write().nav_back(); },
                                title: "{t(\"nav-back\")}",
                                "◀"
                            }
                            button {
                                class: if can_forward { "nav-btn" } else { "nav-btn disabled" },
                                disabled: !can_forward,
                                onclick: move |_| { app_state.write().nav_forward(); },
                                title: "{t(\"nav-forward\")}",
                                "▶"
                            }
                        }
                        ChannelList {}
                        VoiceBar {}
                        // AccountBar intentionally omitted — Poly is multi-account.
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
                    // Channel list panel — nav-bar at top + AccountBar at bottom
                    div { class: "channel-list-wrapper",
                        div { class: "nav-bar",
                            button {
                                class: if can_back { "nav-btn" } else { "nav-btn disabled" },
                                disabled: !can_back,
                                onclick: move |_| { app_state.write().nav_back(); },
                                title: "{t(\"nav-back\")}",
                                "◀"
                            }
                            button {
                                class: if can_forward { "nav-btn" } else { "nav-btn disabled" },
                                disabled: !can_forward,
                                onclick: move |_| { app_state.write().nav_forward(); },
                                title: "{t(\"nav-forward\")}",
                                "▶"
                            }
                        }
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
