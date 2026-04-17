//! Channel right-click / long-press context menu component.
//!
//! Rendered at the `MainLayout` level (above sidebar overflow) so it is
//! never clipped by `overflow: hidden` on the sidebar containers.
//!
//! State lives in `AppState.channel_context_menu`. Any `oncontextmenu`
//! handler (or mobile long-press handler) on a channel row sets
//! `app_state.write().channel_context_menu = Some(...)`.
//! A global click on the `MainLayout` root div clears it.
//!
//! ## Menu items
//! - Mark as Read
//! - Mute / Unmute Channel (toggle, local state for now)
//! - Copy Channel ID

use crate::i18n::t;
use crate::state::{AppState, ChatData};
use crate::ui::account::common::chat_view::mark_channel_as_read;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// Channel right-click / long-press context menu.
///
/// Reads `AppState.channel_context_menu` and renders a floating div at the
#[context_menu(inherit)]
/// stored coordinates. Renders nothing when `channel_context_menu` is `None`.
#[component]
pub fn ChannelContextMenu() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    let Some(menu) = app_state.read().channel_context_menu.clone() else {
        return rsx! {};
    };

    let channel_id = menu.channel_id.clone();
    let mut muted = use_signal(|| false);

    let x = menu.x;
    let y = menu.y;

    let close = move || {
        app_state.write().channel_context_menu = None;
    };

    rsx! {
        // Transparent backdrop — closes menu on click and blocks native context menu.
        div {
            class: "context-menu-backdrop",
            onclick: move |_| {
                app_state.write().channel_context_menu = None;
            },
            oncontextmenu: move |evt| evt.prevent_default(),
        }

        // The floating menu itself.
        div {
            class: "context-menu",
            style: "left: {x}px; top: {y}px;",
            // Stop clicks from reaching the backdrop.
            onclick: move |evt| evt.stop_propagation(),

            // Mark as Read
            {
                let channel_id = channel_id.clone();
                let mut close = close;
                rsx! {
                    ChannelMenuItem {
                        label: t("channel-menu-mark-read"),
                        onclick: move |_| {
                            mark_channel_as_read(&mut chat_data, &channel_id);
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Mute / Unmute toggle (local state — backend integration in a later phase)
            ChannelMenuItem {
                label: if muted() { t("channel-menu-unmute") } else { t("channel-menu-mute") },
                onclick: move |_| muted.toggle(),
            }

            div { class: "context-menu-separator" }

            // Copy Channel ID
            {
                let cid = channel_id.clone();
                let mut close = close;
                rsx! {
                    ChannelMenuItem {
                        label: t("channel-menu-copy-id"),
                        onclick: move |_| {
                            let cid2 = cid.clone();
                            let _eval = document::eval(&format!("navigator.clipboard.writeText('{cid2}')"));
                            close();
                        },
                    }
                }
            }
        }
    }
}

/// A single clickable item inside the channel context menu.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn ChannelMenuItem(
    label: String,
    #[props(default = false)] danger: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: if danger { "context-menu-item danger" } else { "context-menu-item" },
            onclick: move |evt| onclick.call(evt),
            span { "{label}" }
        }
    }
}
