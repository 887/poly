//! Kick member confirmation dialog.
//!
//! Renders a confirmation overlay with an optional reason field.
//! Calls `ClientBackend::kick_member` on confirm.
//! Gated by `BackendCapabilities::has_kick`.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, BatchedSignal};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Kick member confirmation dialog.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn KickMemberDialog(
    server_id: String,
    member_id: String,
    member_name: String,
    account_id: String,
    on_close: EventHandler<()>,
) -> Element {
    let mut reason = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg = use_signal(String::new);
    let mut success = use_signal(|| false);

    let client_manager: BatchedSignal<ClientManager> = use_context();
    let ui_overlays: crate::state::BatchedSignal<crate::state::UiOverlays> = use_context();

    let title = t("dialog-kick-title")
        .replace("{ $user }", &member_name)
        .replace("{$user}", &member_name);

    rsx! {
        div { class: "modal-backdrop",
            onclick: move |_| on_close.call(()),
        }
        div { class: "modal-card",
            onclick: move |evt| evt.stop_propagation(),

            div { class: "modal-header",
                h2 { class: "modal-title", "{title}" }
            }

            div { class: "modal-body",
                if *success.read() {
                    p { class: "modal-success", "{t(\"dialog-kick-success\")}" }
                } else {
                    label { class: "modal-label",
                        "{t(\"dialog-kick-reason\")}"
                        input {
                            r#type: "text",
                            class: "modal-input",
                            placeholder: "{t(\"dialog-kick-reason\")}",
                            value: "{reason}",
                            oninput: move |e| reason.set(e.value()),
                        }
                    }
                    if !error_msg.read().is_empty() {
                        p { class: "modal-error", "{error_msg}" }
                    }
                }
            }

            div { class: "modal-footer",
                button {
                    class: "btn btn-secondary",
                    onclick: move |_| on_close.call(()),
                    "{t(\"dialog-cancel\")}"
                }
                if !*success.read() {
                    button {
                        class: "btn btn-danger",
                        disabled: *submitting.read(),
                        onclick: {
                            let server_id = server_id.clone();
                            let member_id = member_id.clone();
                            let account_id = account_id.clone();
                            move |_| {
                                if *submitting.read() { return; }
                                let reason_str = reason.read().trim().to_string();
                                let reason_opt = if reason_str.is_empty() { None } else { Some(reason_str) };
                                submitting.set(true);
                                error_msg.set(String::new());
                                let sid = server_id.clone();
                                let mid = member_id.clone();
                                let aid = account_id.clone();
                                spawn(async move {
                                    match client_manager.peek().with_backend(&aid, async |b| {
                                        b.kick_member(&sid, &mid, reason_opt.as_deref()).await
                                    }).await {
                                        Ok(()) => {
                                            submitting.set(false);
                                            success.set(true);
                                            // Clear dialog from app state after short delay.
                                            ui_overlays.batch(|o| o.active_moderation_dialog = None);
                                        }
                                        Err(e) => {
                                            submitting.set(false);
                                            let msg = t("dialog-kick-error").replace("{ $error }", &e.to_string()).replace("{$error}", &e.to_string());
                                            error_msg.set(msg);
                                        }
                                    }
                                });
                            }
                        },
                        if *submitting.read() { "…" } else { "{t(\"dialog-kick-confirm\")}" }
                    }
                }
            }
        }
    }
}
