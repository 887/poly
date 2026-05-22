//! Chat layout shell — top-level layout, header, side column, tools panel,
//! and utility rail routing.
//!
//! Single responsibility: decides WHAT goes where in the two-column layout
//! (main content column vs. right side column) and renders the structural
//! shells. Does NOT own any message list / composer / overlay logic.

use dioxus::prelude::*;

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::BatchedSignal;
use crate::state::{
    AccountSessions, AppState, ChatLists, ChatViewState, NavState, UiLayout, UiOverlays,
    VoiceState,
};
use crate::state::chat_data::{backend_badge, user_color};
use poly_client::{MessageSearchHit, PresenceStatus};

use super::markup_ctx::ChatViewMarkupCtx;
use super::ChatUtilityPanel;
use super::header::{ChatHeaderActions, render_agent_toggle_button, render_search_tab_button};
use super::search_filter::render_chat_header_search;
use super::utility_rail::ChatUtilityRail;
use super::open_message_hit;
use super::highlight_message;
use super::super::direct_call::{DirectCallRequest, navigate_to_pending_direct_call_from_active_account};
use super::super::dm_user_sidebar::DmUserSidebar;
use super::super::user_sidebar::UserSidebar;
use super::super::thread_view::ThreadPanel;
use super::overlays::DmContactListPanel;
use super::composer::{render_message_input_area, render_chat_overlays};
use super::scroll::{render_jump_to_present, render_message_list, render_unread_banner};
use super::overlays::TypingIndicator;
use super::super::thread_view::ActiveThreadsBar;
use crate::ui::split_shell::RightWingShell;

#[cfg(target_arch = "wasm32")]
const MOBILE_RIGHT_WING_OPEN_JS: &str = "window.__polySetMobileRightWingOpen?.(true);";
#[cfg(target_arch = "wasm32")]
const MOBILE_RIGHT_WING_CLOSE_JS: &str = "window.__polySetMobileRightWingOpen?.(false);";

/// Returns true when the running page is in mobile-layout mode.
///
/// Inspects the `.poly-app` CSS class list at call time; falls back to
/// the persisted / URL-overridden setting if the element is not yet mounted.
#[cfg(target_arch = "wasm32")]
pub(super) fn runtime_mobile_ui_active() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };

    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or_default();
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or_default();

    let classes = window
        .document()
        .and_then(|document| document.query_selector(".poly-app").ok().flatten())
        .and_then(|root| root.get_attribute("class"));

    // Early render/hydration fallback: if `.poly-app` isn't available yet,
    // mirror the real app-shell precedence: URL override -> persisted setting.
    let Some(classes) = classes else {
        let (configured_mode, legacy_force_mobile) =
            crate::ui::load_persisted_layout_mode_from_window(&window);
        let fallback_mode = crate::ui::layout_query_override().unwrap_or_else(|| {
            crate::ui::effective_layout_mode(configured_mode, legacy_force_mobile)
        });
        return crate::ui::layout_mode_is_mobile(fallback_mode);
    };

    classes
        .split_whitespace()
        .any(|class| class == "poly-layout-mode-force-mobile")
        || (classes
            .split_whitespace()
            .any(|class| class == "poly-layout-mode-auto-width")
            && viewport_width <= 640.0)
        || (classes
            .split_whitespace()
            .any(|class| class == "poly-layout-mode-auto-portrait")
            && viewport_height > viewport_width)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) const fn runtime_mobile_ui_active() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
pub(super) fn sync_mobile_side_column_open(open: bool) {
    if !runtime_mobile_ui_active() {
        let _ = document::eval(MOBILE_RIGHT_WING_CLOSE_JS);
        return;
    }

    let _ = document::eval(if open {
        MOBILE_RIGHT_WING_OPEN_JS
    } else {
        MOBILE_RIGHT_WING_CLOSE_JS
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn sync_mobile_side_column_open(_open: bool) {}

pub(super) fn mobile_server_right_wing_active(ctx: &ChatViewMarkupCtx) -> bool {
    runtime_mobile_ui_active() && !ctx.is_dm_channel && !ctx.is_group_channel
}

pub(super) fn close_chat_side_column_state(
    ui_layout: BatchedSignal<UiLayout>,
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
    mut show_search_filters: Signal<bool>,
    is_group_channel: bool,
    is_dm_channel: bool,
) {
    show_search_filters.set(false);
    if utility_panel.read().is_some() {
        utility_panel.set(None);
        return;
    }

    // Close agent panel first if open
    if false {
        return;
    }

    // Collapse 2-3 writes into ONE batch — see CLAUDE.md § Common WASM-hang causes #1.
    ui_layout.batch(|l| {
        if is_group_channel || is_dm_channel {
            l.dm_right_sidebar_visible = false;
            l.mobile_dm_contact_detail_visible = false;
        } else {
            l.right_sidebar_visible = false;
        }
    });
}

pub(super) fn render_chat_layout_shell(ctx: ChatViewMarkupCtx) -> Element {
    let show_side_column = ctx.utility_panel.read().is_some()
        || ctx.member_list_visible
        || mobile_server_right_wing_active(&ctx);
    let mobile_layout = runtime_mobile_ui_active();

    rsx! {
        div { class: "chat-layout-shell",
            {render_chat_main_column(ctx.clone())}
            if mobile_layout && show_side_column {
                {render_chat_side_column(ctx)}
            }
        }
    }
}

fn render_chat_main_column(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "chat-main-column",
            {render_chat_header(ctx.clone())}
            {render_chat_body_shell(ctx)}
        }
    }
}

fn render_chat_header(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "chat-header",
            {render_chat_header_info(ctx.clone())}
            {render_chat_header_right(ctx)}
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_chat_header_info(ctx: ChatViewMarkupCtx) -> Element {
    let current_channel = ctx.current_channel.clone();
    let current_server = ctx.current_server.clone();
    let dm_user_avatar = ctx.dm_user_avatar.clone();
    let dm_user_presence = ctx.dm_user_presence;
    let is_dm_channel = ctx.is_dm_channel;
    let is_group_channel = ctx.is_group_channel;
    let group_count = ctx.group_members.len();
    let dm_presence_dot_class = match dm_user_presence {
        PresenceStatus::Online => "presence-dot online",
        PresenceStatus::Idle => "presence-dot idle",
        PresenceStatus::DoNotDisturb => "presence-dot dnd",
        PresenceStatus::Offline | PresenceStatus::Invisible | PresenceStatus::Unknown => "",
    };

    rsx! {
        if let Some(ref ch) = current_channel {
            if is_dm_channel {
                div { class: "dm-chat-header-info",
                    div { class: "dm-chat-avatar-wrap",
                        if let Some(ref avatar) = dm_user_avatar {
                            img {
                                class: "dm-chat-avatar",
                                src: "{avatar}",
                                alt: "{ch.name}",
                            }
                        } else {
                            div {
                                class: "dm-chat-avatar",
                                style: "background:{user_color(&ch.id)}",
                                "{ch.name.chars().next().unwrap_or('?')}"
                            }
                        }
                        if !dm_presence_dot_class.is_empty() {
                            span { class: "{dm_presence_dot_class}" }
                        }
                    }
                    div { class: "dm-chat-header-text",
                        span { class: "chat-channel-name", "{ch.name}" }
                        span { class: "chat-header-subtitle", {t("dm-header-subtitle")} }
                    }
                }
            } else if is_group_channel {
                div { class: "dm-chat-header-info",
                    div { class: "group-chat-icon", "👥" }
                    div { class: "dm-chat-header-text",
                        span { class: "chat-channel-name", "{ch.name}" }
                        span { class: "chat-header-subtitle",
                            {format!("{} {}", group_count, t("group-members-title"))}
                        }
                    }
                }
            } else {
                div { class: "server-chat-header-info",
                    span { class: "chat-channel-name", "# {ch.name}" }
                    if let Some(ref server) = current_server {
                        span { class: "chat-source-badge",
                            "{backend_badge(&server.backend)} {server.backend.display_name()}"
                        }
                    }
                }
            }
        } else {
            span { class: "chat-channel-name", {t("chat-no-messages")} }
        }
    }
}

fn render_chat_header_right(ctx: ChatViewMarkupCtx) -> Element {
    let mobile_right_wing = runtime_mobile_ui_active();

    rsx! {
        div { class: "chat-header-right",
            if mobile_right_wing {
                {render_mobile_chat_header_right_toggle(ctx)}
            } else {
                ChatHeaderActions {
                    app_state: ctx.app_state,
                    utility_panel: ctx.utility_panel,
                    notifications_muted: ctx.notifications_muted,
                    show_search_filters: ctx.show_search_filters,
                    header_actions_menu_open: ctx.header_actions_menu_open,
                    header_actions_overflow: ctx.header_actions_overflow,
                    voice_state: ctx.voice_state,
                    client_manager: ctx.client_manager,
                    mobile_layout_resize_tick: ctx.mobile_layout_resize_tick,
                    is_group_channel: ctx.is_group_channel,
                    is_dm_channel: ctx.is_dm_channel,
                    dm_user: ctx.dm_user.clone(),
                    channel_id: ctx.channel_id.clone(),
                    member_list_visible: ctx.member_list_visible,
                }
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_mobile_chat_header_right_toggle(ctx: ChatViewMarkupCtx) -> Element {
    let nav_state = ctx.nav;
    let ui_overlays = ctx.ui_overlays;
    let ui_layout = ctx.ui_layout;
    let mut utility_panel = ctx.utility_panel;
    let mut show_search_filters = ctx.show_search_filters;
    let right_wing_open = ctx.member_list_visible || ctx.utility_panel.read().is_some();
    let current_server = ctx.current_server.clone();
    let current_channel = ctx.current_channel.clone();
    let dm_user = ctx.dm_user.clone();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let voice_state = ctx.voice_state;
    let client_manager = ctx.client_manager;
    let is_dm_channel = ctx.is_dm_channel;
    let is_group_channel = ctx.is_group_channel;
    let active_dm_call = voice_state
        .read()
        .voice_connection
        .clone()
        .filter(|connection| connection.dm_id.as_deref() == ctx.channel_id.as_deref());
    // For DMs, don't use the avatar — always show "@" on mobile
    let toggle_icon_url = if is_dm_channel {
        None
    } else {
        current_server
            .as_ref()
            .and_then(|server| server.icon_url.clone())
    };

    let toggle_label = if is_dm_channel {
        current_channel
            .as_ref().map_or_else(|| t("chat-toggle-contact"), |channel| channel.name.clone())
    } else if is_group_channel {
        current_channel
            .as_ref().map_or_else(|| t("chat-toggle-members"), |channel| channel.name.clone())
    } else {
        current_server
            .as_ref().map_or_else(|| t("chat-toggle-members"), |server| server.name.clone())
    };
    let toggle_fallback = if is_dm_channel {
        // On mobile, DMs show "@" symbol instead of first character
        "@".to_string()
    } else if is_group_channel {
        "👥".to_string()
    } else {
        current_server
            .as_ref().map_or_else(|| "#".to_string(), |server| server.name.chars().next().unwrap_or('#').to_string())
    };

    rsx! {
        div { class: "chat-header-actions chat-header-actions-mobile",
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
                if let Some(dm_target) = dm_user {
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
            button {
                class: if right_wing_open { "header-btn soft-active poly-mobile-right-wing-toggle mobile-server-icon-toggle" } else { "header-btn poly-mobile-right-wing-toggle mobile-server-icon-toggle" },
                title: if is_dm_channel { t("chat-toggle-contact") } else { t("chat-toggle-members") },
                aria_label: "{toggle_label}",
                onclick: move |_| {
                    let currently_open = if is_dm_channel || is_group_channel {
                        ui_layout.read().dm_right_sidebar_visible
                    } else {
                        ui_layout.read().right_sidebar_visible
                    };
                    let is_opening = !currently_open;

                    show_search_filters.set(false);
                    utility_panel.set(None);

                    if is_opening {
                        show_search_filters.set(false);
                        ui_layout.batch(|l| {
                            if is_dm_channel || is_group_channel {
                                l.dm_right_sidebar_visible = true;
                                l.mobile_dm_contact_detail_visible = false;
                            } else {
                                l.right_sidebar_visible = true;
                            }
                        });
                    } else {
                        close_chat_side_column_state(
                            ui_layout,
                            utility_panel,
                            show_search_filters,
                            is_group_channel,
                            is_dm_channel,
                        );
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = document::eval(
                            if is_opening {
                                MOBILE_RIGHT_WING_OPEN_JS
                            } else {
                                MOBILE_RIGHT_WING_CLOSE_JS
                            },
                        );
                    }
                },
                if let Some(ref icon_url) = toggle_icon_url {
                    img {
                        class: "mobile-server-icon-image",
                        src: "{icon_url}",
                        alt: "{toggle_label}",
                    }
                } else {
                    span { class: "mobile-server-icon-fallback", "{toggle_fallback}" }
                }
            }
        }
    }
}

fn render_chat_body_shell(ctx: ChatViewMarkupCtx) -> Element {
    let show_side_column = ctx.utility_panel.read().is_some()
        || ctx.member_list_visible;
    let mobile_layout = runtime_mobile_ui_active();
    // 5.2 — Thread panel is visible when a thread_id is stored in nav state
    // and we are not in mobile layout (mobile uses the full-page ThreadView route).
    let thread_panel_open = ctx.ui_overlays.read().thread_panel_open.is_some();

    rsx! {
        div { class: "chat-body-shell",
            {render_chat_content_column(ctx.clone())}
            if !mobile_layout && thread_panel_open {
                ThreadPanel {}
            }
            if !mobile_layout && show_side_column {
                {render_chat_side_column(ctx)}
            }
        }
    }
}

fn render_chat_content_column(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "chat-content-column",
            // 5.4 — Active threads bar above the message list for text channels
            // that have active threads. Renders nothing if no threads exist.
            ActiveThreadsBar {}
            {render_message_list(ctx.clone())}
            {render_jump_to_present(ctx.clone())}
            TypingIndicator {}
            {render_message_input_area(ctx)}
        }
    }
}

fn render_chat_side_column(ctx: ChatViewMarkupCtx) -> Element {
    let current_channel_name = ctx
        .current_channel
        .as_ref()
        .map(|channel| channel.name.clone())
        .unwrap_or_default();
    let panel = *ctx.utility_panel.read();
    let mobile_tools = runtime_mobile_ui_active();

    rsx! {
        RightWingShell {
            panel_class: String::new(),
            content: rsx! {
                if mobile_tools {
                    {render_chat_tools_panel(ctx.clone())}
                }
                if let Some(panel) = panel {
                    {render_chat_utility_rail(ctx, panel, current_channel_name)}
                } else if ctx.is_dm_channel {
                    DmContactListPanel { channel_id: ctx.channel_id.clone().unwrap_or_default() }
                } else if ctx.is_group_channel {
                    DmUserSidebar {}
                } else {
                    UserSidebar {}
                }
            },
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_chat_tools_panel(ctx: ChatViewMarkupCtx) -> Element {
    let app_state = ctx.app_state;
    let ui_layout = ctx.ui_layout;
    let mut utility_panel = ctx.utility_panel;
    let notifications_muted = ctx.notifications_muted;
    let mut show_search_filters = ctx.show_search_filters;
    let member_sidebar_active = ctx.member_list_visible;
    let is_group_channel = ctx.is_group_channel;
    let is_dm_channel = ctx.is_dm_channel;
    let threads_active = *utility_panel.read() == Some(ChatUtilityPanel::Threads);
    let pinned_active = *utility_panel.read() == Some(ChatUtilityPanel::Pinned);
    let settings_active = *utility_panel.read() == Some(ChatUtilityPanel::Settings);
    rsx! {
        div { class: "chat-tools-panel",
            div { class: "chat-tools-topbar",
                button {
                    class: "header-btn chat-tools-close poly-mobile-right-wing-close-state",
                    title: t("action-close"),
                    onclick: move |_| {
                        close_chat_side_column_state(
                            ui_layout,
                            utility_panel,
                            show_search_filters,
                            is_group_channel,
                            is_dm_channel,
                        );
                        #[cfg(target_arch = "wasm32")]
                        {
                            let _ = document::eval(MOBILE_RIGHT_WING_CLOSE_JS);
                        }
                    },
                    "✕"
                }
                div { class: "chat-tools-actions",
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
                            ui_layout.batch(|l| l.right_sidebar_visible = false);
                        },
                        span { class: "chat-settings-btn-icon",
                            span { class: "chat-settings-btn-icon-cog", "⚙️" }
                            if *notifications_muted.read() {
                                span { class: "chat-settings-btn-muted-dot" }
                            }
                        }
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
                            ui_layout.batch(|l| l.right_sidebar_visible = false);
                        },
                        "🧵"
                    }
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
                            ui_layout.batch(|l| l.right_sidebar_visible = false);
                        },
                        "📌"
                    }
                    // B.5 drafts toggle dropped — pending drafts now live
                    // inside the agent panel (per-chat).
                    {
                        render_search_tab_button(
                            utility_panel,
                            show_search_filters,
                            true,
                            is_group_channel,
                            is_dm_channel,
                            ui_layout,
                        )
                    }
                    {render_agent_toggle_button(app_state, utility_panel, show_search_filters, is_dm_channel, is_group_channel)}
                    button {
                        class: if member_sidebar_active && utility_panel.read().is_none() { "header-btn soft-active chat-members-toggle-btn chat-header-btn-members" } else { "header-btn chat-members-toggle-btn chat-header-btn-members" },
                        title: if is_dm_channel { t("chat-toggle-contact") } else { t("chat-toggle-members") },
                        onclick: move |_| {
                            utility_panel.set(None);
                            show_search_filters.set(false);
                            // Opening members: close agent panel — collapse 2 writes to 1 batch.
                            ui_layout.batch(|l| {
                                if is_dm_channel || is_group_channel {
                                    l.dm_right_sidebar_visible = true;
                                    l.mobile_dm_contact_detail_visible = false;
                                } else {
                                    let current = l.right_sidebar_visible;
                                    l.right_sidebar_visible = !current;
                                }
                            });
                        },
                        "👥"
                    }
                }
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_chat_utility_rail(
    ctx: ChatViewMarkupCtx,
    panel: ChatUtilityPanel,
    current_channel_name: String,
) -> Element {
    let mut utility_panel = ctx.utility_panel;
    let search_query = ctx.search_query_value.clone();
    let search_terms = ctx.search_terms.clone();
    let search_hits = ctx.search_hits.read().clone();
    let pinned_messages = ctx.pinned_messages.read().clone();
    let search_hit_channel_id = ctx.search_hit_channel_id.clone();
    let search_hit_server = ctx.search_hit_server.clone();
    let pinned_hit_channel_id = ctx.pinned_hit_channel_id.clone();
    let pinned_hit_server = ctx.pinned_hit_server.clone();
    let pinned_hit_channel = ctx.pinned_hit_channel.clone();
    let nav_for_search = ctx.nav_for_search;
    let nav_for_pinned = ctx.nav_for_pinned;
    let nav_state_for_search = ctx.nav;
    let nav_state_for_pinned = ctx.nav;
    let client_manager = ctx.client_manager;
    let chat_view_state = ctx.chat_view_state;
    let app_state = ctx.app_state;
    let notifications_muted = ctx.notifications_muted;
    let pinned_filter_open = ctx.pinned_filter_open;
    let pinned_filter_query = ctx.pinned_filter_query;
    let threads_filter_open = ctx.threads_filter_open;
    let threads_filter_query = ctx.threads_filter_query;
    let search_ui = render_chat_header_search(ctx.clone());

    rsx! {
        ChatUtilityRail {
            panel,
            search_ui,
            search_query,
            search_hits,
            search_terms,
            pinned_messages,
            current_channel_name,
            notifications_muted,
            pinned_filter_open,
            pinned_filter_query,
            threads_filter_open,
            threads_filter_query,
            on_open_search_hit: move |hit: MessageSearchHit| {
                let current_channel_id = search_hit_channel_id.clone();
                let current_server_id = search_hit_server
                    .as_ref()
                    .map(|server| server.id.clone());
                let nav = nav_for_search;
                let nav_state = nav_state_for_search;
                spawn(async move {
                    if let Some((route, message_id)) = open_message_hit(
                            hit,
                            current_channel_id,
                            current_server_id,
                            client_manager,
                            chat_view_state,
                            app_state,
                            nav_state,
                        )
                        .await
                    {
                        nav.push(route);
                        highlight_message(&message_id);
                    }
                });
            },
            on_open_pinned: move |message: poly_client::Message| {
                let Some(active_channel_id) = pinned_hit_channel_id.clone() else {
                    return;
                };
                let server_id = pinned_hit_server.as_ref().map(|server| server.id.clone());
                let hit = MessageSearchHit {
                    channel_id: active_channel_id.clone(),
                    channel_name: pinned_hit_channel
                        .as_ref()
                        .map(|channel| channel.name.clone()),
                    server_id,
                    message,
                };
                let current_server_id = pinned_hit_server
                    .as_ref()
                    .map(|server| server.id.clone());
                let nav = nav_for_pinned;
                let nav_state = nav_state_for_pinned;
                spawn(async move {
                    if let Some((route, message_id)) = open_message_hit(
                            hit,
                            Some(active_channel_id),
                            current_server_id,
                            client_manager,
                            chat_view_state,
                            app_state,
                            nav_state,
                        )
                        .await
                    {
                        nav.push(route);
                        highlight_message(&message_id);
                    }
                });
            },
            on_close: move |_| utility_panel.set(None),
        }
    }
}
