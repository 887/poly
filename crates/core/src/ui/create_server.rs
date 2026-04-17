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

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::ChatData;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// Full-page Create Server form.
///
/// Shows a centered card with a server-name input and a Create button.
/// On success: registers the new server and navigates to
/// `/:backend/:instance_id/:account_id/dms`.
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub(crate) fn CreateServerPage(
    backend: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let client_manager: Signal<ClientManager> = use_context();
    let chat_data: Signal<ChatData> = use_context();

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
                                            CreateSignals { client_manager, chat_data, creating, error_msg },
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
                                navigator().push(Route::DmsHome {
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
                                    CreateSignals { client_manager, chat_data, creating, error_msg },
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
    client_manager: Signal<ClientManager>,
    chat_data: Signal<ChatData>,
    creating: Signal<bool>,
    error_msg: Signal<String>,
}

/// Shared helper — spawns the async create-server task.
///
/// Extracted so both the button click and the Enter-key handler can call it
/// without duplicating the spawn closure.
fn do_create_server(
    name: String,
    account_id: String,
    backend: String,
    instance_id: String,
    signals: CreateSignals,
) {
    let CreateSignals {
        mut client_manager,
        mut chat_data,
        mut creating,
        mut error_msg,
    } = signals;
    let backend_opt = client_manager.read().get_backend(&account_id);
    let Some(backend_arc) = backend_opt else {
        error_msg.set("No backend found for this account".to_string());
        return;
    };
    creating.set(true);
    error_msg.set(String::new());
    spawn(async move {
        let guard = backend_arc.read().await;
        match guard.create_server(&name).await {
            Ok(server) => {
                let server_id = server.id.clone();
                // Register so load_server_data can match backend.
                client_manager
                    .write()
                    .register_server(server_id.clone(), account_id.clone());
                // Only add to servers, NOT to favorited_server_ids.
                {
                    let mut cd = chat_data.write();
                    cd.servers.push(server);
                }
                creating.set(false);
                // Navigate to the new server's home.
                navigator().push(Route::ServerHome {
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
