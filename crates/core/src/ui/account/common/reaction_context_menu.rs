//! Reaction chip right-click context menu component.
//!
//! Rendered at the `MainLayout` level so it is never clipped by sidebars.
//! Opened by right-clicking an emoji reaction pill (`.reaction-pill`) on
//! a message in the chat view.
//!
//! State lives in `AppState.reaction_context_menu`. The `oncontextmenu`
//! handler on the reaction `<button>` writes `Some(ReactionContextMenuState)`.
//! A global click on the `MainLayout` root clears it.
//!
//! ## Menu items
//! - Show who reacted — debug stub (full reactors list is out of scope)
//! - Remove my reaction — calls `toggle_reaction_on_message`

use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::state::{AppState, ChatData};
use crate::ui::account::common::chat_view::toggle_reaction_on_message;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Reaction chip right-click context menu.
///
/// Reads `AppState.reaction_context_menu` and renders a floating div at
/// the stored coordinates. Renders nothing when the state is `None`.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ReactionContextMenu() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();

    let Some(menu) = app_state.read().reaction_context_menu.clone() else {
        return rsx! {};
    };

    let x = menu.x;
    let y = menu.y;
    let emoji = menu.emoji.clone();
    let message_id = menu.message_id.clone();

    let close = move || {
        app_state.batch(|st| st.reaction_context_menu = None);
    };

    rsx! {
        // Transparent backdrop — closes menu on click and blocks native context menu.
        div {
            class: "context-menu-backdrop",
            onclick: move |_| {
                app_state.batch(|st| st.reaction_context_menu = None);
            },
            oncontextmenu: move |evt| evt.prevent_default(),
        }

        // The floating menu itself.
        div {
            class: "context-menu",
            style: "left: {x}px; top: {y}px;",
            // Stop clicks from reaching the backdrop.
            onclick: move |evt| evt.stop_propagation(),

            // Show who reacted — stub (reaction-details UI is out of scope for this phase)
            {
                let e = emoji.clone();
                let close = close;
                rsx! {
                    ReactionMenuItem {
                        label: t("reaction-menu-show-reactors"),
                        onclick: move |_| {
                            tracing::debug!(
                                target: "poly::context_menu",
                                "show-reactors stub: emoji={e}"
                            );
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Remove my reaction — toggles the reaction off
            {
                let mid = message_id.clone();
                let e = emoji.clone();
                let close = close;
                rsx! {
                    ReactionMenuItem {
                        label: t("reaction-menu-remove"),
                        onclick: move |_| {
                            toggle_reaction_on_message(chat_data, &mid, &e);
                            close();
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
