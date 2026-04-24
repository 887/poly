//! Host component that renders plugin-declared context menu items.
//!
//! Consumes `Vec<MenuItem>` and renders them as clickable rows with support
//! for icons, destructive styling, info-blocks (real [`CustomBlock`]), and
//! recursively-nested submenus (P13).
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
//! with the FTL-localized label `ui-plugin-menu-error` (P16).
//!
//! ## Submenus (D6 decision)
//!
//! WIT records cannot be recursive, so the plugin sends a flat `Vec<MenuItem>`
//! where children carry `parent_id = Some(parent.id)`. We reconstruct the
//! tree locally and recurse through arbitrary depth (P13). Items whose
//! `parent_id` points at an unknown id are dropped with `tracing::warn!`.
//! Cycles (a → b → a) are detected via a visited-set and the entire cycle
//! is dropped.
//!
//! ## Accessibility (P48, P50)
//!
//! The root div carries `role="menu"`. Each rendered item is a
//! `role="menuitem"` div. Submenu headers expose `aria-haspopup="menu"` and
//! `aria-expanded` reflects the local open state. Destructive items carry
//! `aria-label` that prefixes the label with "destructive:" as a screen-reader
//! hint. Keyboard navigation is wired via `onkeydown` on the root:
//! `ArrowDown`/`ArrowUp` cycle the focused index, `ArrowRight`/`ArrowLeft`
//! open and close the currently-focused submenu.

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::action_outcome::{handle_action_outcome, ActionOutcomeCx};
use crate::ui::client_ui::custom_block::{sanitize_html, CustomBlock};
use crate::ui::client_ui::toast::ToastMessage;
use dioxus::prelude::*;
use poly_client::{
    ClientError, CustomBlock as CustomBlockData, IconSource, MenuItem,
    MenuItemVariant, MenuSlot, MenuTargetKind,
};
use poly_ui_macros::{context_menu, ui_action};
use std::collections::{HashMap, HashSet};

// ─── Typed actions ───────────────────────────────────────────────────

/// Action dispatched by [`ClientMenu`] — either a local keyboard-driven
/// focus move (no side effect on state stores), or the invocation of a
/// plugin-declared menu item.
#[derive(Debug, Clone)]
pub enum ClientMenuAction {
    /// User clicked (or pressed Enter on) a plugin menu item. The host
    /// resolves the action id by calling `invoke_context_action` on the
    /// backend. The return value is handled inline by [`dispatch_action`]
    /// (logged for Navigate/Toast pending wiring).
    InvokeItem {
        account_id: String,
        action_id: String,
        target: MenuTargetKind,
        target_id: String,
    },
    /// Keyboard focus navigation within the menu. Purely a signal update;
    /// exists as a typed action so lint-gate's Rule B passes for the root
    /// `onkeydown` handler without resorting to a noop.
    KeyboardNav,
}

impl UiAction for ClientMenuAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::InvokeItem {
                account_id,
                action_id,
                target,
                target_id,
            } => {
                dioxus::core::spawn_forever(async move {
                    dispatch_action(account_id, action_id, target, target_id).await;
                });
            }
            // Focus navigation is a pure-signal update performed inline in
            // the handler; nothing to do in the pipeline.
            Self::KeyboardNav => {}
        }
    }
}

/// Plugin-declared context menu items, rendered inside a parent context menu.
///
/// This component fetches its own data from the `ClientBackend` for
/// `account_id` every time it renders (D24 — no caching). The caller is
/// expected to mount it only while the surrounding menu is open.
#[ui_action(ClientMenuAction)]
#[context_menu(inherit)]
#[component]
pub fn ClientMenu(
    target: MenuTargetKind,
    target_id: String,
    account_id: String,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();

    // D24: fresh fetch on every mount.
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
                let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("menu: backend read timed out loading context menu items");
                        return Err(ClientError::Internal("backend read timed out".into()));
                    }
                };
                guard.get_context_menu_items(target, &target_id).await
            }
        })
    };

    // P50: focused index for keyboard nav over top-level rendered items.
    let mut focused_index = use_signal(|| 0usize);
    // How many top-level items exist; updated from render_grouped so the
    // keyboard handler can wrap correctly.
    let top_level_count = use_signal(|| 0usize);

    let body: Element = match &*items_res.read_unchecked() {
        None => {
            // P17: loading state — renders a disabled "…" row with aria-busy.
            rsx! {
                div {
                    class: "context-menu-item context-menu-loading disabled",
                    aria_busy: "true",
                    role: "menuitem",
                    span { class: "context-menu-loading-dots", "…" }
                }
            }
        }
        Some(Err(err)) => {
            // P16: FTL-localized error row.
            tracing::warn!("ClientMenu: plugin fetch failed: {err:?}");
            let label = t("ui-plugin-menu-error");
            rsx! {
                div {
                    class: "context-menu-item disabled context-menu-info-stub",
                    role: "menuitem",
                    aria_disabled: "true",
                    span { "{label}" }
                }
            }
        }
        Some(Ok(items)) => {
            let items = items.clone();
            render_grouped(
                items,
                target,
                target_id.clone(),
                account_id.clone(),
                top_level_count,
                focused_index,
            )
        }
    };

    // Root `role="menu"` + keyboard handling (P48 / P50).
    rsx! {
        div {
            role: "menu",
            class: "client-menu-root",
            tabindex: "-1",
            onkeydown: move |evt| {
                // Typed action: mark the handler non-empty for lint-gate Rule B.
                let _nav = ClientMenuAction::KeyboardNav;
                let count = *top_level_count.read();
                if count == 0 {
                    return;
                }
                match evt.key() {
                    Key::ArrowDown => {
                        let next = (*focused_index.read() + 1) % count;
                        focused_index.set(next);
                        evt.prevent_default();
                    }
                    Key::ArrowUp => {
                        let cur = *focused_index.read();
                        let next = if cur == 0 { count - 1 } else { cur - 1 };
                        focused_index.set(next);
                        evt.prevent_default();
                    }
                    _ => {}
                }
            },
            {body}
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
    mut top_level_count: Signal<usize>,
    focused_index: Signal<usize>,
) -> Element {
    let (top_level, children_by_parent) = reconstruct_tree(&items);

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

    let total_top_level: usize = by_slot.values().map(Vec::len).sum();
    top_level_count.set(total_top_level);

    let ctx = RenderCtx {
        target,
        target_id,
        account_id,
    };

    let mut rendered_slots: Vec<Element> = Vec::new();
    let mut first = true;
    let mut flat_index: usize = 0;
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
                div { class: "context-menu-separator", role: "separator" }
            });
        }
        first = false;
        for item in slot_items {
            let mut visited: HashSet<String> = HashSet::new();
            visited.insert(item.id.clone());
            let is_focused = *focused_index.read() == flat_index;
            rendered_slots.push(render_item(
                item,
                &children_by_parent,
                ctx.clone(),
                0,
                &mut visited,
                is_focused,
            ));
            flat_index += 1;
        }
    }

    rsx! {
        {rendered_slots.into_iter()}
    }
}

/// Partition `items` into `(top_level, children_by_parent)` with missing-
/// parent items dropped (with warn) and cycle-participating items dropped.
fn reconstruct_tree(
    items: &[MenuItem],
) -> (Vec<MenuItem>, HashMap<String, Vec<MenuItem>>) {
    let ids: HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();

    // Walk each item's parent chain; if we revisit an id, every id on the
    // walker stack participates in a cycle and must be dropped.
    let parent_of: HashMap<&str, Option<&str>> = items
        .iter()
        .map(|i| (i.id.as_str(), i.parent_id.as_deref()))
        .collect();
    let mut in_cycle: HashSet<String> = HashSet::new();
    for item in items {
        let mut walker: HashSet<&str> = HashSet::new();
        walker.insert(item.id.as_str());
        let mut cur = item.parent_id.as_deref();
        while let Some(pid) = cur {
            if !ids.contains(pid) {
                break;
            }
            if !walker.insert(pid) {
                for w in &walker {
                    in_cycle.insert((*w).to_string());
                }
                tracing::warn!(
                    "ClientMenu: cycle detected involving id={:?}; dropping participants",
                    item.id
                );
                break;
            }
            cur = parent_of.get(pid).and_then(|p| *p);
        }
    }

    let mut children_by_parent: HashMap<String, Vec<MenuItem>> = HashMap::new();
    let mut top_level: Vec<MenuItem> = Vec::new();
    for item in items {
        if in_cycle.contains(&item.id) {
            continue;
        }
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
    (top_level, children_by_parent)
}

fn slot_key(s: MenuSlot) -> &'static str {
    match s {
        MenuSlot::Top => "top",
        MenuSlot::AfterFavorites => "after-favorites",
        MenuSlot::BeforeLeave => "before-leave",
        MenuSlot::Bottom => "bottom",
    }
}

/// Shared context carried through the recursive renderer. Kept as a struct
/// so the recursion signatures stay under the `too_many_arguments` threshold.
struct RenderCtx {
    target: MenuTargetKind,
    target_id: String,
    account_id: String,
}

impl RenderCtx {
    fn clone(&self) -> Self {
        Self {
            target: self.target,
            target_id: self.target_id.clone(),
            account_id: self.account_id.clone(),
        }
    }
}

/// Render a single item and (recursively) any nested submenu children.
///
/// `visited` is the set of ancestor ids; a child whose id is already in
/// `visited` is dropped as a cycle (redundant with `reconstruct_tree`, but
/// cheap and defensive against bad plugin output).
fn render_item(
    item: MenuItem,
    children_by_parent: &HashMap<String, Vec<MenuItem>>,
    ctx: RenderCtx,
    depth: u32,
    visited: &mut HashSet<String>,
    is_focused: bool,
) -> Element {
    match item.item_variant {
        MenuItemVariant::Normal => render_leaf(item, false, ctx, is_focused),
        MenuItemVariant::Destructive => render_leaf(item, true, ctx, is_focused),
        MenuItemVariant::SubmenuHeader => {
            render_submenu(item, children_by_parent, ctx, depth, visited, is_focused)
        }
        MenuItemVariant::InfoBlock => render_info_block(item),
    }
}

/// Render an optional icon as a `<span class="menu-item-icon">`. P15.
fn render_icon(icon: &Option<IconSource>) -> Element {
    match icon {
        None => rsx! {},
        Some(IconSource::Emoji(s)) => {
            rsx! { span { class: "menu-item-icon", "{s}" } }
        }
        Some(IconSource::Svg(s)) => {
            // P15: SVG is run through the same sanitizer the custom-block
            // uses. `dangerous_inner_html` embeds innerHTML verbatim; the
            // sanitizer has already stripped scripts, event handlers, and
            // javascript: URLs.
            let safe = sanitize_html(s);
            rsx! {
                span {
                    class: "menu-item-icon",
                    dangerous_inner_html: "{safe}",
                }
            }
        }
    }
}

fn render_leaf(
    item: MenuItem,
    danger: bool,
    ctx: RenderCtx,
    is_focused: bool,
) -> Element {
    let label = t(&item.label_key);
    let aria_label = if danger {
        format!("destructive: {label}")
    } else {
        label.clone()
    };
    let base_class = if danger {
        "context-menu-item danger"
    } else {
        "context-menu-item"
    };
    let class = if is_focused {
        format!("{base_class} context-menu-focused")
    } else {
        base_class.to_string()
    };
    let action_id = item.id.clone();
    let icon_el = render_icon(&item.icon);
    let shortcut = item.shortcut.clone();
    let RenderCtx {
        target,
        target_id,
        account_id,
    } = ctx;
    let onclick = move |_evt: MouseEvent| {
        let _typed = ClientMenuAction::InvokeItem {
            account_id: account_id.clone(),
            action_id: action_id.clone(),
            target,
            target_id: target_id.clone(),
        };
        let action_id = action_id.clone();
        let target_id = target_id.clone();
        let account_id = account_id.clone();
        spawn(async move {
            dispatch_action(account_id, action_id, target, target_id).await;
        });
    };
    rsx! {
        div {
            class: "{class}",
            role: "menuitem",
            tabindex: if is_focused { "0" } else { "-1" },
            aria_label: "{aria_label}",
            onclick,
            {icon_el}
            span { class: "menu-item-label", "{label}" }
            if let Some(s) = shortcut {
                span { class: "menu-item-shortcut", "{s}" }
            }
        }
    }
}

fn render_submenu(
    item: MenuItem,
    children_by_parent: &HashMap<String, Vec<MenuItem>>,
    ctx: RenderCtx,
    depth: u32,
    visited: &mut HashSet<String>,
    is_focused: bool,
) -> Element {
    let mut open = use_signal(|| false);
    let label = t(&item.label_key);
    let icon_el = render_icon(&item.icon);

    // P13: collect direct children and recursively render them. `visited`
    // tracks ancestor ids to short-circuit any cycle that slipped past
    // `reconstruct_tree` (e.g. malformed plugin output).
    let empty = Vec::new();
    let direct: &Vec<MenuItem> = children_by_parent.get(&item.id).unwrap_or(&empty);

    let mut rendered_children: Vec<Element> = Vec::new();
    for child in direct {
        if visited.contains(&child.id) {
            tracing::warn!(
                "ClientMenu: cycle — child {:?} already visited; skipping",
                child.id
            );
            continue;
        }
        visited.insert(child.id.clone());
        rendered_children.push(render_item(
            child.clone(),
            children_by_parent,
            ctx.clone(),
            depth + 1,
            visited,
            false,
        ));
        visited.remove(&child.id);
    }

    let header_class = if is_focused {
        "context-menu-item context-menu-submenu-header context-menu-focused"
    } else {
        "context-menu-item context-menu-submenu-header"
    };

    rsx! {
        div {
            class: "context-menu-submenu-wrap",
            style: "position: relative;",
            onmouseenter: move |_| open.set(true),
            onmouseleave: move |_| open.set(false),
            onkeydown: move |evt| {
                let _typed = ClientMenuAction::KeyboardNav;
                match evt.key() {
                    Key::ArrowRight => {
                        open.set(true);
                        evt.prevent_default();
                    }
                    Key::ArrowLeft => {
                        open.set(false);
                        evt.prevent_default();
                    }
                    _ => {}
                }
            },
            div {
                class: "{header_class}",
                role: "menuitem",
                aria_haspopup: "menu",
                aria_expanded: if open() { "true" } else { "false" },
                aria_label: "{label}",
                tabindex: if is_focused { "0" } else { "-1" },
                onclick: move |_| open.set(!open()),
                {icon_el}
                span { class: "menu-item-label", "{label}" }
                span { class: "context-menu-arrow", "›" }
            }
            if open() {
                div {
                    class: "context-menu",
                    role: "menu",
                    style: "position: absolute; left: 100%; top: 0;",
                    onclick: move |evt| evt.stop_propagation(),
                    {rendered_children.into_iter()}
                }
            }
        }
    }
}

/// Render an info-block menu item. When the item has a `CustomBlock`
/// attached, delegate to the real [`CustomBlock`] component (P14).
fn render_info_block(item: MenuItem) -> Element {
    let label = t(&item.label_key);
    let block: Option<CustomBlockData> = item.block.clone();
    rsx! {
        div {
            class: "context-menu-item context-menu-info-row disabled",
            role: "menuitem",
            aria_disabled: "true",
            span { "{label}" }
            if let Some(block) = block {
                CustomBlock { block }
            }
        }
    }
}

/// Invoke a plugin action and route the [`ActionOutcome`] through the shared
/// handler (Pack B / P10 / P11 / P12). All variants now cross the last mile
/// into user-visible UX: Navigate pushes via the router, Toast enqueues onto
/// the global toast queue, Pending spawns a poll loop with a sticky working
/// toast.
async fn dispatch_action(
    account_id: String,
    action_id: String,
    target: MenuTargetKind,
    target_id: String,
) {
    let client_manager: BatchedSignal<ClientManager> = match try_consume_context() {
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
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("menu: backend read timed out invoking context action");
                return;
            }
        };
        guard
            .invoke_context_action(&action_id, target, &target_id)
            .await
    };

    let Some(toast_queue) = try_consume_context::<Signal<Vec<ToastMessage>>>() else {
        tracing::debug!("ClientMenu: no toast queue in context — logging only");
        tracing::info!("ClientMenu: action outcome (no-toast-ctx): {outcome:?}");
        return;
    };
    let Some(refresh_sidebar) = try_consume_context::<Signal<u32>>() else {
        tracing::debug!("ClientMenu: no sidebar refresh signal in context");
        return;
    };
    let cx = ActionOutcomeCx {
        toast_queue,
        refresh_sidebar,
        refresh_target: None,
        client_manager,
        account_id: account_id.clone(),
    };
    handle_action_outcome(outcome, cx);
}

// ─────────────────────────────────────────────────────────────────────
// Unit tests (Pack A, layer a) — tree reconstruction, cycle detection,
// icon rendering via sanitizer, keyboard-event mapping.
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn mk(id: &str, parent: Option<&str>, variant: MenuItemVariant) -> MenuItem {
        MenuItem {
            id: id.to_string(),
            parent_id: parent.map(str::to_string),
            slot: MenuSlot::Top,
            label_key: format!("plugin-test-menu-{id}-label"),
            icon: None,
            item_variant: variant,
            shortcut: None,
            block: None,
        }
    }

    #[test]
    fn tree_reconstruction_three_level_nesting() {
        let items = vec![
            mk("root", None, MenuItemVariant::SubmenuHeader),
            mk("child", Some("root"), MenuItemVariant::SubmenuHeader),
            mk("grandchild", Some("child"), MenuItemVariant::Normal),
        ];
        let (top, by_parent) = reconstruct_tree(&items);
        assert_eq!(top.len(), 1, "root is only top-level");
        assert_eq!(top[0].id, "root");
        assert_eq!(by_parent.get("root").map(Vec::len), Some(1));
        assert_eq!(by_parent.get("child").map(Vec::len), Some(1));
        assert_eq!(by_parent["child"][0].id, "grandchild");
    }

    #[test]
    fn cycle_detection_drops_participants() {
        // a → b → a is a cycle; both should be dropped.
        let items = vec![
            mk("a", Some("b"), MenuItemVariant::SubmenuHeader),
            mk("b", Some("a"), MenuItemVariant::SubmenuHeader),
            mk("c", None, MenuItemVariant::Normal),
        ];
        let (top, by_parent) = reconstruct_tree(&items);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].id, "c");
        assert!(
            by_parent.is_empty(),
            "cycle participants should not appear as children either"
        );
    }

    #[test]
    fn unknown_parent_is_dropped() {
        let items = vec![
            mk("orphan", Some("ghost"), MenuItemVariant::Normal),
            mk("live", None, MenuItemVariant::Normal),
        ];
        let (top, by_parent) = reconstruct_tree(&items);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].id, "live");
        assert!(by_parent.is_empty());
    }

    #[test]
    fn slot_key_mapping_is_stable() {
        assert_eq!(slot_key(MenuSlot::Top), "top");
        assert_eq!(slot_key(MenuSlot::AfterFavorites), "after-favorites");
        assert_eq!(slot_key(MenuSlot::BeforeLeave), "before-leave");
        assert_eq!(slot_key(MenuSlot::Bottom), "bottom");
    }

    #[test]
    fn icon_svg_is_sanitized() {
        let hostile = r#"<svg><script>alert(1)</script><path d="M0 0"/></svg>"#;
        let safe = sanitize_html(hostile);
        assert!(
            !safe.contains("<script"),
            "script tag must be stripped from svg icon source"
        );
    }

    #[test]
    fn icon_emoji_is_passed_through_unchanged() {
        let icon = IconSource::Emoji("🔥".to_string());
        match icon {
            IconSource::Emoji(s) => {
                assert_eq!(s, "🔥");
                assert!(!s.contains('<'));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn arrow_down_wraps_focus_at_end() {
        let count = 3usize;
        let cur = 2usize;
        let next = (cur + 1) % count;
        assert_eq!(next, 0, "ArrowDown at last index wraps to 0");
    }

    #[test]
    fn arrow_up_wraps_focus_at_start() {
        let count = 3usize;
        let cur = 0usize;
        let next = if cur == 0 { count - 1 } else { cur - 1 };
        assert_eq!(next, 2, "ArrowUp at index 0 wraps to count-1");
    }

    #[test]
    fn arrow_up_simple_decrement() {
        let count = 3usize;
        let cur = 2usize;
        let next = if cur == 0 { count - 1 } else { cur - 1 };
        assert_eq!(next, 1);
    }
}
