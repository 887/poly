//! Server-list sub-concern for the account server bar.
//!
//! Owns: drag-and-drop ordering helpers, [`AccountServerIcon`], [`ServerIconDisplay`].
//! Each server icon handles its own DnD events and click navigation independently
//! of the account-nav buttons (ISP — server-list concern is separate from account-nav).

use crate::state::BatchedSignal;
use super::super::super::super::routes::Route;
use crate::client_manager::ClientManager;
use crate::state::chat_data::user_color;
use crate::state::{
    AccountSessions, ChatAction, ChatLists, ChatViewState, ContextMenuState,
    DragSource, DragState, NavState, UiOverlays,
};
use crate::ui::context_menu::menus::server_icon_entry_at;
use crate::ui::account::common::chat_history::remember_message_list_scroll_position;
use crate::ui::favorites_sidebar::SidebarTooltip;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Compute the display-ordered server list for an account, respecting saved drag-drop ordering.
pub(super) fn get_ordered_servers(
    account_sessions: &AccountSessions,
    account_id: &str,
    account_servers: &[poly_client::Server],
) -> Vec<poly_client::Server> {
    if let Some(order) = account_sessions.account_server_order.get(account_id) {
        let mut ordered: Vec<_> = order
            .iter()
            .filter_map(|id| account_servers.iter().find(|s| &s.id == id))
            .cloned()
            .collect();
        for s in account_servers {
            if !order.contains(&s.id) {
                ordered.push(s.clone());
            }
        }
        ordered
    } else {
        account_servers.to_vec()
    }
}

/// Compute the new server order after a Bar-2 drag-and-drop reorder.
fn compute_bar2_reorder(
    existing: Option<&Vec<String>>,
    all_servers: &[poly_client::Server],
    account_id: &str,
    drag_id: &str,
    target_id: &str,
) -> Vec<String> {
    let mut order: Vec<String> = existing.cloned().unwrap_or_else(|| {
        all_servers
            .iter()
            .filter(|s| s.account_id == account_id)
            .map(|s| s.id.clone())
            .collect()
    });
    if !order.contains(&drag_id.to_string()) {
        order.push(drag_id.to_string());
    }
    if let Some(from) = order.iter().position(|x| x == drag_id) {
        order.remove(from);
        if let Some(to) = order.iter().position(|x| x == target_id) {
            order.insert(to, drag_id.to_string());
        } else {
            order.push(drag_id.to_string());
        }
    }
    order
}

/// Apply a Bar-2 drop event: update `AccountSessions` with the new server order.
pub(super) fn apply_bar2_drop(
    account_sessions: &mut AccountSessions,
    servers: &[poly_client::Server],
    drag_id: &str,
    target_id: &str,
    account_id: &str,
) {
    let order = compute_bar2_reorder(
        account_sessions.account_server_order.get(account_id),
        servers,
        account_id,
        drag_id,
        target_id,
    );
    account_sessions.account_server_order
        .insert(account_id.to_string(), order);
}

/// A single draggable server icon in the account server bar.
///
/// Handles all drag-and-drop events, right-click context menu, and click navigation.
/// Extracted from the `AccountServerBar` for-loop to keep RSX macros small and
/// avoid Dioxus macro complexity limits inside `for` iterator blocks.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AccountServerIcon(
    server_id: String,
    server_name: String,
    backend_slug: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
    account_id: String,
    unread: u32,
    /// Number of @mention notifications (shown as red badge).
    mention: u32,
    is_selected: bool,
    /// Optional server icon URL. When `Some`, rendered as an `<img>`; when
    /// `None`, falls back to a colored first-letter placeholder.
    icon_url: Option<String>,
) -> Element {
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();
    let nav_state: BatchedSignal<NavState> = use_context();
    let _client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let drag_state: BatchedSignal<DragState> = use_context();

    let is_drag_over = drag_state.read() // poly-lint: allow render-time-read — intentional: re-render when drag_over_id changes to update CSS class live
        .drag_over_id.as_deref() == Some(server_id.as_str());
    let item_class = match (is_selected, is_drag_over) {
        (true, true) => "server-icon active drag-over-target",
        (true, false) => "server-icon active",
        (false, true) => "server-icon drag-over-target",
        (false, false) => "server-icon",
    };

    // Pre-build closures to keep the RSX block compact.
    let sid_ctx = server_id.clone();
    let sname_ctx = server_name.clone();
    let aid_ctx = account_id.clone();
    let iid_ctx = instance_id.clone();
    let bslug_ctx = backend_slug.clone();
    let on_context_menu = move |evt: Event<MouseData>| {
        evt.prevent_default();
        evt.stop_propagation();
        let coords = evt.client_coordinates();
        ui_overlays.batch(|o| {
            o.context_menu_stack.push(server_icon_entry_at(
                &ContextMenuState {
                    x: coords.x,
                    y: coords.y,
                    server_id: sid_ctx.clone(),
                    server_name: sname_ctx.clone(),
                    account_id: aid_ctx.clone(),
                    instance_id: iid_ctx.clone(),
                    backend_slug: bslug_ctx.clone(),
                },
                coords.x,
                coords.y,
            ));
        });
    };

    let sid_ds = server_id.clone();
    let on_drag_start = move |_: Event<DragData>| {
        drag_state.batch(|d| {
            d.dragging_server_id = Some(sid_ds.clone());
            d.drag_source = DragSource::AccountServer;
        });
    };

    let sid_do = server_id.clone();
    let on_drag_over = move |evt: Event<DragData>| {
        evt.prevent_default();
        evt.stop_propagation();
        drag_state.batch(|d| d.drag_over_id = Some(sid_do.clone()));
    };

    let sid_dl = server_id.clone();
    let on_drag_leave = move |_: Event<DragData>| {
        if drag_state.read() // poly-lint: allow render-time-read — inside event closure, not render body
            .drag_over_id.as_deref() == Some(sid_dl.as_str()) {
            drag_state.batch(|d| d.drag_over_id = None);
        }
    };

    let tid = server_id.clone();
    let aid_drop = account_id.clone();
    let on_drop = move |evt: Event<DragData>| {
        evt.prevent_default();
        evt.stop_propagation();
        // Snapshot and clear drag state before mutating chat_data.
        let (drag_id, src) = drag_state.batch(|d| {
            let drag_id = d.dragging_server_id.clone();
            let src = d.drag_source.clone();
            *d = DragState::default();
            (drag_id, src)
        });
        let Some(drag_id) = drag_id else {
            return;
        };
        if matches!(src, DragSource::AccountServer) && drag_id != tid {
            let servers_snapshot = chat_lists.read() // poly-lint: allow render-time-read — inside event closure, not render body
                .servers.clone();
            account_sessions.batch(|as_| apply_bar2_drop(as_, &servers_snapshot, &drag_id, &tid, &aid_drop));
        }
    };

    let on_drag_end = move |_: Event<DragData>| {
        drag_state.batch(|d| { *d = DragState::default(); });
    };

    let sid_click = server_id.clone();
    let bslug_click = backend_slug.clone();
    let aid_click = account_id.clone();
    let on_click = move |_: Event<MouseData>| {
        if let Some(previous_channel_id) = nav_state.read() // poly-lint: allow render-time-read — inside click closure, not render body
            .selected_channel.cloned() {
            remember_message_list_scroll_position(&previous_channel_id);
        }
        // Clear per-server transient data synchronously before the route change so
        // that `ServerHome` never sees stale `current_channel` / `current_server`
        // from a previous server or from demo data. Without this, the stale channel
        // type can flip `ServerHome` into rendering `VoiceChannelView` even before
        // `load_server_data` fires, which requests audio permission and hard-crashes
        // Chromium on Linux.
        chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
        chat_lists.batch(|cl| cl.set_channels(Vec::new()));
        // NOTE: do NOT spawn `load_server_data` here even when not in mobile
        // drawer context. `ServerHome::use_effect` (via `use_spawn_once`)
        // already kicks off the load when the route mounts. Spawning a second
        // copy from this click handler caused the Teams server-switch hang
        // (2026-04-25): two concurrent loaders racing on the same `chat_data`
        // signal compounded with the loading=true→false toggles from each,
        // re-firing every chat_data subscriber and starving the WASM
        // scheduler. The `mobile_left_drawer_open()` distinction also needs
        // to live in the route effect itself, not here, so there's exactly
        // one source of truth for "what gets loaded for this server".
        crate::nav!(Route::ServerHome {
            backend: bslug_click.clone(),
            instance_id: instance_id.clone(),
            account_id: aid_click.clone(),
            server_id: sid_click.clone(),
        });
    };

    rsx! {
        div {
            class: "{item_class}",
            draggable: "true",
            oncontextmenu: on_context_menu,
            ondragstart: on_drag_start,
            ondragover: on_drag_over,
            ondragleave: on_drag_leave,
            ondrop: on_drop,
            ondragend: on_drag_end,
            onclick: on_click,

            ServerIconDisplay {
                icon_url: icon_url.clone(),
                server_name: server_name.clone(),
                server_id: server_id.clone(),
                unread,
                mention,
            }
            SidebarTooltip {
                line1: server_name.clone(),
                line2: Some(backend_slug.clone()),
                line3: None,
            }
        }
    }
}

/// Renders the visual content of a server icon: image (or letter fallback) plus notification badges.
///
/// Shows a red `@{mention}` badge for direct @mentions, and a small unread dot
/// when there are unread messages but no direct mentions.
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn ServerIconDisplay(
    icon_url: Option<String>,
    server_name: String,
    server_id: String,
    unread: u32,
    /// Number of @mention notifications. When > 0, shows a red @badge.
    mention: u32,
) -> Element {
    let first_letter: String = server_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let icon_color = user_color(&server_id);
    rsx! {
        if let Some(ref url) = icon_url {
            img {
                class: "server-icon-image",
                src: "{url}",
                alt: "{server_name}",
            }
        } else {
            div {
                class: "server-icon-letter",
                style: "background-color: {icon_color};",
                "{first_letter}"
            }
        }
        // @mention badge (red): only for direct @mentions.
        if mention > 0 {
            span { class: "badge mention-count-badge", "@{mention}" }
        } else if unread > 0 {
            // Unread-but-not-mentioned: show count (same as favorites bar for consistency)
            span { class: "badge mention-count-badge", "{unread}" }
        }
    }
}
