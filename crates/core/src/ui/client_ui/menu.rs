//! Host component that renders plugin-declared context menu items.
//!
//! Consumes `Vec<MenuItem>` and renders via the existing `ContextMenuItem`
//! primitive from [`crate::ui::account::server::context_menu`].
//!
//! ## Prop shape (WP 2.A)
//!
//! - `target: MenuTargetKind`, `target_id: String`, `account_id: String` —
//!   target description. `account_id` selects the `ClientBackend` via
//!   [`crate::client_manager::ClientManager::get_backend`].
//! - Items are fetched internally on each render via `use_resource` — D24
//!   says fresh fetch on every menu open, so caller should remount us.
//!
//! ## Error handling (D26)
//!
//! If the backend query errors, `ClientMenu` renders ONE disabled info row
//! with hardcoded label `"plugin error: failed to load items"`. Host-universal
//! items (Copy ID, Leave, Favorites, …) continue rendering because they live
//! in the calling `ServerContextMenu`, not here.
//!
//! ## Submenus (D6 decision)
//!
//! WIT records cannot be recursive, so the plugin sends a flat `Vec<MenuItem>`
//! where children carry `parent_id = Some(parent.id)`. We reconstruct the
//! tree locally. Items whose `parent_id` points at an unknown id are dropped
//! with `tracing::warn!`.

use crate::client_manager::ClientManager;
use crate::ui::account::server::context_menu::ContextMenuItem;
use dioxus::prelude::*;
use poly_client::{
    ActionOutcome, ClientError, MenuItem, MenuItemVariant, MenuSlot, MenuTargetKind,
};
use poly_ui_macros::{context_menu, ui_action};
use std::collections::HashMap;

/// Plugin-declared context menu items, rendered inside a parent context menu.
///
/// This component fetches its own data from the `ClientBackend` for
/// `account_id` every time it renders (D24 — no caching). The caller is
/// expected to mount it only while the surrounding menu is open.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ClientMenu(
    target: MenuTargetKind,
    target_id: String,
    account_id: String,
) -> Element {
    let client_manager: Signal<ClientManager> = use_context();

    // D24: fresh fetch on every mount. `use_resource` re-runs when any of
    // its captured signals change; we capture the props so an outer
    // remount (different target) re-fetches.
    let items_res = {
        let account_id = account_id.clone();
        let target_id = target_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            let target_id = target_id.clone();
            async move {
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = backend.read().await;
                guard.get_context_menu_items(target, &target_id).await
            }
        })
    };

    match &*items_res.read_unchecked() {
        None => {
            // First render — still loading.
            rsx! {
                div { class: "context-menu-item disabled",
                    span { "loading…" }
                }
            }
        }
        Some(Err(err)) => {
            // D26: single disabled info row. FTL key polish is a future task.
            tracing::warn!("ClientMenu: plugin fetch failed: {err:?}");
            rsx! {
                div {
                    class: "context-menu-item disabled context-menu-info-stub",
                    span { "plugin error: failed to load items" }
                }
            }
        }
        Some(Ok(items)) => {
            let items = items.clone();
            render_grouped(items, target, target_id.clone(), account_id.clone())
        }
    }
}

/// Render the flat `items` list as slot-grouped top-level items, with
/// host separators inserted between occupied slots.
fn render_grouped(
    items: Vec<MenuItem>,
    target: MenuTargetKind,
    target_id: String,
    account_id: String,
) -> Element {
    // Index all items by id so we can look up children.
    let ids: std::collections::HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();

    // Collect children by parent_id.
    let mut children_by_parent: HashMap<String, Vec<MenuItem>> = HashMap::new();
    let mut top_level: Vec<MenuItem> = Vec::new();
    for item in &items {
        match &item.parent_id {
            None => top_level.push(item.clone()),
            Some(pid) => {
                if ids.contains(pid.as_str()) {
                    children_by_parent
                        .entry(pid.clone())
                        .or_default()
                        .push(item.clone());
                } else {
                    tracing::warn!(
                        "ClientMenu: dropping item {:?} with unknown parent_id {:?}",
                        item.id,
                        pid
                    );
                }
            }
        }
    }

    // Group top-level by slot, preserving declared order within each slot.
    let slot_order = [
        MenuSlot::Top,
        MenuSlot::AfterFavorites,
        MenuSlot::BeforeLeave,
        MenuSlot::Bottom,
    ];
    let mut by_slot: HashMap<&'static str, Vec<MenuItem>> = HashMap::new();
    for item in top_level {
        let key = slot_key(item.slot);
        by_slot.entry(key).or_default().push(item);
    }

    let mut rendered_slots: Vec<Element> = Vec::new();
    let mut first = true;
    for slot in slot_order {
        let key = slot_key(slot);
        let Some(slot_items) = by_slot.remove(key) else {
            continue;
        };
        if slot_items.is_empty() {
            continue;
        }
        if !first {
            rendered_slots.push(rsx! {
                div { class: "context-menu-separator" }
            });
        }
        first = false;
        for item in slot_items {
            let children = children_by_parent
                .get(&item.id)
                .cloned()
                .unwrap_or_default();
            rendered_slots.push(render_item(
                item,
                children,
                target,
                target_id.clone(),
                account_id.clone(),
            ));
        }
    }

    rsx! {
        {rendered_slots.into_iter()}
    }
}

fn slot_key(s: MenuSlot) -> &'static str {
    match s {
        MenuSlot::Top => "top",
        MenuSlot::AfterFavorites => "after-favorites",
        MenuSlot::BeforeLeave => "before-leave",
        MenuSlot::Bottom => "bottom",
    }
}

/// Render a single top-level item (or a submenu child, recursively).
fn render_item(
    item: MenuItem,
    children: Vec<MenuItem>,
    target: MenuTargetKind,
    target_id: String,
    account_id: String,
) -> Element {
    match item.item_variant {
        MenuItemVariant::Normal => render_leaf(item, false, target, target_id, account_id),
        MenuItemVariant::Destructive => render_leaf(item, true, target, target_id, account_id),
        MenuItemVariant::SubmenuHeader => {
            render_submenu(item, children, target, target_id, account_id)
        }
        MenuItemVariant::InfoBlock => render_info_block(item),
    }
}

fn render_leaf(
    item: MenuItem,
    danger: bool,
    target: MenuTargetKind,
    target_id: String,
    account_id: String,
) -> Element {
    // FTL resolution belongs to the plugin's own bundle (not yet wired —
    // docs/plans/plan-client-ui-surface.md §4.3). For now display the key
    // itself so authors see it.
    let label = item.label_key.clone();
    let action_id = item.id.clone();
    let onclick = move |_evt: MouseEvent| {
        let action_id = action_id.clone();
        let target_id = target_id.clone();
        let account_id = account_id.clone();
        spawn(async move {
            dispatch_action(account_id, action_id, target, target_id).await;
        });
    };
    rsx! {
        ContextMenuItem {
            label,
            danger,
            onclick,
        }
    }
}

fn render_submenu(
    item: MenuItem,
    children: Vec<MenuItem>,
    target: MenuTargetKind,
    target_id: String,
    account_id: String,
) -> Element {
    // Local signal — hover to open, leave to close.
    let mut open = use_signal(|| false);
    let label = item.label_key.clone();

    // Pre-render the nested items so the submenu body is static rsx.
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in &children {
        ids.insert(c.id.clone());
    }
    // Submenus from a flat list: a child whose parent is `item.id` is a direct
    // child. Grandchildren (parent_id == some child.id) are not expanded here;
    // WP 2 restricts menus to one level of nesting — deeper nesting would need
    // another pass through render_grouped. Drop with warn for now.
    let direct: Vec<MenuItem> = children
        .into_iter()
        .filter(|c| c.parent_id.as_deref() == Some(item.id.as_str()))
        .collect();

    let mut rendered_children: Vec<Element> = Vec::new();
    for child in direct {
        if child.item_variant == MenuItemVariant::SubmenuHeader {
            tracing::warn!(
                "ClientMenu: nested submenus beyond one level are not supported \
                 (parent={}, child={}); rendering as leaf",
                item.id,
                child.id,
            );
        }
        rendered_children.push(render_item(
            child,
            Vec::new(),
            target,
            target_id.clone(),
            account_id.clone(),
        ));
    }

    rsx! {
        div {
            class: "context-menu-submenu-wrap",
            style: "position: relative;",
            onmouseenter: move |_| open.set(true),
            onmouseleave: move |_| open.set(false),
            ContextMenuItem {
                label,
                has_arrow: true,
                onclick: move |_| open.set(true),
            }
            if open() {
                div {
                    class: "context-menu",
                    style: "position: absolute; left: 100%; top: 0;",
                    onclick: move |evt| evt.stop_propagation(),
                    {rendered_children.into_iter()}
                }
            }
        }
    }
}

fn render_info_block(item: MenuItem) -> Element {
    // WP 5 ships the real CustomBlock renderer; until then emit a stub so the
    // slot is visible in snapshots.
    let has_block = item.block.is_some();
    let label = item.label_key.clone();
    rsx! {
        div {
            class: "context-menu-item context-menu-info-row disabled",
            span { "{label}" }
            if has_block {
                div {
                    class: "context-menu-info-stub",
                    "[custom-block pending WP 5]"
                }
            }
        }
    }
}

/// Invoke a plugin action and handle the [`ActionOutcome`] at WP-2 scope.
///
/// Only `Navigate` is wired through to the navigator here. Toast/Pending/
/// RefreshTarget/OpenSettings/OpenModal are logged for now — full routing
/// lands in WP 7 / WP 8.
async fn dispatch_action(
    account_id: String,
    action_id: String,
    target: MenuTargetKind,
    target_id: String,
) {
    // Re-read the ClientManager from the current scope. The caller `spawn`s
    // this future from within a component, so context is available.
    // We can't `use_context` inside an async fn, but we don't need to — the
    // caller has already captured the `Signal<ClientManager>` into the
    // closure. In practice the outer component reads it; here we access
    // via a fresh `Signal::peek` on the running scope. To keep the
    // abstraction local, use the dioxus `consume_context` helper.
    let client_manager: Signal<ClientManager> = match try_consume_context() {
        Some(cm) => cm,
        None => {
            tracing::warn!("ClientMenu: no ClientManager in context during dispatch");
            return;
        }
    };

    let Some(backend) = client_manager.read().get_backend(&account_id) else {
        tracing::warn!("ClientMenu: no backend for account {account_id}");
        return;
    };

    let outcome = {
        let guard = backend.read().await;
        guard
            .invoke_context_action(&action_id, target, &target_id)
            .await
    };

    match outcome {
        Ok(ActionOutcome::Navigate(route)) => {
            // D20 — plugins produce a route string built via host-api.build-route.
            // Dioxus navigator expects a typed Route; parsing lives in host. For
            // WP 2.A we log; WP 2.C integrates with navigator::push_str via a
            // route parser.
            tracing::info!("ClientMenu: Navigate({route}) — wiring pending");
        }
        Ok(ActionOutcome::Toast(payload)) => {
            tracing::info!("ClientMenu: toast {payload:?}");
        }
        Ok(other) => {
            tracing::debug!("ClientMenu: action outcome: {other:?}");
        }
        Err(err) => {
            tracing::warn!(
                "ClientMenu: invoke_context_action({action_id}) failed: {err:?}"
            );
        }
    }
}
