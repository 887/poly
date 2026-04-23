//! User avatar right-click context menu component.
//!
//! Rendered at the `MainLayout` level so it is never clipped by sidebars.
//! Opened by right-clicking a user avatar `<img>` in message rows.
//!
//! State lives in `AppState.avatar_context_menu`. The `oncontextmenu`
//! handler on the avatar image writes `Some(AvatarContextMenuState)`.
//! A global click on the `MainLayout` root clears it.
//!
//! ## Menu items
//! - View profile — opens the UserProfileModal
//! - Send DM — stub TODO (DM navigation requires a channel ID from the backend)
//! - Mention — copies `@username` to clipboard
//!
//! ## Decision: UserRowContextMenu vs AvatarContextMenu
//!
//! `UserRowContextMenu` (in `context_menu/menus.rs`) is wired to *sidebar user
//! rows* (`DmMemberRow`, `DmContactRow`, `UserSidebar`) and offers "View Profile"
//! + "Copy ID". `AvatarContextMenu` is for *message-row avatar images* and offers
//! "View Profile" + "Send DM" + "Mention". These are separate surfaces with
//! different expected actions, so both menus are kept. The `UserRowContextMenu`
//! annotations on sidebar rows are left untouched.

use crate::i18n::t;
use crate::state::AppState;
use crate::ui::account::common::user_profile_modal::open_user_profile;
use dioxus::prelude::*;
use poly_client::{PresenceStatus, User};
use poly_ui_macros::{context_menu, ui_action};

/// Avatar right-click context menu.
///
/// Reads `AppState.avatar_context_menu` and renders a floating div at the
/// stored coordinates. Renders nothing when the state is `None`.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AvatarContextMenu() -> Element {
    let mut app_state: Signal<AppState> = use_context();

    let Some(menu) = app_state.read().avatar_context_menu.clone() else {
        return rsx! {};
    };

    let x = menu.x;
    let y = menu.y;
    let user_id = menu.user_id.clone();
    let display_name = menu.user_display_name.clone();

    let close = move || {
        app_state.write().avatar_context_menu = None;
    };

    rsx! {
        // Transparent backdrop — closes menu on click and blocks native context menu.
        div {
            class: "context-menu-backdrop",
            onclick: move |_| {
                app_state.write().avatar_context_menu = None;
            },
            oncontextmenu: move |evt| evt.prevent_default(),
        }

        // The floating menu itself.
        div {
            class: "context-menu",
            style: "left: {x}px; top: {y}px;",
            onclick: move |evt| evt.stop_propagation(),

            // Header — user display name
            div { class: "context-menu-label", "{display_name}" }
            div { class: "context-menu-separator" }

            // View profile
            {
                let uid = user_id.clone();
                let dname = display_name.clone();
                let mut close = close;
                rsx! {
                    AvatarMenuItem {
                        label: t("avatar-menu-view-profile"),
                        onclick: move |_| {
                            // Build a minimal User so open_user_profile can render the modal.
                            let user = User {
                                id: uid.clone(),
                                display_name: dname.clone(),
                                avatar_url: None,
                                presence: PresenceStatus::Offline,
                                backend: poly_client::BackendType::Demo,
                            };
                            open_user_profile(app_state, user);
                            close();
                        },
                    }
                }
            }

            // Send DM — TODO: requires resolving a DM channel ID from the backend.
            // Stub logs a debug message until the nav path is available.
            {
                let uid = user_id.clone();
                let mut close = close;
                rsx! {
                    AvatarMenuItem {
                        label: t("avatar-menu-send-dm"),
                        onclick: move |_| {
                            tracing::debug!(
                                target: "poly::context_menu",
                                "send-dm stub: user_id={uid}"
                            );
                            close();
                        },
                    }
                }
            }

            // Mention — copies `@username` to clipboard
            {
                let mention_text = format!("@{display_name}");
                let mut close = close;
                rsx! {
                    AvatarMenuItem {
                        label: t("avatar-menu-mention"),
                        onclick: move |_| {
                            let escaped = mention_text
                                .replace('\\', "\\\\")
                                .replace('`', "\\`");
                            let _eval = document::eval(&format!(
                                "navigator.clipboard && navigator.clipboard.writeText(`{escaped}`);"
                            ));
                            close();
                        },
                    }
                }
            }
        }
    }
}

/// A single clickable item inside the avatar context menu.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AvatarMenuItem(
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
