//! General server settings — server info and leave server action.
//!
//! The leave server action uses an inline confirm widget (no JS confirm())
//! to avoid triggering the browser native dialog.

use super::super::super::super::routes::Route;
use crate::i18n::{t, t_args};
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;

/// General settings panel for a server.
///
/// Shows server info and a leave-server action with an inline confirm.
#[component]
pub fn ServerGeneralSettings(
    server_id: String,
    server_name: String,
    backend_slug: String,
    account_id: String,
) -> Element {
    let mut show_confirm = use_signal(|| false);

    rsx! {
        div { class: "settings-section",
            h3 { class: "settings-section-title", "{t(\"server-general-info\")}" }
            div { class: "settings-field",
                label { class: "settings-label", "ID" }
                p { class: "settings-value settings-monospace", "{server_id}" }
            }
            div { class: "settings-field",
                label { class: "settings-label", "Name" }
                p { class: "settings-value", "{server_name}" }
            }
        }

        div { class: "settings-section settings-danger-zone",
            h3 { class: "settings-section-title danger", "{t(\"server-general-danger\")}" }

            if show_confirm() {
                // Inline leave-server confirm widget
                LeaveServerConfirm {
                    server_name: server_name.clone(),
                    server_id: server_id.clone(),
                    backend_slug: backend_slug.clone(),
                    account_id: account_id.clone(),
                    oncancel: move |_| show_confirm.set(false),
                }
            } else {
                button {
                    class: "btn-danger",
                    onclick: move |_| show_confirm.set(true),
                    "{t(\"server-menu-leave\")}"
                }
            }
        }
    }
}

/// Inline confirm widget for leaving a server.
///
/// Does NOT use `window.confirm()`. The confirm dialog is rendered in-DOM.
#[component]
fn LeaveServerConfirm(
    server_name: String,
    server_id: String,
    backend_slug: String,
    account_id: String,
    oncancel: EventHandler<MouseEvent>,
) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    let aid_nav = account_id.clone();
    let bslug_nav = backend_slug.clone();
    let sid_remove = server_id.clone();
    // Pre-compute the title using t_args so the Fluent {$name} placeholder is filled
    let leave_title = t_args("leave-server-title", &[("name", server_name.as_str())]);

    rsx! {
        div { class: "leave-server-confirm",
            h4 { class: "leave-server-confirm-title", "{leave_title}" }
            p { class: "leave-server-confirm-body", "{t(\"leave-server-body\")}" }
            div { class: "leave-server-confirm-actions",
                button {
                    class: "btn-secondary",
                    onclick: move |evt| oncancel.call(evt),
                    "{t(\"leave-server-cancel\")}"
                }
                button {
                    class: "btn-danger",
                    onclick: move |_| {
                        // Remove server from chat data
                        let sid = sid_remove.clone();
                        chat_data.write().servers.retain(|s| s.id != sid);
                        chat_data.write().favorited_server_ids.retain(|id| id != &sid);
                        chat_data
                            .write()
                            .account_server_order
                            .values_mut() // Navigate back to the account's DM home
                            .for_each(|v| {
                                v.retain(|id| id != &sid);
                            });
                        if app_state.read().nav.selected_server.as_deref() == Some(&sid) {
                            app_state.write().nav.selected_server = None;
                        }
                        navigator()
                            .replace(Route::DmsHome {
                                backend: bslug_nav.clone(),
                                account_id: aid_nav.clone(),
                            });
                    },
                    "{t(\"leave-server-confirm\")}"
                }
            }
        }
    }
}
