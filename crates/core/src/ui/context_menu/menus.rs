//! Phase A menu authors — `ForumPostContextMenu` and `UserRowContextMenu`.
//!
//! Per `plan-context-menu-quality-control.md` §4.6 (first new menus to
//! flow through the stack host). Each menu:
//!
//! - Defines a zero-sized marker type used as the `#[context_menu(Foo)]`
//!   argument at the trigger component.
//! - Implements `ContextMenuFor<TriggerProps>` — `build_ctx` snapshots
//!   just the data the items need, `render` returns the overlay items.
//! - Registers a JSON-round-tripping render fn with `register_menu`
//!   so `ContextMenuStack` can dispatch by `menu_type` string without
//!   needing compile-time knowledge of every menu.
//!
//! `register_all_menus()` wires the registry at `App` mount. Call it
//! once from `use_effect` alongside the other native registrations.

use super::host::register_menu;
use super::ContextMenuFor;
use crate::i18n::t;
use crate::state::{ActiveContextMenu, AppState, MenuAnchor};
use crate::ui::account::common::user_profile_modal::open_user_profile;
use dioxus::events::MouseEvent;
use dioxus::prelude::*;
use poly_client::User;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// ForumPostContextMenu — right-click / long-press on a forum post card or
// threaded comment.
// ─────────────────────────────────────────────────────────────────────────────

pub struct ForumPostContextMenu;

#[derive(Clone, Serialize, Deserialize)]
pub struct ForumPostCtx {
    pub post_id: String,
    pub author_id: String,
    pub author_name: String,
    pub text: String,
}

pub const FORUM_POST_MENU_TYPE: &str = "forum_post";

/// Convenience: build an `ActiveContextMenu` that the caller can push onto
/// `AppState.context_menu_stack`.
pub fn forum_post_entry(ctx: ForumPostCtx, evt: &MouseEvent) -> ActiveContextMenu {
    let coords = evt.client_coordinates();
    ActiveContextMenu {
        id: next_menu_id(),
        anchor: MenuAnchor::Cursor {
            x: coords.x,
            y: coords.y,
        },
        ctx_json: serde_json::to_value(&ctx).unwrap_or(serde_json::Value::Null),
        menu_type: FORUM_POST_MENU_TYPE,
        dismiss_on_outside: true,
    }
}

fn render_forum_post(ctx_json: &serde_json::Value, close: EventHandler<()>) -> Element {
    let Ok(ctx) = serde_json::from_value::<ForumPostCtx>(ctx_json.clone()) else {
        return rsx! {};
    };
    rsx! {
        div { class: "context-menu-items",
            button {
                class: "context-menu-item",
                onclick: move |_| {
                    copy_text_best_effort(&ctx.text);
                    close.call(());
                },
                "{t(\"menu-copy-text\")}"
            }
            button {
                class: "context-menu-item",
                onclick: move |_| {
                    copy_text_best_effort(&ctx.post_id);
                    close.call(());
                },
                "{t(\"menu-copy-id\")}"
            }
            div { class: "context-menu-separator" }
            div { class: "context-menu-label", "{ctx.author_name}" }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UserRowContextMenu — right-click / long-press on a DM group member row.
// ─────────────────────────────────────────────────────────────────────────────

pub struct UserRowContextMenu;

#[derive(Clone, Serialize, Deserialize)]
pub struct UserRowCtx {
    pub user: User,
    pub group_id: String,
    pub account_id: String,
}

pub const USER_ROW_MENU_TYPE: &str = "user_row";

pub fn user_row_entry(ctx: UserRowCtx, evt: &MouseEvent) -> ActiveContextMenu {
    let coords = evt.client_coordinates();
    ActiveContextMenu {
        id: next_menu_id(),
        anchor: MenuAnchor::Cursor {
            x: coords.x,
            y: coords.y,
        },
        ctx_json: serde_json::to_value(&ctx).unwrap_or(serde_json::Value::Null),
        menu_type: USER_ROW_MENU_TYPE,
        dismiss_on_outside: true,
    }
}

fn render_user_row(ctx_json: &serde_json::Value, close: EventHandler<()>) -> Element {
    let Ok(ctx) = serde_json::from_value::<UserRowCtx>(ctx_json.clone()) else {
        return rsx! {};
    };
    let app_state: Signal<AppState> = use_context();
    let user_for_profile = ctx.user.clone();
    let copy_id = ctx.user.id.clone();
    let display_name = ctx.user.display_name.clone();

    rsx! {
        div { class: "context-menu-items",
            div { class: "context-menu-label", "{display_name}" }
            div { class: "context-menu-separator" }
            button {
                class: "context-menu-item",
                onclick: move |_| {
                    open_user_profile(app_state, user_for_profile.clone());
                    close.call(());
                },
                "{t(\"menu-view-profile\")}"
            }
            button {
                class: "context-menu-item",
                onclick: move |_| {
                    copy_text_best_effort(&copy_id);
                    close.call(());
                },
                "{t(\"menu-copy-id\")}"
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ContextMenuFor impls — the trigger components' Props are not nameable
// outside the `#[component]` expansion, so these impls use `()` as the
// Props type. The `#[context_menu(Foo)]` macro is argument-validation
// only in Phase A (it does not emit a trait bound), so the Props binding
// is advisory until the Phase B DOM wrapper lands.
// ─────────────────────────────────────────────────────────────────────────────

// lint-allow-unused: consumed by the Phase B DOM wrapper that lands alongside the first migrated menu — keeping the impl now anchors the contract
#[allow(dead_code)]
impl ContextMenuFor<()> for ForumPostContextMenu {
    type Ctx = ForumPostCtx;
    fn build_ctx(_props: &(), _evt: &MouseEvent) -> Self::Ctx {
        ForumPostCtx {
            post_id: String::new(),
            author_id: String::new(),
            author_name: String::new(),
            text: String::new(),
        }
    }
    fn render(ctx: Self::Ctx, close: EventHandler<()>) -> Element {
        let json = serde_json::to_value(&ctx).unwrap_or(serde_json::Value::Null);
        render_forum_post(&json, close)
    }
}

// lint-allow-unused: consumed by the Phase B DOM wrapper that lands alongside the first migrated menu — keeping the impl now anchors the contract
#[allow(dead_code)]
impl ContextMenuFor<()> for UserRowContextMenu {
    type Ctx = UserRowCtx;
    fn build_ctx(_props: &(), _evt: &MouseEvent) -> Self::Ctx {
        unreachable!("UserRowCtx requires trigger-scoped data not available from ()")
    }
    fn render(ctx: Self::Ctx, close: EventHandler<()>) -> Element {
        let json = serde_json::to_value(&ctx).unwrap_or(serde_json::Value::Null);
        render_user_row(&json, close)
    }
}

/// Register every Phase-A menu render fn. Call once from `App` mount.
pub fn register_all_menus() {
    register_menu(FORUM_POST_MENU_TYPE, render_forum_post);
    register_menu(USER_ROW_MENU_TYPE, render_user_row);
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn next_menu_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn copy_text_best_effort(text: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let escaped = text.replace('\\', "\\\\").replace('`', "\\`");
        let _ = document::eval(&format!(
            "navigator.clipboard && navigator.clipboard.writeText(`{escaped}`);"
        ));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = text;
    }
}
