//! Create Channel — full-page form rendered inside `ServerLayout`.
//!
//! Navigated to from the "+ New Channel" button in the channel list sidebar for
//! Poly-server accounts.  `FavoritesBar`, `AccountServerBar`, and the
//! `ChannelList` (with the server's existing channels) all stay visible on the
//! left because this route lives inside `ServerLayout`'s `Outlet`.
//!
//! On success, the new channel is pushed to `ChatData::channels` and the router
//! navigates to the new channel's `ServerChat` route.
//!
//! ## 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, ChatData};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::ChannelType;
use tracing::{error, info};

/// Full-page Create Channel form rendered in the main content area.
///
/// The left-hand `ChannelList` sidebar (with existing channels) remains visible
/// because this route is rendered inside `ServerLayout`'s `Outlet`.
#[rustfmt::skip]
#[component]
pub(crate) fn CreateChannelPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    let client_manager: Signal<ClientManager> = use_context();
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    let mut channel_name = use_signal(String::new);
    let creating = use_signal(|| false);
    let error_msg = use_signal(String::new);

    // Clones for the Cancel navigation closure.
    let backend_nav     = backend.clone();
    let instance_id_nav = instance_id.clone();
    let account_id_nav  = account_id.clone();
    let server_id_nav   = server_id.clone();

    // Clones for the Enter-key handler.
    let backend_kd     = backend.clone();
    let instance_id_kd = instance_id.clone();
    let account_id_kd  = account_id.clone();
    let server_id_kd   = server_id.clone();

    rsx! {
        div { class: "create-server-page",
            div { class: "create-server-card",
                h1 { class: "create-server-card-title", "{t(\"create-channel-page-title\")}" }
                p  { class: "create-server-card-subtitle", "{t(\"create-channel-page-subtitle\")}" }

                div { class: "create-server-card-body",
                    label { class: "create-server-label",
                        "{t(\"create-channel-page-label\")}"
                        input {
                            r#type: "text",
                            class: "create-server-page-input",
                            placeholder: "{t(\"create-channel-placeholder\")}",
                            value: "{channel_name}",
                            oninput: move |e| channel_name.set(e.value()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    let name = channel_name.read().trim().to_string();
                                    if !name.is_empty() && !*creating.read() {
                                        do_create_channel(
                                            name,
                                            server_id_kd.clone(),
                                            account_id_kd.clone(),
                                            backend_kd.clone(),
                                            instance_id_kd.clone(),
                                            CreateChannelSignals { client_manager, app_state, chat_data, creating, error_msg },
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
                                navigator().push(Route::ServerHome {
                                    backend:     backend_nav.clone(),
                                    instance_id: instance_id_nav.clone(),
                                    account_id:  account_id_nav.clone(),
                                    server_id:   server_id_nav.clone(),
                                });
                            },
                            "{t(\"create-channel-cancel\")}"
                        }
                        button {
                            class: "btn btn-primary",
                            disabled: channel_name.read().trim().is_empty() || *creating.read(),
                            onclick: move |_| {
                                let name = channel_name.read().trim().to_string();
                                if name.is_empty() || *creating.read() { return; }
                                do_create_channel(
                                    name,
                                    server_id.clone(),
                                    account_id.clone(),
                                    backend.clone(),
                                    instance_id.clone(),
                                    CreateChannelSignals { client_manager, app_state, chat_data, creating, error_msg },
                                );
                            },
                            if *creating.read() { "{t(\"create-channel-creating\")}" } else { "{t(\"create-channel-submit\")}" }
                        }
                    }
                }
            }
        }
    }
}

/// Bundle of mutable signals passed to the create-channel async task.
struct CreateChannelSignals {
    client_manager: Signal<ClientManager>,
    app_state: Signal<AppState>,
    chat_data: Signal<ChatData>,
    creating: Signal<bool>,
    error_msg: Signal<String>,
}

/// Shared helper — spawns the async create-channel task.
///
/// Extracted so both the button click and the Enter-key handler can call it
/// without duplicating the spawn closure.
fn do_create_channel(
    name: String,
    server_id: String,
    account_id: String,
    backend: String,
    instance_id: String,
    signals: CreateChannelSignals,
) {
    info!("do_create_channel: name={name:?} server_id={server_id:?}");
    let CreateChannelSignals {
        client_manager,
        mut app_state,
        mut chat_data,
        mut creating,
        mut error_msg,
    } = signals;
    let backend_opt = client_manager.read().get_backend_for_server(&server_id);
    let Some((_acct_id, backend_arc)) = backend_opt else {
        let msg = format!("No backend found for server {server_id:?}");
        error!("do_create_channel: {msg}");
        error_msg.set(msg);
        return;
    };
    info!("do_create_channel: backend found, spawning task");
    creating.set(true);
    error_msg.set(String::new());
    spawn(async move {
        info!("do_create_channel: async task started");
        let guard = backend_arc.read().await;
        info!("do_create_channel: backend lock acquired, calling create_channel");
        match guard
            .create_channel(&server_id, &name, ChannelType::Text)
            .await
        {
            Ok(channel) => {
                info!("do_create_channel: channel created id={:?}", channel.id);
                let channel_id = channel.id.clone();
                // Pre-set current_channel and selected_channel BEFORE navigating.
                // This mirrors what ChannelItemRow.onclick does so that
                // ServerChat.use_effect sees already_loaded=true and skips
                // restore_server_channel (which panics with concurrent spawns).
                {
                    let mut cd = chat_data.write();
                    cd.channels.push(channel.clone());
                    cd.current_channel = Some(channel);
                    cd.messages = Vec::new();
                    cd.members = Vec::new();
                    cd.loading = false;
                }
                app_state.write().nav.selected_channel = Some(channel_id.clone());
                creating.set(false);
                navigator().push(Route::ServerChat {
                    backend,
                    instance_id,
                    account_id,
                    server_id,
                    channel_id,
                });
            }
            Err(e) => {
                error!("do_create_channel: create_channel error: {e}");
                error_msg.set(e.to_string());
                creating.set(false);
            }
        }
    });
}
