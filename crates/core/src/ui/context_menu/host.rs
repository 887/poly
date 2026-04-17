//! `ContextMenuStack` — renders the active `AppState.context_menu_stack`.
//!
//! Per plan-context-menu-quality-control.md §4.1.2 the stack host is
//! mounted once at `MainLayout` level (above sidebars, below the voice
//! banner). It walks the stack and dispatches each entry to its
//! registered render function by `menu_type`.
//!
//! Phase A scope (this file): the host component itself, the
//! `register_menu` registry that menu authors call from their module's
//! `#[ctor]`-style init, desktop cursor-anchored positioning, and
//! mobile center-overlay with a scrim. Submenu stacking is supported
//! structurally — the `Vec<ActiveContextMenu>` is walked top-to-bottom
//! and each level gets its own overlay — but a dedicated anchored-below
//! layout for `AnchoredBelow` is still TODO.
//!
//! Existing `ServerContextMenu` / `ChannelContextMenu` /
//! `MsgContextMenuOverlay` do not flow through this host yet (they
//! continue reading `AppState.context_menu` / `channel_context_menu`
//! directly). New menus — `ForumPostContextMenu`, `UserRowContextMenu`,
//! and eventually the migrated originals — target this host.

use crate::state::{ActiveContextMenu, AppState, MenuAnchor};
use dioxus::prelude::*;
use poly_ui_macros::context_menu;
use std::cell::RefCell;
use std::collections::HashMap;

/// A registered render function keyed by the `menu_type` string carried on
/// each `ActiveContextMenu`.
///
/// The function receives the `ctx_json` blob and a `close` handler; it
/// decodes the JSON back into its typed `Ctx` and returns the menu's
/// rendered `Element`.
pub type RenderFn = fn(ctx_json: &serde_json::Value, close: EventHandler<()>) -> Element;

thread_local! {
    static REGISTRY: RefCell<HashMap<&'static str, RenderFn>> = RefCell::new(HashMap::new());
}

/// Menu authors call this once (per process) to register their render
/// function. The `menu_type` must match the string the opener writes into
/// `ActiveContextMenu.menu_type`.
// lint-allow-unused: consumed by task #122 (ForumPostContextMenu / UserRowContextMenu) which lands in the next commit; keeping it pub now avoids a visibility flip later
#[allow(dead_code)]
pub fn register_menu(menu_type: &'static str, render: RenderFn) {
    REGISTRY.with(|r| {
        r.borrow_mut().insert(menu_type, render);
    });
}

fn lookup(menu_type: &str) -> Option<RenderFn> {
    REGISTRY.with(|r| r.borrow().get(menu_type).copied())
}

/// Render the currently-active menu stack. Renders nothing when the stack
/// is empty. Mount this once in `MainLayout` — a second mount would
/// render the stack twice.
#[context_menu(None)]
#[component]
pub fn ContextMenuStack() -> Element {
    let app_state: Signal<AppState> = use_context();
    let stack = app_state.read().context_menu_stack.clone();

    if stack.is_empty() {
        return rsx! {};
    }

    let is_mobile = crate::ui::main_layout::runtime_mobile_ui_active();

    rsx! {
        div { class: "context-menu-host",
            for (idx, entry) in stack.into_iter().enumerate() {
                ContextMenuStackEntry {
                    key: "{entry.id}",
                    entry,
                    depth: idx,
                    is_mobile,
                }
            }
        }
    }
}

/// Single entry in the stack — rendered independently so each submenu
/// level gets its own backdrop / key.
#[context_menu(None)]
#[component]
fn ContextMenuStackEntry(entry: ActiveContextMenu, depth: usize, is_mobile: bool) -> Element {
    let mut app_state: Signal<AppState> = use_context();

    let menu_id = entry.id;
    let close = use_callback(move |()| {
        app_state
            .write()
            .context_menu_stack
            .retain(|m| m.id != menu_id);
    });

    let Some(render) = lookup(entry.menu_type) else {
        // Menu type wasn't registered — log at debug level and render
        // nothing rather than blow up.
        #[cfg(debug_assertions)]
        tracing::debug!(
            target: "poly::context_menu",
            "no render fn registered for menu_type `{}`",
            entry.menu_type
        );
        return rsx! {};
    };
    let content = render(&entry.ctx_json, close);

    // On mobile, coerce any anchor to a centered overlay (§4.3.1).
    let anchor = if is_mobile {
        MenuAnchor::Center
    } else {
        entry.anchor.clone()
    };

    let style = match &anchor {
        MenuAnchor::Cursor { x, y } => format!("position: fixed; left: {x}px; top: {y}px;"),
        MenuAnchor::AnchoredBelow { x, y, width } => {
            format!("position: fixed; left: {x}px; top: {y}px; min-width: {width}px;")
        }
        MenuAnchor::Center => String::new(),
    };

    let class = if matches!(anchor, MenuAnchor::Center) {
        "context-menu-overlay context-menu-overlay-mobile"
    } else {
        "context-menu-overlay"
    };

    // Dismissal: click on the backdrop pops this level.
    let dismiss_on_outside = entry.dismiss_on_outside;
    let on_backdrop_click = move |_| {
        if dismiss_on_outside {
            close.call(());
        }
    };

    rsx! {
        div {
            class: "context-menu-backdrop",
            "data-depth": "{depth}",
            onclick: on_backdrop_click,
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    close.call(());
                }
            },
            div {
                class: "{class}",
                style: "{style}",
                role: "menu",
                // Stop propagation so a click *inside* the menu card doesn't
                // bubble up to the backdrop and dismiss the menu.
                onclick: move |evt| evt.stop_propagation(),
                {content}
            }
        }
    }
}
