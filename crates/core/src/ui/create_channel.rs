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

use crate::state::BatchedSignal;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{ChannelType, ServerAdminBackend};
use poly_ui_macros::{context_menu, ui_action};
use tracing::{error, info};

/// Typed actions for the Create Channel modal form.
pub enum CreateChannelAction {
    Submit,
    Cancel,
}

impl crate::ui::actions::UiAction for CreateChannelAction {
    fn apply(self, cx: crate::ui::actions::ActionCx<'_>) {
        // Submit and Cancel operate on component-local Signals (channel_name, creating,
        // error_msg) and need server/account props not carried in ActionCx.
        // The component handles both variants inline via onclick and onkeydown handlers.
        // This apply() exists so the Action contract compiles; Cancel can navigate via
        // the navigator when the exact route props are not needed.
        match self {
            Self::Submit => {
                // Submit requires channel_name / server_id / account_id / backend props
                // which are component-local. Handled inline by do_create_channel().
                tracing::debug!(
                    target: "poly_core::ui::create_channel",
                    "CreateChannelAction::Submit — handled inline by component"
                );
            }
            Self::Cancel => {
                // Best-effort: navigate back via the browser history.
                if let Some(nav) = cx.navigator {
                    nav.go_back();
                }
            }
        }
    }
}

/// Full-page Create Channel form rendered in the main content area.
///
/// The left-hand `ChannelList` sidebar (with existing channels) remains visible
/// because this route is rendered inside `ServerLayout`'s `Outlet`.
#[rustfmt::skip]
#[ui_action(CreateChannelAction)]
#[context_menu(none)]
#[component]
pub(crate) fn CreateChannelPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<crate::state::ChatLists> = use_context();
    let chat_view_state: BatchedSignal<crate::state::ChatViewState> = use_context();

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
                                            CreateChannelSignals { client_manager, chat_lists, chat_view_state, creating, error_msg },
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
                                crate::nav!(Route::ServerHome {
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
                                    CreateChannelSignals { client_manager, chat_lists, chat_view_state, creating, error_msg },
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
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<crate::state::ChatLists>,
    chat_view_state: BatchedSignal<crate::state::ChatViewState>,
    creating: Signal<bool>,
    error_msg: Signal<String>,
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
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
        chat_lists,
        chat_view_state,
        mut creating,
        mut error_msg,
    } = signals;
    info!("do_create_channel: spawning task");
    creating.set(true);
    error_msg.set(String::new());
    spawn(async move {
        info!("do_create_channel: async task started");
        match client_manager.peek().with_backend_for_server(&server_id, async |_aid, b| {
            match b.as_server_admin() {
                Some(sa) => sa.create_channel(&server_id, &name, ChannelType::Text).await,
                None => Err(poly_client::ClientError::NotSupported("backend does not support channel creation".to_string())),
            }
        }).await {
            Ok(channel) => {
                info!("do_create_channel: channel created id={:?}", channel.id);
                let channel_id = channel.id.clone();
                // Pre-set current_channel and selected_channel BEFORE navigating.
                // This mirrors what ChannelItemRow.onclick does so that
                // ServerChat.use_effect sees already_loaded=true and skips
                // restore_server_channel (which panics with concurrent spawns).
                let channel_for_cv = channel.clone();
                chat_lists.batch(move |cl| {
                    cl.set_channels({
                        let mut chs = cl.channels.clone();
                        chs.push(channel);
                        chs
                    });
                });
                chat_view_state.batch(move |cv| {
                    cv.current_channel = Some(channel_for_cv);
                    cv.set_messages(Vec::new());
                    cv.members = Vec::new();
                    cv.loading = false;
                });
                creating.set(false);
                crate::nav!(Route::ServerChat {
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
