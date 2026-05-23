//! Server channel view — categories, channel rows, and the server banner.
//!
//! ## B.4 — Category/permission filtering (Single Responsibility)
//!
//! The concern "which channels belong to which category, which channels
//! are uncategorized, and which backends support channel-creation" is now
//! isolated in [`ServerChannelFilter`].  `ServerChannelView` reads its
//! result but does NOT implement the filtering logic itself.
//!
//! ## Components
//! - `ServerBanner` — clickable server-name header + dropdown + invite.
//! - `ServerChannelView` — category list / forge sidebar / forum sidebar.
//! - `ChannelsRolesPanel` — demo-only category visibility picker.
//!
//! ## Async helpers
//! - `load_channel_data` — fetches messages + members for a text channel
//!   or voice participants for a voice/video channel.

use super::items::CategorySection;
use super::ChannelListAction;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::BatchedSignal;
use crate::state::{ChatLists, ChatViewState, NavState, UserPrefs, VoiceState};
use crate::ui::account::common::chat_history::{
    initial_message_query, read_channel_view_anchor, remember_message_list_scroll_position,
    request_restore_scroll_position_or_bottom, request_restore_to_anchor,
};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{ChannelType, Server};
use poly_ui_macros::{context_menu, ui_action};

// ── B.4 helper: category / permission filtering ──────────────────────────────

/// Pure-data result of categorising the server's channel list.
///
/// Computing this in a dedicated struct keeps `ServerChannelView`'s render
/// body free of filtering loops (SRP).  The struct is cheaply constructible
/// and carries no Signals — it's a one-shot snapshot per render.
struct ServerChannelFilter {
    /// Channel IDs that appear in at least one category.
    pub categorized_ids: Vec<String>,
    /// Channel IDs present in the backend list but absent from every category.
    pub uncategorized_ids: Vec<String>,
    /// Back-end slug for route construction.
    pub backend_slug: String,
    /// Whether the current backend is HackerNews.
    pub is_hn: bool,
    /// Whether the current backend is GitHub.
    pub is_github: bool,
    /// Whether the current backend is a forge (GitHub or Forgejo).
    pub is_forge: bool,
    /// Whether creating a new channel is allowed.
    pub can_create: bool,
}

impl ServerChannelFilter {
    /// Build the filter from a snapshot of server + channels.
    fn from_server(server: &Server, channels: &[poly_client::Channel]) -> Self {
        let categorized_ids: Vec<String> = server
            .categories
            .iter()
            .flat_map(|cat| cat.channel_ids.iter().cloned())
            .collect();

        let uncategorized_ids: Vec<String> = channels
            .iter()
            .filter(|ch| !categorized_ids.contains(&ch.id))
            .map(|ch| ch.id.clone())
            .collect();

        let backend_slug = server.backend.slug().to_string();
        let is_hn = backend_slug == "hackernews";
        let is_github = backend_slug == "github";
        let is_forge = backend_slug == "github" || backend_slug == "forgejo";
        // Read-only and demo backends do not support channel creation.
        let can_create = server.backend != "demo" && !is_hn && !is_github && !is_forge;

        Self {
            categorized_ids,
            uncategorized_ids,
            backend_slug,
            is_hn,
            is_github,
            is_forge,
            can_create,
        }
    }
}

// ── Async loader (hang-class #4 safe — uses read_with_timeout) ───────────────

/// Load messages, members, and voice participants for a channel.
///
/// The in-tree CLAUDE.md comments at the original lines 193-195 and 360-364
/// document why `tokio::time::timeout(Duration, backend.read())` panics on
/// `wasm32-unknown-unknown` (`Instant::now()` is unimplemented). This async
/// fn uses `BackendHandleExt::read_with_timeout` (hang-class #4
/// countermeasure) everywhere instead.
pub(super) async fn load_channel_data(
    channel_id: String,
    client_manager: BatchedSignal<ClientManager>,
    nav: BatchedSignal<NavState>,
    voice_state: BatchedSignal<VoiceState>,
    chat_view_state: BatchedSignal<ChatViewState>,
) {
    // Fire an initial spinner cascade so the UI paints "loading" before we
    // start awaiting.  Every subsequent mutation is deferred into a single
    // terminal PendingUpdate::apply().
    chat_view_state.batch(|cv| cv.loading = true);

    let unread_count = chat_view_state
        .peek()
        .current_channel
        .as_ref()
        .filter(|channel| channel.id == channel_id)
        .map_or(0, |channel| channel.unread_count);

    // Get selected server to find the right backend
    let server_id = nav.read().selected_server.cloned();
    let Some(server_id) = server_id else {
        chat_view_state.batch(|cv| cv.loading = false);
        return;
    };

    let channel_type = chat_view_state
        .peek()
        .current_channel
        .as_ref()
        .map(|ch| ch.channel_type);

    let backend_info = client_manager.peek().get_backend_for_server(&server_id);
    let Some((_account_id, backend)) = backend_info else {
        chat_view_state.batch(|cv| cv.loading = false);
        return;
    };

    // WASM-safe timeout: BackendHandleExt::read_with_timeout uses
    // gloo_timers on WASM instead of tokio::time::timeout which panics
    // (Instant::now() unimplemented on wasm32-unknown-unknown).
    // See original channel_list.rs comments at lines 193-195, 360-364.
    let guard = match backend
        .read_with_timeout(std::time::Duration::from_secs(5))
        .await
    {
        Ok(g) => g,
        Err(_) => {
            tracing::warn!(channel_id = %channel_id, "load_channel_data: backend read timed out");
            chat_view_state.batch(|cv| cv.loading = false);
            return;
        }
    };
    let mut pending = chat_view_state.pending_update();

    match channel_type {
        Some(poly_client::ChannelType::Voice) | Some(poly_client::ChannelType::Video) => {
            // Voice/video channel — load participant list from backend
            if let Ok(participants) = guard.get_voice_participants(&channel_id).await {
                let chid = channel_id.clone();
                voice_state.batch(move |v| {
                    v.voice_channel_participants.insert(chid, participants);
                });
            }
        }
        _ => {
            // Text channel — load messages and members.
            // If a scrollend-saved anchor exists for this channel, load around that
            // message so the user returns to approximately where they were reading.
            let anchor = read_channel_view_anchor(&channel_id).await;
            let query = if let Some((_, ref msg_id, _)) = anchor {
                poly_client::MessageQuery {
                    around: Some(msg_id.clone()),
                    limit: Some(initial_message_query(unread_count).limit.unwrap_or(36)),
                    ..Default::default()
                }
            } else {
                initial_message_query(unread_count)
            };
            let anchor_for_scroll = anchor.clone();
            if let Ok(messages) = guard.get_messages(&channel_id, query).await {
                let had_anchor = anchor.is_some();
                pending.set(move |cv| {
                    cv.set_messages(messages);
                    cv.messages_loaded_via_anchor = had_anchor;
                });
                if let Some((ref element_id, _, offset_px)) = anchor_for_scroll {
                    request_restore_to_anchor(&channel_id, element_id, offset_px);
                } else {
                    request_restore_scroll_position_or_bottom(&channel_id);
                }
            }
            let members = guard.get_channel_members(&channel_id).await.ok();
            if let Some(mbrs) = members {
                pending.set(move |cv| cv.members = mbrs);
            }
        }
    }

    pending.set(|cv| cv.loading = false);
    pending.apply();
}

// ── Components ────────────────────────────────────────────────────────────────

/// Discord-style server banner — top of the channel list sidebar.
///
/// Shows:
/// - **DMs view:** simple "Direct Messages" heading.
/// - **Server view:**
///   - Optional full-width banner image (when `server.banner_url` is `Some`).
///   - Header bar with a clickable server-name button (opens dropdown) and an
///     inline invite-people button on the right.
///   - Dropdown menu: Server Settings, ──, Invite People, Notification
///     Settings, ──, Leave Server.
///
/// The dropdown is closed by clicking the transparent `.context-menu-backdrop`
/// overlay that covers the full viewport beneath the panel.
// DECISION(DX): reuses the context-menu-backdrop/context-menu CSS pattern
// established in phase-2.10 so we don't need new z-index layers.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(ChannelListAction)]
#[component]
pub(super) fn ServerBanner(
    current_view: crate::state::View,
    current_server: Option<Server>,
    visible_category_ids: Signal<Vec<String>>,
) -> Element {
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let mut dropdown_open = use_signal(|| false);
    let mut channels_roles_open = use_signal(|| false);

    // Derive route-construction fields from AppState before entering RSX so
    // that we don't hold a borrow of `app_state` inside closures that also
    // mutate `dropdown_open`.
    let instance_id = nav
        .read()
                .active_instance_id
        .cloned()
        .unwrap_or_default();
    let account_id = nav
        .read()
                .active_account_id
        .cloned()
        .unwrap_or_default();
    let server_id = nav
        .read()
                .selected_server
        .cloned()
        .unwrap_or_default();

    // Backend slug comes from the Server struct itself (always consistent with
    // what was used to navigate here).
    let backend_slug = current_server
        .as_ref()
        .map(|s| s.backend.slug().to_string())
        .unwrap_or_default();
    let supports_channels_roles = current_server
        .as_ref()
        .is_some_and(|server| server.backend.as_str() == "demo");

    rsx! {
        div { class: "server-banner-sidebar",
            // ── Transparent click-catcher to close the dropdown ──────────────
            if *dropdown_open.read() {
                div {
                    class: "context-menu-backdrop",
                    onclick: move |_| dropdown_open.set(false),
                }
            }

            if current_view == crate::state::View::DmsFriends {
                // ── DMs / Friends view: plain heading ────────────────────────
                div { class: "server-banner-header",
                    h3 { "{t(\"nav-dms\")}" }
                }
            } else if let Some(ref server) = current_server {
                // ── Server view ──────────────────────────────────────────────
                if let Some(ref url) = server.banner_url {
                    div { class: "server-banner-hero",
                        img {
                            class: "server-banner-img",
                            src: "{url}",
                            alt: "",
                            draggable: false,
                        }
                        div { class: "server-banner-overlay",
                            div { class: "server-banner-header server-banner-header-overlay",
                                button {
                                    class: "server-name-trigger",
                                    onclick: move |_| {
                                        let open = *dropdown_open.read();
                                        dropdown_open.set(!open);
                                    },
                                    span { class: "server-name-text", "{server.name}" }
                                    if *dropdown_open.read() {
                                        span { class: "server-name-chevron", "▴" }
                                    } else {
                                        span { class: "server-name-chevron", "▾" }
                                    }
                                }
                            }
                            if supports_channels_roles {
                                button {
                                    class: "server-channels-roles-btn",
                                    onclick: move |_| {
                                        let open = *channels_roles_open.read();
                                        channels_roles_open.set(!open);
                                    },
                                    span { class: "server-channels-roles-icon", "☰" }
                                    span { "{t(\"server-banner-channels-roles\")}" }
                                }
                            }
                        }
                    }
                } else {
                    div { class: "server-banner-header",
                        button {
                            class: "server-name-trigger",
                            onclick: move |_| {
                                let open = *dropdown_open.read();
                                dropdown_open.set(!open);
                            },
                            span { class: "server-name-text", "{server.name}" }
                            if *dropdown_open.read() {
                                span { class: "server-name-chevron", "▴" }
                            } else {
                                span { class: "server-name-chevron", "▾" }
                            }
                        }
                    }
                    if supports_channels_roles {
                        div { class: "server-banner-secondary-action",
                            button {
                                class: "server-channels-roles-btn server-channels-roles-btn-flat",
                                onclick: move |_| {
                                    let open = *channels_roles_open.read();
                                    channels_roles_open.set(!open);
                                },
                                span { class: "server-channels-roles-icon", "☰" }
                                span { "{t(\"server-banner-channels-roles\")}" }
                            }
                        }
                    }
                }

                // Dropdown panel (positioned absolutely over the sidebar).
                if *dropdown_open.read() {
                    nav { class: "server-dropdown-menu",
                        Link {
                            class: "server-dropdown-item",
                            to: Route::ServerSettingsRoute {
                                backend: backend_slug.clone(),
                                instance_id: instance_id.clone(),
                                account_id: account_id.clone(),
                                server_id: server_id.clone(),
                            },
                            onclick: move |_| dropdown_open.set(false),
                            "{t(\"server-banner-settings\")}"
                        }
                        div { class: "context-menu-separator" }
                        button {
                            class: "server-dropdown-item",
                            onclick: move |_| {
                                // TODO(phase-3): open Invite People modal.
                                tracing::info!("Invite People clicked — placeholder");
                                dropdown_open.set(false);
                            },
                            "{t(\"server-banner-invite\")}"
                        }
                        button {
                            class: "server-dropdown-item",
                            onclick: move |_| {
                                // TODO(phase-3): open per-server notification settings.
                                tracing::info!("Notification Settings clicked — placeholder");
                                dropdown_open.set(false);
                            },
                            "{t(\"server-banner-notif-settings\")}"
                        }
                        div { class: "context-menu-separator" }
                        button {
                            class: "server-dropdown-item server-dropdown-item-danger",
                            onclick: move |_| {
                                // TODO(phase-3): hook to the leave-server confirmation flow.
                                tracing::info!("Leave Server clicked — placeholder");
                                dropdown_open.set(false);
                            },
                            "{t(\"server-banner-leave\")}"
                        }
                    }
                }

                if supports_channels_roles && *channels_roles_open.read() {
                    ChannelsRolesPanel { server: server.clone(), visible_category_ids }
                }
            } else {
                // ── Fallback (no server selected) ────────────────────────────
                div { class: "server-banner-header",
                    h3 { "{t(\"nav-dms\")}" }
                }
            }
        }
    }
}

/// Server channel view — categories and channels.
///
/// Delegates category/permission filtering to [`ServerChannelFilter`] (B.4),
/// then renders the appropriate sidebar variant:
/// - Discord-style category sections (text/voice servers),
/// - Forge sidebar (GitHub / Forgejo),
/// - Forum sidebar (Lemmy-style).
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ServerChannelView(visible_category_ids: Signal<Vec<String>>) -> Element {
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let user_prefs: crate::state::BatchedSignal<UserPrefs> = use_context();
    let _client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();

    let current_server = chat_view_state.read().current_server.clone(); // poly-lint: allow render-time-read — render snapshot; subscription intentional

    // Derive route construction fields for InlineCreateChannel.
    let instance_id = nav.read().active_instance_id.cloned().unwrap_or_default();
    let account_id  = nav.read().active_account_id.cloned().unwrap_or_default();

    if let Some(ref server) = current_server {
        // B.4: category/permission filtering is a separate concern — delegate to helper.
        let channels_snap = chat_lists.peek();
        let filter = ServerChannelFilter::from_server(server, &channels_snap.channels);
        drop(channels_snap); // release peek before entering RSX signal reads

        let server_id = server.id.clone();
        let backend_slug = filter.backend_slug.clone();

        // Is the current channel a (Lemmy-style) forum channel?
        // HackerNews uses its own sidebar, so it is excluded here.
        let current_ch_type = chat_view_state.read().current_channel.as_ref() // poly-lint: allow render-time-read — render snapshot; subscription intentional
            .map(|ch| ch.channel_type);
        let is_forum = matches!(current_ch_type, Some(ChannelType::Forum));
        let current_channel_id = chat_view_state.read().current_channel.as_ref() // poly-lint: allow render-time-read — render snapshot; subscription intentional
            .map(|ch| ch.id.clone())
            .unwrap_or_default();

        let current_route = use_route::<Route>();
        let on_comments = matches!(current_route, Route::ForumCommentsRoute { .. });

        rsx! {
            // Discord-style categories: shown for all backends, including HN.
            // Hidden for Lemmy/forum and forge backends whose sidebar replaces categories.
            if !is_forum && !filter.is_forge {
                if !filter.uncategorized_ids.is_empty() {
                    CategorySection {
                        cat_name: t("channel-list-text-channels"),
                        cat_channel_ids: filter.uncategorized_ids,
                    }
                }
                for category in &server.categories {
                    if visible_category_ids.read().is_empty()
                        || visible_category_ids.read().contains(&category.id)
                    {
                        CategorySection {
                            cat_name: category.name.clone(),
                            cat_channel_ids: category.channel_ids.clone(),
                        }
                    }
                }
                // HN-specific footer: Algolia search link.
                if filter.is_hn {
                    a {
                        class: "hn-algolia-link",
                        href: "https://hn.algolia.com/",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "🔍 Search on Algolia"
                    }
                }
            }
            // Forge (GitHub/Forgejo) sidebar: Issues / Pull Requests / Code channel links.
            if filter.is_forge {
                div { class: "forge-sidebar-channels",
                    for ch in chat_lists.peek().channels.iter().filter(|c| c.server_id == server_id) {
                        {
                            let ch_id = ch.id.clone();
                            // Use nav.selected_channel (updated synchronously by
                            // sync_route_to_app_state) instead of chat_data.current_channel
                            // which lags behind the route on same-server channel switches.
                            let nav_selected = nav.read().selected_channel.cloned().unwrap_or_default();
                            let is_active = ch_id == nav_selected;
                            let icon = match ch.channel_type {
                                ChannelType::Forum => match ch.name.as_str() {
                                    "pull-requests" => "🔀",
                                    "discussions" => "💬",
                                    // "issues" + everything else falls through to a
                                    // notepad — issues aren't always bugs (feature
                                    // requests, tasks, RFCs).
                                    _ => "📋",
                                },
                                ChannelType::Code => "📁",
                                ChannelType::Text
                                | ChannelType::Voice
                                | ChannelType::Video
                                | ChannelType::HackerNews
                                | ChannelType::Thread
                                | ChannelType::Announcement => "#",
                            };
                            let label = match ch.name.as_str() {
                                "issues" => "Issues",
                                "pull-requests" => "Pull Requests",
                                "discussions" => "Discussions",
                                "code" => "Code",
                                other => other,
                            };
                            rsx! {
                                Link {
                                    class: if is_active { "forge-channel-item active" } else { "forge-channel-item" },
                                    to: Route::ServerChat {
                                        backend: backend_slug.clone(),
                                        instance_id: instance_id.clone(),
                                        account_id: account_id.clone(),
                                        server_id: server_id.clone(),
                                        channel_id: ch_id,
                                    },
                                    span { class: "forge-channel-icon", "{icon}" }
                                    span { class: "forge-channel-label", "{label}" }
                                }
                            }
                        }
                    }
                }
            }
            // Forum (Lemmy-style) sidebar: Posts/Comments tabs + scope filters.
            else if is_forum && !filter.is_forge {
                div { class: "forum-sidebar-controls",
                    // Posts / Comments nav tabs
                    div { class: "forum-nav-tabs",
                        Link {
                            class: if !on_comments { "forum-nav-tab active" } else { "forum-nav-tab" },
                            to: Route::ServerChat {
                                backend: backend_slug.clone(),
                                instance_id: instance_id.clone(),
                                account_id: account_id.clone(),
                                server_id: server_id.clone(),
                                channel_id: current_channel_id.clone(),
                            },
                            "Posts"
                        }
                        Link {
                            class: if on_comments { "forum-nav-tab active" } else { "forum-nav-tab" },
                            to: Route::ForumCommentsRoute {
                                backend: backend_slug.clone(),
                                instance_id: instance_id.clone(),
                                account_id: account_id.clone(),
                                server_id: server_id.clone(),
                                channel_id: current_channel_id.clone(),
                            },
                            "Comments"
                        }
                    }
                    // Scope filter — stacked vertically. onclick updates
                    // `AppState.forum_scope` which re-keys `ForumView`'s
                    // `ClientView` mount so `get_view_rows` re-fetches with
                    // the new tab_id. read() (not peek!) so the active class
                    // re-renders when the user clicks one of the buttons —
                    // peek() left the previous active button highlighted.
                    {
                        let scope = user_prefs.read().forum_scope.clone();
                        let cls_sub = if scope == "subscribed" { "forum-filter-btn active forum-filter-full" } else { "forum-filter-btn forum-filter-full" };
                        let cls_loc = if scope == "local"      { "forum-filter-btn active forum-filter-full" } else { "forum-filter-btn forum-filter-full" };
                        let cls_all = if scope == "all"        { "forum-filter-btn active forum-filter-full" } else { "forum-filter-btn forum-filter-full" };
                        rsx! {
                            button {
                                class: "{cls_sub}",
                                r#type: "button",
                                onclick: move |_| { user_prefs.batch(|p| p.forum_scope = "subscribed".to_string()); },
                                "Subscribed"
                            }
                            button {
                                class: "{cls_loc}",
                                r#type: "button",
                                onclick: move |_| { user_prefs.batch(|p| p.forum_scope = "local".to_string()); },
                                "Local"
                            }
                            button {
                                class: "{cls_all}",
                                r#type: "button",
                                onclick: move |_| { user_prefs.batch(|p| p.forum_scope = "all".to_string()); },
                                "All"
                            }
                        }
                    }
                    // Show hidden toggle
                    button { class: "forum-filter-btn forum-filter-full forum-filter-text",
                        title: "Toggle hidden posts",
                        "Show hidden posts"
                    }
                    Link {
                        class: "forum-create-post-btn",
                        "data-testid": "forum-composer-new-post-btn",
                        to: Route::CreateForumPostRoute {
                            backend: backend_slug,
                            instance_id,
                            account_id,
                            server_id,
                            channel_id: current_channel_id,
                        },
                        span { "+" }
                        span { "Create Post" }
                    }
                }
            } else if filter.can_create {
                // "+ New Channel" link → full-page CreateChannelRoute (non-demo, non-HN only).
                Link {
                    class: "channel-create-btn",
                    to: Route::CreateChannelRoute {
                        backend: filter.backend_slug,
                        instance_id,
                        account_id,
                        server_id,
                    },
                    span { class: "channel-create-btn-icon", "+" }
                    span { "{t(\"create-channel-btn\")}" }
                }
            }
        }
    } else {
        rsx! {}
    }
}

/// Demo-only panel to opt into category visibility, inspired by Discord's
/// Channels & Roles onboarding surface.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ChannelsRolesPanel(server: Server, mut visible_category_ids: Signal<Vec<String>>) -> Element {
    let all_ids: Vec<String> = server.categories.iter().map(|c| c.id.clone()).collect();

    rsx! {
        div { class: "server-channels-roles-panel",
            div { class: "server-channels-roles-panel-header",
                h4 { "{t(\"server-banner-channels-roles\")}" }
                span { class: "server-channels-roles-subtitle", "{t(\"server-banner-browse-channels\")}" }
            }
            div { class: "server-channels-roles-list",
                for category in &server.categories {
                    {
                        let checked = visible_category_ids.read().is_empty()
                            || visible_category_ids.read().contains(&category.id);
                        let category_id = category.id.clone();
                        let all_ids_for_toggle = all_ids.clone();
                        rsx! {
                            label { class: "server-channels-role-row",
                                input {
                                    r#type: "checkbox",
                                    checked,
                                    onchange: move |evt| {
                                        let mut next = if visible_category_ids.read().is_empty() {
                                            all_ids_for_toggle.clone()
                                        } else {
                                            visible_category_ids.read().clone()
                                        };
                                        if evt.checked() {
                                            if !next.contains(&category_id) {
                                                next.push(category_id.clone());
                                            }
                                        } else {
                                            next.retain(|id| id != &category_id);
                                        }
                                        visible_category_ids.set(next);
                                    },
                                }
                                div { class: "server-channels-role-copy",
                                    span { class: "server-channels-role-name", "{category.name}" }
                                    span { class: "server-channels-role-meta",
                                        "{category.channel_ids.len()} {t(\"server-banner-channel-count\")}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
