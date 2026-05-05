//! Reaction chip right-click context menu component.
//!
//! Rendered via the `ContextMenuStack` host at the `MainLayout` level.
//! Opened by right-clicking an emoji reaction pill (`.reaction-pill`) on
//! a message in the chat view. State is pushed onto
//! `AppState.context_menu_stack`.
//!
//! ## Menu items
//! - Show who reacted — debug stub (full reactors list is out of scope)
//! - Remove my reaction — calls `toggle_reaction_on_message`

use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::state::{AppState, ChatViewState, ReactionContextMenuState};
use crate::ui::account::common::chat_view::toggle_reaction_on_message;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Reaction chip right-click context menu — stack-based inner component.
///
/// Receives the deserialized `ReactionContextMenuState` from the stack host
/// and a `close` callback to pop itself off the stack.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ReactionContextMenuInner(menu: ReactionContextMenuState, close: EventHandler<()>) -> Element {
    let _app_state: BatchedSignal<AppState> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();

    let x = menu.x;
    let y = menu.y;
    let emoji = menu.emoji.clone();
    let message_id = menu.message_id.clone();

    rsx! {
        // The floating menu itself — backdrop + dismiss handled by the stack host.
        div {
            class: "context-menu",
            style: "left: {x}px; top: {y}px;",
            onclick: move |evt| evt.stop_propagation(),

            // Show who reacted — stub (reaction-details UI is out of scope for this phase)
            {
                let e = emoji.clone();
                rsx! {
                    ReactionMenuItem {
                        label: t("reaction-menu-show-reactors"),
                        onclick: move |_| {
                            tracing::debug!(
                                target: "poly::context_menu",
                                "show-reactors stub: emoji={e}"
                            );
                            close.call(());
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Remove my reaction — toggles the reaction off
            {
                let mid = message_id.clone();
                let e = emoji.clone();
                rsx! {
                    ReactionMenuItem {
                        label: t("reaction-menu-remove"),
                        onclick: move |_| {
                            toggle_reaction_on_message(chat_view_state, &mid, &e);
                            close.call(());
                        },
                    }
                }
            }
        }
    }
}

/// A single clickable item inside the reaction context menu.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ReactionMenuItem(
    label: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: "context-menu-item",
            onclick: move |evt| onclick.call(evt),
            span { "{label}" }
        }
    }
}
