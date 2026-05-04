//! User avatar right-click context menu component.
//!
//! Rendered via the `ContextMenuStack` host at the `MainLayout` level.
//! Opened by right-clicking a user avatar `<img>` in message rows.
//! State is pushed onto `AppState.context_menu_stack`.
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
//! plus "Copy ID". `AvatarContextMenu` is for *message-row avatar images* and
//! offers "View Profile", "Send DM", and "Mention". These are separate surfaces
//! with different expected actions, so both menus are kept. The
//! `UserRowContextMenu` annotations on sidebar rows are left untouched.

use crate::i18n::t;
use crate::state::{AppState, AvatarContextMenuState, BatchedSignal, UiOverlays};
use crate::ui::account::common::user_profile_modal::open_user_profile;
use dioxus::prelude::*;
use poly_client::{PresenceStatus, User};
use poly_ui_macros::{context_menu, ui_action};

/// Avatar right-click context menu — stack-based inner component.
///
/// Receives the deserialized `AvatarContextMenuState` from the stack host
/// and a `close` callback to pop itself off the stack.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AvatarContextMenuInner(menu: AvatarContextMenuState, close: EventHandler<()>) -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();

    let x = menu.x;
    let y = menu.y;
    let user_id = menu.user_id.clone();
    let display_name = menu.user_display_name.clone();

    rsx! {
        // The floating menu itself — backdrop + dismiss handled by the stack host.
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
                                backend: poly_client::BackendType::from("demo"),
                            };
                            open_user_profile(ui_overlays, user);
                            close.call(());
                        },
                    }
                }
            }

            // Send DM — TODO: requires resolving a DM channel ID from the backend.
            // Stub logs a debug message until the nav path is available.
            {
                let uid = user_id.clone();
                rsx! {
                    AvatarMenuItem {
                        label: t("avatar-menu-send-dm"),
                        onclick: move |_| {
                            tracing::debug!(
                                target: "poly::context_menu",
                                "send-dm stub: user_id={uid}"
                            );
                            close.call(());
                        },
                    }
                }
            }

            // Mention — copies `@username` to clipboard
            {
                let mention_text = format!("@{display_name}");
                rsx! {
                    AvatarMenuItem {
                        label: t("avatar-menu-mention"),
                        onclick: move |_| {
                            let escaped = mention_text
                                .replace('\\', "\\\\")
                                .replace('`', "\\`");
                            // lint-allow-unused: Eval is fire-and-forget here (Copy + Future).
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = document::eval(&format!(
                                "navigator.clipboard && navigator.clipboard.writeText(`{escaped}`);"
                            ));
                            close.call(());
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
