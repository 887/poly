//! Channel right-click / long-press context menu component.
//!
//! Rendered via the `ContextMenuStack` host at the `MainLayout` level
//! (above sidebar overflow) so it is never clipped by `overflow: hidden`.
//!
//! State is pushed onto `AppState.context_menu_stack` by `oncontextmenu`
//! handlers on channel rows. The stack host dispatches to `ChannelContextMenuInner`
//! via `register_menu(CHANNEL_MENU_TYPE, render_channel)`.
//!
//! ## Menu items
//! - Mark as Read
//! - Mute / Unmute Channel (toggle, local state for now)
//! - Copy Channel ID

use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::nav;
use crate::state::{ChannelContextMenuState, ChatLists, ChatViewState};
use crate::ui::account::common::chat_view::mark_channel_as_read;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Channel right-click / long-press context menu — stack-based inner component.
///
/// Receives the deserialized `ChannelContextMenuState` from the stack host
/// and a `close` callback to pop itself off the stack when dismissed.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ChannelContextMenuInner(menu: ChannelContextMenuState, close: EventHandler<()>) -> Element {
    let nav_state: BatchedSignal<crate::state::NavState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();

    let channel_id = menu.channel_id.clone();
    let mut muted = use_signal(|| false);

    let x = menu.x;
    let y = menu.y;

    rsx! {
        // The floating menu itself — backdrop + dismiss handled by the stack host.
        div {
            class: "context-menu",
            style: "left: {x}px; top: {y}px;",
            // Stop clicks from reaching the backdrop.
            onclick: move |evt| evt.stop_propagation(),

            // Mark as Read
            {
                let channel_id = channel_id.clone();
                rsx! {
                    ChannelMenuItem {
                        label: t("channel-menu-mark-read"),
                        onclick: move |_| {
                            mark_channel_as_read(chat_lists, chat_view_state, &channel_id);
                            close.call(());
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

            // Channel Settings — Pack C.3 / P19.
            {
                let server_id = menu.server_id.clone();
                let account_id = menu.account_id.clone();
                let channel_id_for_settings = channel_id.clone();
                let backend_slug = nav_state
                    .read() // poly-lint: allow render-time-read — nav snapshot for channel settings route; subscription intentional
                    .active_backend
                    .cloned().map_or_else(|| "demo".to_string(), |b| b.slug().to_string());
                let instance_id = nav_state
                    .read() // poly-lint: allow render-time-read — nav snapshot for instance_id route param
                    .active_instance_id
                    .cloned()
                    .unwrap_or_default();
                rsx! {
                    ChannelMenuItem {
                        label: t("channel-settings-title"),
                        onclick: move |_| {
                            nav!(Route::ChannelSettingsRoute {
                                backend: backend_slug.clone(),
                                instance_id: instance_id.clone(),
                                account_id: account_id.clone(),
                                server_id: server_id.clone(),
                                channel_id: channel_id_for_settings.clone(),
                            });
                            close.call(());
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Copy Channel ID
            {
                let cid = channel_id.clone();
                rsx! {
                    ChannelMenuItem {
                        label: t("channel-menu-copy-id"),
                        onclick: move |_| {
                            let cid2 = cid.clone();
                            #[allow(clippy::let_underscore_must_use)] let _ = document::eval(&format!("navigator.clipboard.writeText('{cid2}')"));
                            close.call(());
                        },
                    }
                }
            }
        }
    }
}

/// A single clickable item inside the channel context menu.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
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
