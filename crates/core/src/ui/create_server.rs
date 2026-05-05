//! Create Server — full-page form rendered inside `MainLayout`.
//!
//! Navigated to from the "+" button in the AccountServerBar for Poly accounts.
//! Both `FavoritesBar` (bar 1) and `AccountServerBar` (bar 2) remain visible
//! while this page is active, because the route is inside `MainLayout`.
//!
//! On success, the new server is committed to `ChatData` and the user is
//! navigated to the DMs home for their account.
//!
//! ## 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines.

use crate::state::BatchedSignal;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::ChatLists;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Typed actions for the Create Server modal form.
pub enum CreateServerAction {
    Submit,
    Cancel,
}

impl crate::ui::actions::UiAction for CreateServerAction {
    fn apply(self, _cx: crate::ui::actions::ActionCx<'_>) {
        match self {
            Self::Submit => todo!("phase-E: submit create-server form"),
            Self::Cancel => todo!("phase-E: cancel create-server and navigate back"),
        }
    }
}

/// Full-page Create Server form.
///
/// Shows a centered card with a server-name input and a Create button.
/// On success: registers the new server and navigates to
/// `/:backend/:instance_id/:account_id/dms`.
#[rustfmt::skip]
#[ui_action(CreateServerAction)]
#[context_menu(none)]
#[component]
pub(crate) fn CreateServerPage(
    backend: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

    let mut server_name = use_signal(String::new);
    let creating = use_signal(|| false);
    let error_msg = use_signal(String::new);

    let account_id_nav = account_id.clone();
    let backend_nav   = backend.clone();
    let instance_id_nav = instance_id.clone();

    // Clones for the onkeydown closure
    let account_id_kd = account_id.clone();
    let backend_kd    = backend.clone();
    let instance_id_kd = instance_id.clone();

    rsx! {
        div { class: "create-server-page",
            div { class: "create-server-card",
                h1 { class: "create-server-card-title", "{t(\"create-server-page-title\")}" }
                p  { class: "create-server-card-subtitle", "{t(\"create-server-page-subtitle\")}" }

                div { class: "create-server-card-body",
                    label { class: "create-server-label",
                        "{t(\"create-server-page-label\")}"
                        input {
                            r#type: "text",
                            class: "create-server-page-input",
                            placeholder: "{t(\"create-server-placeholder\")}",
                            value: "{server_name}",
                            oninput: move |e| server_name.set(e.value()),
                            // Submit on Enter key
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    let name = server_name.read().trim().to_string();
                                    if !name.is_empty() && !*creating.read() {
                                        do_create_server(
                                            name,
                                            account_id_kd.clone(),
                                            backend_kd.clone(),
                                            instance_id_kd.clone(),
                                            CreateSignals { client_manager, chat_lists, creating, error_msg },
                                        );
                                    }
                                }
                            },
                        }
                    }

                    if !error_msg.read().is_empty() {
                        p { class: "create-server-page-error", "{error_msg}" }
                    }

                    div { class: "create-server-card-actions",
                        button {
                            class: "btn btn-secondary",
                            onclick: move |_| {
                                crate::nav!(Route::DmsHome {
                                    backend:     backend_nav.clone(),
                                    instance_id: instance_id_nav.clone(),
                                    account_id:  account_id_nav.clone(),
                                });
                            },
                            "{t(\"create-server-cancel\")}"
                        }
                        button {
                            class: "btn btn-primary",
                            disabled: server_name.read().trim().is_empty() || *creating.read(),
                            onclick: move |_| {
                                let name = server_name.read().trim().to_string();
                                if name.is_empty() || *creating.read() { return; }
                                do_create_server(
                                    name,
                                    account_id.clone(),
                                    backend.clone(),
                                    instance_id.clone(),
                                    CreateSignals { client_manager, chat_lists, creating, error_msg },
                                );
                            },
                            if *creating.read() { "{t(\"create-server-creating\")}" } else { "{t(\"create-server-submit\")}" }
                        }
                    }
                }
            }
        }
    }
}

/// Bundle of mutable signals passed to the create-server async task.
struct CreateSignals {
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<ChatLists>,
    creating: Signal<bool>,
    error_msg: Signal<String>,
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn do_create_server(
    name: String,
    account_id: String,
    backend: String,
    instance_id: String,
    signals: CreateSignals,
) {
    let CreateSignals {
        client_manager,
        chat_lists,
        mut creating,
        mut error_msg,
    } = signals;
    creating.set(true);
    error_msg.set(String::new());
    spawn(async move {
        match client_manager.peek().with_backend(&account_id, async |b| {
            b.create_server(&name).await
        }).await {
            Ok(server) => {
                let server_id = server.id.clone();
                // Register so load_server_data can match backend.
                let sid = server_id.clone();
                let aid = account_id.clone();
                client_manager.batch(move |cm| cm.register_server(sid, aid));
                // Only add to servers, NOT to favorited_server_ids.
                chat_lists.batch(move |cl| cl.push_server(server));
                creating.set(false);
                // Navigate to the new server's home.
                crate::nav!(Route::ServerHome {
                    backend,
                    instance_id,
                    account_id,
                    server_id,
                });
            }
            Err(e) => {
                error_msg.set(e.to_string());
                creating.set(false);
            }
        }
    });
}
