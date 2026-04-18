//! Plugin-declared non-chat view dispatcher.
//!
//! `ClientView` fetches the per-channel `ViewDescriptor` from the account's
//! `ClientBackend::get_channel_view` (D5) and dispatches to one of four body
//! engines:
//!
//! - [`ListBody`] — paged flat list (HN stories, issues).
//! - [`CardBody`] — grid of cards (Reddit / Mastodon).
//! - [`TreeBody`] — threaded list with depth indentation (Lemmy comments).
//! - [`SplitBody`] — master-detail (GitHub issue + body).
//!
//! If the backend returns `Err(NotSupported(_))` (or any other error) we
//! render a small fallback "no view declared" message. WP 5.C fills in the
//! real view descriptors for Lemmy / HN / GitHub / Forgejo in parallel.

pub mod card_body;
pub mod header;
pub mod list_body;
pub mod split_body;
pub mod toolbar;
pub mod tree_body;

pub use card_body::CardBody;
pub use header::ViewHeader;
pub use list_body::ListBody;
pub use split_body::SplitBody;
pub use toolbar::ViewToolbar;
pub use tree_body::TreeBody;

use crate::client_manager::ClientManager;
use dioxus::prelude::*;
use poly_client::{ClientError, ViewBody, ViewDescriptor};
use poly_ui_macros::{context_menu, ui_action};

/// Host-rendered non-chat view. Reads the active backend's declared
/// `ViewDescriptor` for `channel_id` and routes to the matching body engine.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ClientView(channel_id: String, account_id: String) -> Element {
    let client_manager: Signal<ClientManager> = use_context();

    let desc_res = {
        let account_id = account_id.clone();
        let channel_id = channel_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            let channel_id = channel_id.clone();
            async move {
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = backend.read().await;
                guard.get_channel_view(&channel_id).await
            }
        })
    };

    match &*desc_res.read_unchecked() {
        None => rsx! {
            div { class: "client-view client-view-loading",
                span { "Loading view…" }
            }
        },
        Some(Err(err)) => {
            tracing::debug!("ClientView: get_channel_view failed: {err:?}");
            rsx! {
                div { class: "client-view client-view-error",
                    div { class: "view-error", "No view declared" }
                }
            }
        }
        Some(Ok(desc)) => {
            let desc: ViewDescriptor = desc.clone();
            render_descriptor(channel_id.clone(), account_id.clone(), desc)
        }
    }
}

fn render_descriptor(channel_id: String, account_id: String, desc: ViewDescriptor) -> Element {
    let header = desc.header.clone();
    let toolbar = desc.toolbar.clone();
    let body = desc.body.clone();
    // D30 — parent-owned filter + refresh signals; toolbar writes, bodies
    // read. A non-forum view that never shows the filter input still has
    // these signals sitting at their defaults (empty string / tick=0) and
    // the body engines short-circuit their filter pass.
    let filter = use_signal(String::new);
    let refresh_tick = use_signal(|| 0u32);
    let filter_str = filter.read().clone();
    rsx! {
        div { class: "client-view",
            if let Some(h) = header {
                ViewHeader { header: h }
            }
            if let Some(t) = toolbar {
                ViewToolbar { toolbar: t, filter, refresh_tick }
            }
            div { class: "client-view-body",
                {
                    match body {
                        ViewBody::ListBody(spec) => rsx! {
                            ListBody {
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                spec,
                                filter: filter_str.clone(),
                            }
                        },
                        ViewBody::CardBody(spec) => rsx! {
                            CardBody {
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                spec,
                            }
                        },
                        ViewBody::TreeBody(spec) => rsx! {
                            TreeBody {
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                spec,
                                filter: filter_str.clone(),
                            }
                        },
                        ViewBody::SplitBody(spec) => rsx! {
                            SplitBody {
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                spec,
                            }
                        },
                    }
                }
            }
        }
    }
}
