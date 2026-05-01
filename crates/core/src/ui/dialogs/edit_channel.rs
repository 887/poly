//! Edit channel dialog.
//!
//! Renders an overlay form for editing channel name, topic, and slow-mode.
//! Calls `ClientBackend::update_channel` on save.
//! Gated by `BackendCapabilities::has_channel_mgmt`.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, BatchedSignal};
use dioxus::prelude::*;
use poly_client::UpdateChannelParams;
use poly_ui_macros::{context_menu, ui_action};

/// Edit channel dialog — name, topic, slow-mode (hidden for Teams), NSFW toggle (hidden for Teams).
///
/// When `backend_slug` is `"teams"`, only name and description fields are shown —
/// Teams/Graph has no slow-mode or NSFW concept. Don't render disabled fields;
/// hide them entirely per the "accurate to backend" UX decision.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn EditChannelDialog(
    channel_id: String,
    account_id: String,
    /// Backend slug (e.g. `"teams"`, `"discord"`) — drives per-backend field gating.
    #[props(default)]
    backend_slug: String,
    on_close: EventHandler<()>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut topic = use_signal(String::new);
    let mut slowmode = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg = use_signal(String::new);
    let mut success = use_signal(|| false);

    // Teams only supports name + description — hide slow-mode and nsfw entirely.
    let show_slowmode = backend_slug != "teams";

    let client_manager: BatchedSignal<ClientManager> = use_context();
    let app_state: BatchedSignal<AppState> = use_context();

    rsx! {
        div { class: "modal-backdrop",
            onclick: move |_| on_close.call(()),
        }
        div { class: "modal-card",
            onclick: move |evt| evt.stop_propagation(),

            div { class: "modal-header",
                h2 { class: "modal-title", "{t(\"dialog-edit-channel-title\")}" }
            }

            div { class: "modal-body",
                if *success.read() {
                    p { class: "modal-success", "{t(\"dialog-edit-channel-success\")}" }
                } else {
                    label { class: "modal-label",
                        "{t(\"dialog-edit-channel-name\")}"
                        input {
                            r#type: "text",
                            class: "modal-input",
                            placeholder: "{t(\"dialog-edit-channel-name\")}",
                            value: "{name}",
                            oninput: move |e| name.set(e.value()),
                        }
                    }
                    label { class: "modal-label",
                        "{t(\"dialog-edit-channel-topic\")}"
                        input {
                            r#type: "text",
                            class: "modal-input",
                            placeholder: "{t(\"dialog-edit-channel-topic\")}",
                            value: "{topic}",
                            oninput: move |e| topic.set(e.value()),
                        }
                    }
                    if show_slowmode {
                        label { class: "modal-label",
                            "{t(\"dialog-edit-channel-slowmode\")}"
                            input {
                                r#type: "number",
                                class: "modal-input",
                                min: "0",
                                max: "21600",
                                placeholder: "0",
                                value: "{slowmode}",
                                oninput: move |e| slowmode.set(e.value()),
                            }
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
                        class: "btn btn-primary",
                        disabled: *submitting.read(),
                        onclick: {
                            let channel_id = channel_id.clone();
                            let account_id = account_id.clone();
                            move |_| {
                                if *submitting.read() { return; }
                                let name_str = name.read().trim().to_string();
                                let topic_str = topic.read().trim().to_string();
                                // Only parse slow-mode when the field is shown (not Teams).
                                let slow_mode = if show_slowmode {
                                    slowmode.read().trim().parse::<u32>().ok()
                                } else {
                                    None
                                };

                                // At least one field must be non-empty to make the call.
                                if name_str.is_empty() && topic_str.is_empty() && slow_mode.is_none() {
                                    error_msg.set("Enter at least one field to update.".to_string());
                                    return;
                                }

                                let params = UpdateChannelParams {
                                    name: if name_str.is_empty() { None } else { Some(name_str) },
                                    topic: if topic_str.is_empty() { None } else { Some(topic_str) },
                                    slow_mode_secs: slow_mode,
                                    nsfw: None,
                                    position: None,
                                };

                                let backend_opt = client_manager.read().get_backend(&account_id);
                                let Some(backend_arc) = backend_opt else {
                                    error_msg.set(format!("No backend for account {account_id}"));
                                    return;
                                };
                                submitting.set(true);
                                error_msg.set(String::new());
                                let cid = channel_id.clone();
                                spawn(async move {
                                    let guard = backend_arc.read().await;
                                    match guard.update_channel(&cid, params).await {
                                        Ok(_) => {
                                            submitting.set(false);
                                            success.set(true);
                                            app_state.batch(|st| st.active_moderation_dialog = None);
                                        }
                                        Err(e) => {
                                            submitting.set(false);
                                            let msg = t("dialog-edit-channel-error").replace("{ $error }", &e.to_string()).replace("{$error}", &e.to_string());
                                            error_msg.set(msg);
                                        }
                                    }
                                });
                            }
                        },
                        if *submitting.read() { "…" } else { "{t(\"dialog-edit-channel-save\")}" }
                    }
                }
            }
        }
    }
}
