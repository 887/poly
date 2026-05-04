//! `ChatHeaderActions` — desktop header action bar for the chat view.
//!
//! Contains the icon buttons rendered in the top-right of the chat header:
//! call/video (DM channels), agent toggle, member toggle, threads, pinned,
//! search, settings — plus the overflow menu that collapses them when the
//! header is too narrow.

use crate::client_manager::{ClientManager};
use crate::i18n::t;
use crate::state::{AccountSessions, AppState, BatchedSignal, ChatLists};
use crate::state::ChatData;
use crate::state::VoiceState;
use dioxus::prelude::*;
use poly_client::User;
use poly_ui_macros::{context_menu, ui_action};
use super::super::direct_call::{DirectCallRequest, navigate_to_pending_direct_call_from_active_account};
use super::ChatUtilityPanel;

// ── Overflow detection effect ─────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
pub(super) fn use_header_actions_overflow_effect(
    mut header_actions_overflow: Signal<bool>,
    mut header_actions_menu_open: Signal<bool>,
    mobile_layout_resize_tick: Signal<u64>,
) {
    use_effect(move || { // poly-lint: allow stale-effect-capture — Signal-only; subscribes to mobile_layout_resize_tick Signal
        let _resize_tick = *mobile_layout_resize_tick.read();

        spawn(async move {
            let is_overflowing = dioxus::document::eval(
                r#"(() => {
                    const wrap = document.querySelector('.chat-header-actions-wrap');
                    const row = document.querySelector('.chat-header-actions-primary');
                    if (!wrap || !row) return false;
                    return row.scrollWidth > wrap.clientWidth + 1;
                })()"#,
            )
            .await
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

            header_actions_overflow.set(is_overflowing);
            if !is_overflowing {
                header_actions_menu_open.set(false);
            }
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn use_header_actions_overflow_effect(
    _header_actions_overflow: Signal<bool>,
    _header_actions_menu_open: Signal<bool>,
    _mobile_layout_resize_tick: Signal<u64>,
) {
}

// ── Components ────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn HeaderOverflowItem(
    icon: String,
    label: String,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class_name = if active {
        "chat-header-overflow-item active"
    } else {
        "chat-header-overflow-item"
    };

    rsx! {
        button {
            class: "{class_name}",
            onclick: move |evt| onclick.call(evt),
            span { class: "chat-header-overflow-icon", "{icon}" }
            span { class: "chat-header-overflow-label", "{label}" }
        }
    }
}

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn ChatHeaderActions(
    app_state: BatchedSignal<AppState>,
    utility_panel: Signal<Option<ChatUtilityPanel>>,
    notifications_muted: Signal<bool>,
    show_search_filters: Signal<bool>,
    header_actions_menu_open: Signal<bool>,
    header_actions_overflow: Signal<bool>,
    chat_data: BatchedSignal<ChatData>,
    voice_state: BatchedSignal<VoiceState>,
    client_manager: BatchedSignal<ClientManager>,
    mobile_layout_resize_tick: Signal<u64>,
    is_group_channel: bool,
    is_dm_channel: bool,
    dm_user: Option<User>,
    channel_id: Option<String>,
    member_list_visible: bool,
) -> Element {
    use_header_actions_overflow_effect(header_actions_overflow, header_actions_menu_open, mobile_layout_resize_tick);

    let ui_layout: crate::state::BatchedSignal<crate::state::UiLayout> = use_context();
    let nav_state: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let ui_overlays: crate::state::BatchedSignal<crate::state::UiOverlays> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let app_state = app_state;
    let mut utility_panel = utility_panel;
    let notifications_muted = notifications_muted;
    let mut show_search_filters = show_search_filters;
    let mut header_actions_menu_open = header_actions_menu_open;
    let header_actions_overflow = header_actions_overflow;
    let _chat_data = chat_data; // chat_data kept as prop for API compatibility; navigation now uses chat_lists + account_sessions
    let client_manager = client_manager;
    let active_dm_call = voice_state
        .read()
        .voice_connection
        .clone()
        .filter(|connection| connection.dm_id.as_deref() == channel_id.as_deref());
    let member_sidebar_active = member_list_visible && utility_panel.read().is_none();
    let threads_active = *utility_panel.read() == Some(ChatUtilityPanel::Threads);
    let pinned_active = *utility_panel.read() == Some(ChatUtilityPanel::Pinned);
    let settings_active = *utility_panel.read() == Some(ChatUtilityPanel::Settings);
    let search_active = *utility_panel.read() == Some(ChatUtilityPanel::Search);
    rsx! {
        div { class: "chat-header-actions-wrap",
            div { class: if *header_actions_overflow.read() { "chat-header-actions chat-header-actions-primary is-measuring" } else { "chat-header-actions chat-header-actions-primary" },
                if is_dm_channel && active_dm_call.is_none() {
                    if let Some(dm_target) = dm_user.clone() {
                        button {
                            class: "header-btn chat-header-btn-call",
                            title: t("user-profile-call"),
                            onclick: move |_| {
                                navigate_to_pending_direct_call_from_active_account(
                                    DirectCallRequest {
                                        target_user: dm_target.clone(),
                                        start_video: false,
                                        allow_add_to_active_temporary: false,
                                    },
                                    nav_state,
                                    ui_overlays,
                                    chat_lists,
                                    account_sessions,
                                    client_manager,
                                    navigator(),
                                );
                            },
                            "📞"
                        }
                    }
                    if let Some(dm_target) = dm_user.clone() {
                        button {
                            class: "header-btn chat-header-btn-video",
                            title: t("user-profile-video"),
                            onclick: move |_| {
                                navigate_to_pending_direct_call_from_active_account(
                                    DirectCallRequest {
                                        target_user: dm_target.clone(),
                                        start_video: true,
                                        allow_add_to_active_temporary: false,
                                    },
                                    nav_state,
                                    ui_overlays,
                                    chat_lists,
                                    account_sessions,
                                    client_manager,
                                    navigator(),
                                );
                            },
                            "🎥"
                        }
                    }
                }
                {render_agent_toggle_button(app_state, utility_panel, show_search_filters, is_dm_channel, is_group_channel)}
                {
                    render_member_toggle_button(
                        ui_layout,
                        utility_panel,
                        show_search_filters,
                        is_group_channel,
                        is_dm_channel,
                    )
                }
                button {
                    class: if threads_active { "header-btn active chat-header-btn-threads" } else { "header-btn chat-header-btn-threads" },
                    title: t("threads"),
                    onclick: move |_| {
                        show_search_filters.set(false);
                        let next = if *utility_panel.read() == Some(ChatUtilityPanel::Threads) {
                            None
                        } else {
                            Some(ChatUtilityPanel::Threads)
                        };
                        utility_panel.set(next);
                    },
                    "🧵"
                }
                // Catch-me-up tab removed — feature relocated to chat
                // settings as a "Copy last 20 messages" clipboard button.
                button {
                    class: if pinned_active { "header-btn active chat-header-btn-pinned" } else { "header-btn chat-header-btn-pinned" },
                    title: t("pinned-messages"),
                    onclick: move |_| {
                        show_search_filters.set(false);
                        let next = if *utility_panel.read() == Some(ChatUtilityPanel::Pinned) {
                            None
                        } else {
                            Some(ChatUtilityPanel::Pinned)
                        };
                        utility_panel.set(next);
                    },
                    "📌"
                }
                {
                    render_search_tab_button(
                        utility_panel,
                        show_search_filters,
                        false,
                        is_group_channel,
                        is_dm_channel,
                        app_state,
                        ui_layout,
                    )
                }
                button {
                    class: if settings_active { "header-btn active chat-header-btn-settings" } else { "header-btn chat-header-btn-settings" },
                    title: t("chat-settings"),
                    onclick: move |_| {
                        show_search_filters.set(false);
                        let next = if *utility_panel.read() == Some(ChatUtilityPanel::Settings) {
                            None
                        } else {
                            Some(ChatUtilityPanel::Settings)
                        };
                        utility_panel.set(next);
                        if next.is_some() {
                            ui_layout.batch(|l| {
                                if is_dm_channel || is_group_channel {
                                    l.dm_right_sidebar_visible = false;
                                } else {
                                    l.right_sidebar_visible = false;
                                }
                            });
                        }
                    },
                    span { class: "chat-settings-btn-icon",
                        span { class: "chat-settings-btn-icon-cog", "⚙️" }
                        if *notifications_muted.read() {
                            span { class: "chat-settings-btn-muted-dot" }
                        }
                    }
                }
            }
            if *header_actions_overflow.read() {
                div { class: "chat-header-overflow-anchor",
                    button {
                        class: if *header_actions_menu_open.read() { "header-btn active chat-header-btn-overflow" } else { "header-btn chat-header-btn-overflow" },
                        title: t("action-more"),
                        onclick: move |_| {
                            let is_open = *header_actions_menu_open.read();
                            header_actions_menu_open.set(!is_open);
                        },
                        "..."
                    }
                    if *header_actions_menu_open.read() {
                        div { class: "chat-header-overflow-menu",
                            if is_dm_channel && active_dm_call.is_none() {
                                if let Some(dm_target) = dm_user.clone() {
                                    HeaderOverflowItem {
                                        icon: "📞".to_string(),
                                        label: t("user-profile-call"),
                                        active: false,
                                        onclick: move |_| {
                                            header_actions_menu_open.set(false);
                                            navigate_to_pending_direct_call_from_active_account(
                                                DirectCallRequest {
                                                    target_user: dm_target.clone(),
                                                    start_video: false,
                                                    allow_add_to_active_temporary: false,
                                                },
                                                nav_state,
                                                ui_overlays,
                                                chat_lists,
                                                account_sessions,
                                                client_manager,
                                                navigator(),
                                            );
                                        },
                                    }
                                }
                                if let Some(dm_target) = dm_user {
                                    HeaderOverflowItem {
                                        icon: "🎥".to_string(),
                                        label: t("user-profile-video"),
                                        active: false,
                                        onclick: move |_| {
                                            header_actions_menu_open.set(false);
                                            navigate_to_pending_direct_call_from_active_account(
                                                DirectCallRequest {
                                                    target_user: dm_target.clone(),
                                                    start_video: true,
                                                    allow_add_to_active_temporary: false,
                                                },
                                                nav_state,
                                                ui_overlays,
                                                chat_lists,
                                                account_sessions,
                                                client_manager,
                                                navigator(),
                                            );
                                        },
                                    }
                                }
                            }
                            HeaderOverflowItem {
                                icon: "🤖".to_string(),
                                label: t("agent-panel-toggle"),
                                active: false,
                                onclick: move |_| {
                                    header_actions_menu_open.set(false);
                                    let current = false;
                                    if !current {
                                        // opening agent panel: close members
                                        ui_layout.batch(|l| {
                                            l.dm_right_sidebar_visible = false;
                                            l.right_sidebar_visible = false;
                                        });
                                    }
                                    utility_panel.set(None);
                                    show_search_filters.set(false);
                                },
                            }
                            HeaderOverflowItem {
                                icon: if is_dm_channel { "👤".to_string() } else { "👥".to_string() },
                                label: if is_dm_channel { t("chat-toggle-contact") } else { t("chat-toggle-members") },
                                active: member_sidebar_active,
                                onclick: move |_| {
                                    header_actions_menu_open.set(false);
                                    let current = if is_dm_channel || is_group_channel {
                                        ui_layout.read().dm_right_sidebar_visible
                                    } else {
                                        ui_layout.read().right_sidebar_visible
                                    };
                                    ui_layout.batch(|l| {
                                        if is_dm_channel || is_group_channel {
                                            l.dm_right_sidebar_visible = !current;
                                            l.mobile_dm_contact_detail_visible = false;
                                        } else {
                                            l.right_sidebar_visible = !current;
                                        }
                                    });
                                    // Opening members: close agent panel
                                    utility_panel.set(None);
                                    show_search_filters.set(false);
                                },
                            }
                            HeaderOverflowItem {
                                icon: "🧵".to_string(),
                                label: t("threads"),
                                active: threads_active,
                                onclick: move |_| {
                                    header_actions_menu_open.set(false);
                                    show_search_filters.set(false);
                                    let next = if *utility_panel.read() == Some(ChatUtilityPanel::Threads) {
                                        None
                                    } else {
                                        Some(ChatUtilityPanel::Threads)
                                    };
                                    utility_panel.set(next);
                                },
                            }
                            HeaderOverflowItem {
                                icon: "📌".to_string(),
                                label: t("pinned-messages"),
                                active: pinned_active,
                                onclick: move |_| {
                                    header_actions_menu_open.set(false);
                                    show_search_filters.set(false);
                                    let next = if *utility_panel.read() == Some(ChatUtilityPanel::Pinned) {
                                        None
                                    } else {
                                        Some(ChatUtilityPanel::Pinned)
                                    };
                                    utility_panel.set(next);
                                },
                            }
                            HeaderOverflowItem {
                                icon: "🔎".to_string(),
                                label: t("search-messages"),
                                active: search_active,
                                onclick: move |_| {
                                    header_actions_menu_open.set(false);
                                    show_search_filters.set(false);
                                    let next = if *utility_panel.read() == Some(ChatUtilityPanel::Search) {
                                        None
                                    } else {
                                        Some(ChatUtilityPanel::Search)
                                    };
                                    utility_panel.set(next);
                                    if next.is_some() {
                                        ui_layout.batch(|l| {
                                            if is_dm_channel || is_group_channel {
                                                l.dm_right_sidebar_visible = false;
                                            } else {
                                                l.right_sidebar_visible = false;
                                            }
                                        });
                                    }
                                },
                            }
                            HeaderOverflowItem {
                                icon: "⚙️".to_string(),
                                label: t("chat-settings"),
                                active: settings_active,
                                onclick: move |_| {
                                    header_actions_menu_open.set(false);
                                    show_search_filters.set(false);
                                    let next = if *utility_panel.read() == Some(ChatUtilityPanel::Settings) {
                                        None
                                    } else {
                                        Some(ChatUtilityPanel::Settings)
                                    };
                                    utility_panel.set(next);
                                    if next.is_some() {
                                        ui_layout.batch(|l| {
                                            if is_dm_channel || is_group_channel {
                                                l.dm_right_sidebar_visible = false;
                                            } else {
                                                l.right_sidebar_visible = false;
                                            }
                                        });
                                    }
                                },
                            }
                            // B.5 drafts overflow item dropped — pending
                            // drafts now live inside the agent panel
                            // (per-chat) instead of a standalone tab.
                        }
                    }
                }
            }
        }
    }
}

// ── Render helpers (called only from ChatHeaderActions) ───────────────────────

pub(super) fn render_search_tab_button(
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
    mut show_search_filters: Signal<bool>,
    mobile_tools: bool,
    is_group_channel: bool,
    is_dm_channel: bool,
    app_state: BatchedSignal<AppState>,
    ui_layout: crate::state::BatchedSignal<crate::state::UiLayout>,
) -> Element {
    let active = *utility_panel.read() == Some(ChatUtilityPanel::Search);

    rsx! {
        button {
            class: if active { "header-btn active chat-search-tab-btn chat-header-btn-search" } else { "header-btn chat-search-tab-btn chat-header-btn-search" },
            title: t("search-messages"),
            onclick: move |_| {
                show_search_filters.set(false);
                let next = if *utility_panel.read() == Some(ChatUtilityPanel::Search) {
                    None
                } else {
                    Some(ChatUtilityPanel::Search)
                };
                utility_panel.set(next);
                if mobile_tools || next.is_some() {
                    ui_layout.batch(|l| {
                        if is_dm_channel || is_group_channel {
                            l.dm_right_sidebar_visible = false;
                        } else {
                            l.right_sidebar_visible = false;
                        }
                    });
                }
            },
            span { class: "chat-search-tab-icon",
                span { class: "chat-search-tab-icon-base", "📰" }
                span { class: "chat-search-tab-icon-overlay", "🔎" }
            }
        }
    }
}

pub(super) fn render_agent_toggle_button(
    _app_state: BatchedSignal<AppState>,
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
    mut show_search_filters: Signal<bool>,
    is_dm_channel: bool,
    is_group_channel: bool,
) -> Element {
    let agent_active = *utility_panel.read() == Some(ChatUtilityPanel::Agent);
    rsx! {
        button {
            class: if agent_active { "header-btn soft-active chat-header-btn-agent" } else { "header-btn chat-header-btn-agent" },
            title: t("agent-panel-toggle"),
            onclick: move |_| {
                // Toggle the Agent utility-rail tab. The right wing also
                // hosts Search/Members/Threads/Pinned/Settings/Drafts, so
                // sharing the rail keeps all those reachable from a single
                // toggle group instead of stacking a takeover panel on top.
                let next = if *utility_panel.read() == Some(ChatUtilityPanel::Agent) {
                    None
                } else {
                    Some(ChatUtilityPanel::Agent)
                };
                utility_panel.set(next);
                show_search_filters.set(false);
                let _ = (is_dm_channel, is_group_channel);
            },
            "🤖"
        }
    }
}

pub(super) fn render_member_toggle_button(
    ui_layout: crate::state::BatchedSignal<crate::state::UiLayout>,
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
    mut show_search_filters: Signal<bool>,
    is_group_channel: bool,
    is_dm_channel: bool,
) -> Element {
    if is_group_channel {
        return rsx! {
            button {
                class: if ui_layout.read().dm_right_sidebar_visible { "header-btn soft-active chat-members-toggle-btn chat-header-btn-members" } else { "header-btn chat-members-toggle-btn chat-header-btn-members" },
                title: t("chat-toggle-members"),
                onclick: move |_| {
                    let current = ui_layout.read().dm_right_sidebar_visible;
                    ui_layout.batch(|l| l.dm_right_sidebar_visible = !current);
                    // Opening members: close agent panel
                    utility_panel.set(None);
                    show_search_filters.set(false);
                },
                "👥"
            }
        };
    }

    if is_dm_channel {
        return rsx! {
            button {
                class: if ui_layout.read().dm_right_sidebar_visible { "header-btn soft-active chat-members-toggle-btn chat-header-btn-members" } else { "header-btn chat-members-toggle-btn chat-header-btn-members" },
                title: t("chat-toggle-contact"),
                onclick: move |_| {
                    let current = ui_layout.read().dm_right_sidebar_visible;
                    ui_layout.batch(|l| l.dm_right_sidebar_visible = !current);
                    // Opening contact panel: close agent panel
                    utility_panel.set(None);
                    show_search_filters.set(false);
                },
                "👤"
            }
        };
    }

    rsx! {
        button {
            class: if ui_layout.read().right_sidebar_visible { "header-btn soft-active chat-members-toggle-btn chat-header-btn-members" } else { "header-btn chat-members-toggle-btn chat-header-btn-members" },
            title: t("chat-toggle-members"),
            onclick: move |_| {
                let current = ui_layout.read().right_sidebar_visible;
                ui_layout.batch(|l| l.right_sidebar_visible = !current);
                // Opening members: close agent panel
                utility_panel.set(None);
                show_search_filters.set(false);
            },
            "👥"
        }
    }
}
