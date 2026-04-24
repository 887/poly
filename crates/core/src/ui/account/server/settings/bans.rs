//! Bans tab for per-server settings.
//!
//! Lists banned members; provides an unban button for each entry.
//! Gated by `BackendCapabilities::has_ban`.

use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use crate::i18n::t;
use dioxus::prelude::*;
use poly_client::BannedMember;
use poly_ui_macros::{context_menu, ui_action};

/// Bans tab component — shows the list of banned members and unban controls.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn BansTab(server_id: String, account_id: String) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let bans_resource = {
        let server_id = server_id.clone();
        let account_id = account_id.clone();
        use_resource(move || {
            let server_id = server_id.clone();
            let account_id = account_id.clone();
            let client_manager = client_manager;
            async move {
                let Some(backend_arc) = client_manager.read().get_backend(&account_id) else {
                    return Err("No backend".to_string());
                };
                let guard = backend_arc.read().await;
                guard.get_bans(&server_id).await.map_err(|e| e.to_string())
            }
        })
    };

    let bans_snapshot = bans_resource.read_unchecked().as_ref().cloned();

    rsx! {
        div { class: "bans-tab",
            match &bans_snapshot {
                None => rsx! {
                    p { class: "tab-loading", "{t(\"bans-tab-loading\")}" }
                },
                Some(Err(e)) => rsx! {
                    p { class: "tab-error", "{e}" }
                },
                Some(Ok(bans)) if bans.is_empty() => rsx! {
                    p { class: "tab-empty", "{t(\"bans-tab-empty\")}" }
                },
                Some(Ok(bans)) => rsx! {
                    div { class: "bans-list",
                        for ban in bans.iter() {
                            {render_ban_row(ban.clone(), server_id.clone(), account_id.clone(), client_manager, bans_resource)}
                        }
                    }
                },
            }
        }
    }
}

fn render_ban_row(
    ban: BannedMember,
    server_id: String,
    account_id: String,
    client_manager: BatchedSignal<ClientManager>,
    mut bans_resource: Resource<Result<Vec<BannedMember>, String>>,
) -> Element {
    let mut unban_error = use_signal(String::new);

    let reason_display = ban.reason.as_deref().unwrap_or(&t("bans-tab-reason-none")).to_string();
    let user_id = ban.user_id.clone();
    let display_name = ban.display_name.clone();

    rsx! {
        div { class: "ban-row",
            div { class: "ban-row-info",
                span { class: "ban-row-name", "{display_name}" }
                span { class: "ban-row-reason", "{reason_display}" }
            }
            div { class: "ban-row-actions",
                if !unban_error.read().is_empty() {
                    span { class: "ban-row-error", "{unban_error}" }
                }
                button {
                    class: "btn btn-secondary btn-sm",
                    onclick: {
                        let server_id = server_id.clone();
                        let account_id = account_id.clone();
                        let uid = user_id.clone();
                        move |_| {
                            let Some(backend_arc) = client_manager.read().get_backend(&account_id) else {
                                unban_error.set("No backend".to_string());
                                return;
                            };
                            let sid = server_id.clone();
                            let mid = uid.clone();
                            spawn(async move {
                                let guard = backend_arc.read().await;
                                match guard.unban_member(&sid, &mid).await {
                                    Ok(()) => {
                                        unban_error.set(String::new());
                                        bans_resource.restart();
                                    }
                                    Err(e) => {
                                        let msg = t("bans-tab-unban-error").replace("{ $error }", &e.to_string()).replace("{$error}", &e.to_string());
                                        unban_error.set(msg);
                                    }
                                }
                            });
                        }
                    },
                    "{t(\"bans-tab-unban\")}"
                }
            }
        }
    }
}
