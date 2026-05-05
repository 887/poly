//! Code repository explorer view (`ChannelType::Code`).
//!
//! Two-pane layout: a directory listing on the left and the contents of the
//! selected file on the right. Backed by [`poly_client::CodeRepoBackend`] via
//! the `as_code_repo()` capability accessor — works against any backend that
//! opts in to `CodeRepoBackend` (currently `poly-github` and `poly-forgejo`).
//!
//! Search is intentionally external: the view shows a "Search on the web"
//! button that opens the backend's instance URL with the relevant search
//! params (per the github backend doc-comment) so users get the full code
//! search experience without us having to host an index.

use crate::state::BatchedSignal;
use dioxus::prelude::*;
use poly_client::FileEntry;

use crate::state::{AccountSessions, ChatViewState};
use poly_ui_macros::{context_menu, ui_action};

/// Two-pane explorer rendered when the current channel is `ChannelType::Code`.
#[ui_action(inherit)]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn CodeExplorerView(#[props(default)] route_channel_id: String) -> Element {
    let chat_view_state = use_context::<BatchedSignal<ChatViewState>>();
    let account_sessions = use_context::<BatchedSignal<AccountSessions>>();
    let client_manager = use_context::<BatchedSignal<crate::client_manager::ClientManager>>();

    let (channel_id, server_id) = {
        let cv = chat_view_state.read();
        let ch_id = if route_channel_id.is_empty() {
            cv.current_channel.as_ref().map(|c| c.id.clone())
        } else {
            Some(route_channel_id.clone())
        };
        (
            ch_id,
            cv.current_server.as_ref().map(|s| s.id.clone()),
        )
    };

    // Resolve the optional backend_url for the server's account session, used
    // for the external "Search on web" button.
    let backend_url: Option<String> = server_id.as_ref().and_then(|sid| {
        let acct = client_manager
            .read()
            .get_backend_for_server(sid)
            .map(|(a, _)| a)?;
        account_sessions
            .read()
            .account_sessions
            .get(&acct)
            .and_then(|s| s.backend_url.clone())
    });

    let mut current_path = use_signal(String::new);
    let mut entries: Signal<Vec<FileEntry>> = use_signal(Vec::new);
    let mut selected_file: Signal<Option<String>> = use_signal(|| None);
    let mut file_text: Signal<Option<String>> = use_signal(|| None);
    let mut loading = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    // Refetch the directory listing whenever the channel or path changes.
    {
        let channel_id = channel_id.clone();
        let server_id_for_eff = server_id.clone();
        use_effect(move || {
            let Some(ch_id) = channel_id.clone() else { return; };
            let Some(srv_id) = server_id_for_eff.clone() else { return; };
            let path = current_path.read().clone();
            let cm = client_manager;
            spawn(async move {
                loading.set(true);
                error_msg.set(None);
                let backend = cm.read().get_backend_for_server(&srv_id);
                let Some((_acct, handle)) = backend else {
                    error_msg.set(Some("backend not loaded".into()));
                    loading.set(false);
                    return;
                };
                let guard = handle.read().await;
                // H.2.a — capability-gate: only backends that impl CodeRepoBackend
                // have code channels; if accessor returns None, the channel type
                // would never have been ChannelType::Code, so this is defensive.
                match guard.as_code_repo() {
                    Some(cr) => match cr.list_files(&ch_id, &path).await {
                        Ok(list) => entries.set(list),
                        Err(e) => error_msg.set(Some(format!("{e}"))),
                    },
                    None => error_msg.set(Some("backend does not support code channels".into())),
                }
                loading.set(false);
            });
        });
    }

    let path_display = current_path.read().clone();
    let breadcrumb = if path_display.is_empty() {
        "/".to_string()
    } else {
        format!("/{path_display}")
    };

    rsx! {
        div { class: "code-explorer",
            div { class: "code-explorer-toolbar",
                button {
                    class: "btn btn-secondary code-explorer-up",
                    disabled: current_path.read().is_empty(),
                    onclick: move |_| {
                        let p = current_path.read().clone();
                        let parent = p.rsplit_once('/').map(|(a, _)| a.to_string()).unwrap_or_default();
                        current_path.set(parent);
                        selected_file.set(None);
                        file_text.set(None);
                    },
                    "↑ Up"
                }
                span { class: "code-explorer-path", "{breadcrumb}" }
                if let Some(url) = backend_url.clone() {
                    a {
                        class: "btn btn-secondary code-explorer-search",
                        href: format_search_url(&url, &server_id.clone().unwrap_or_default()),
                        target: "_blank",
                        rel: "noopener",
                        "Search on web"
                    }
                }
            }

            div { class: "code-explorer-body",
                div { class: "code-explorer-tree",
                    if *loading.read() {
                        p { class: "code-explorer-loading", "Loading…" }
                    }
                    if let Some(err) = error_msg.read().as_ref() {
                        p { class: "code-explorer-error", "{err}" }
                    }
                    ul { class: "code-explorer-list",
                        for entry in entries.read().iter().cloned() {
                            CodeExplorerEntry {
                                entry: entry.clone(),
                                is_selected: selected_file.read().as_deref() == Some(&entry.path),
                                on_open: {
                                    let entry = entry.clone();
                                    let ch_id = channel_id.clone();
                                    let srv_id = server_id.clone();
                                    EventHandler::new(move |_| {
                                        if entry.kind == poly_client::FileKind::Directory {
                                            current_path.set(entry.path.clone());
                                            selected_file.set(None);
                                            file_text.set(None);
                                        } else {
                                            let (Some(ch_id), Some(srv_id)) = (ch_id.clone(), srv_id.clone()) else { return; };
                                            let path = entry.path.clone();
                                            let cm = client_manager;
                                            selected_file.set(Some(path.clone()));
                                            spawn(async move {
                                                file_text.set(Some("Loading…".into()));
                                                let backend = cm.read().get_backend_for_server(&srv_id);
                                                let Some((_acct, handle)) = backend else {
                                                    file_text.set(Some("backend not loaded".into()));
                                                    return;
                                                };
                                                let guard = handle.read().await;
                                                // H.2.a — capability-gate via CodeRepoBackend accessor.
                                                match guard.as_code_repo() {
                                                    Some(cr) => match cr.read_file(&ch_id, &path).await {
                                                        Ok(content) => {
                                                            let text = String::from_utf8_lossy(&content.bytes).into_owned();
                                                            file_text.set(Some(text));
                                                        }
                                                        Err(e) => file_text.set(Some(format!("{e}"))),
                                                    },
                                                    None => file_text.set(Some("backend does not support code channels".into())),
                                                }
                                            });
                                        }
                                    })
                                }
                            }
                        }
                    }
                }
                div { class: "code-explorer-content",
                    if let Some(path) = selected_file.read().as_ref() {
                        h4 { class: "code-explorer-filename", "{path}" }
                    }
                    pre { class: "code-explorer-source",
                        code {
                            "{file_text.read().clone().unwrap_or_else(|| String::from(\"Select a file to view its contents.\"))}"
                        }
                    }
                }
            }
        }
    }
}

/// One row in the file/directory list. Extracted so the parent component stays
/// under the 150-line cap.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn CodeExplorerEntry(entry: FileEntry, is_selected: bool, on_open: EventHandler<()>) -> Element {
    let icon = match entry.kind {
        poly_client::FileKind::Directory => "📁",
        poly_client::FileKind::File => "📄",
        poly_client::FileKind::Symlink => "🔗",
        poly_client::FileKind::Submodule => "📦",
    };
    let class = if is_selected {
        "code-explorer-entry selected"
    } else {
        "code-explorer-entry"
    };
    rsx! {
        li {
            class: "{class}",
            onclick: move |_| on_open.call(()),
            span { class: "code-explorer-icon", "{icon}" }
            span { class: "code-explorer-name", "{entry.name}" }
        }
    }
}

/// Build an external code-search URL from a server's `backend_url` and id.
///
/// The github backend's docs say to redirect to
/// `https://{instance}/{owner}/{repo}/search?type=code&q=`. We don't have the
/// owner/repo split here, so we link to the instance root and let the user
/// type the query — UI will be refined when more code backends exist.
fn format_search_url(backend_url: &str, server_id: &str) -> String {
    let _ = server_id;
    format!("{backend_url}/search?type=code")
}
