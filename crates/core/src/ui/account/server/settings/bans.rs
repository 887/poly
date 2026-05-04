//! Bans tab for per-server settings.
//!
//! Lists banned members; provides an unban button for each entry.
//! Gated by `BackendCapabilities::has_ban`.

use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::ui::client_ui::use_view_resource::{use_view_resource, ViewQuery};
use dioxus::prelude::*;
use poly_client::{BannedMember, ClientBackend, ClientError, ClientResult};
use poly_ui_macros::{context_menu, ui_action};

// ── ViewQuery impl ────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct ServerBansQuery {
    account_id: String,
    server_id: String,
}

impl ViewQuery for ServerBansQuery {
    type Output = Vec<BannedMember>;
    fn account_id(&self) -> &str { &self.account_id }
    async fn fetch(&self, b: &dyn ClientBackend) -> ClientResult<Self::Output> {
        b.get_bans(&self.server_id).await
    }
}

/// Bans tab component — shows the list of banned members and unban controls.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn BansTab(server_id: String, account_id: String) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let bans_resource: Resource<ClientResult<Vec<BannedMember>>> = use_view_resource(ServerBansQuery {
        account_id: account_id.clone(),
        server_id: server_id.clone(),
    });

    // `ClientError` is not `Clone`, so we can't snapshot via `.cloned()`.
    // Clone only the `Vec<BannedMember>` on success so we don't hold the guard
    // across the rsx! render.
    let bans_ok: Option<Vec<BannedMember>> = match &*bans_resource.read_unchecked() {
        Some(Ok(v)) => Some(v.clone()),
        _ => None,
    };
    let bans_err: Option<String> = match &*bans_resource.read_unchecked() {
        Some(Err(e)) => Some(e.to_string()),
        _ => None,
    };

    rsx! {
        div { class: "bans-tab",
            if bans_resource.read_unchecked().is_none() {
                p { class: "tab-loading", "{t(\"bans-tab-loading\")}" }
            } else if let Some(e) = bans_err {
                p { class: "tab-error", "{e}" }
            } else if let Some(bans) = bans_ok {
                if bans.is_empty() {
                    p { class: "tab-empty", "{t(\"bans-tab-empty\")}" }
                } else {
                    div { class: "bans-list",
                        for ban in bans.into_iter() {
                            {render_ban_row(ban, server_id.clone(), account_id.clone(), client_manager, bans_resource)}
                        }
                    }
                }
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_ban_row(
    ban: BannedMember,
    server_id: String,
    account_id: String,
    client_manager: BatchedSignal<ClientManager>,
    mut bans_resource: Resource<ClientResult<Vec<BannedMember>>>,
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
                            let sid = server_id.clone();
                            let mid = uid.clone();
                            let aid = account_id.clone();
                            spawn(async move {
                                match client_manager.peek().with_backend(&aid, async |b| {
                                    b.unban_member(&sid, &mid).await
                                }).await {
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
