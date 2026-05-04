//! Roles tab for per-server settings.
//!
//! Fetches and displays the server's role list (read-only v1).
//! Gated by `BackendCapabilities::has_roles`.

use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use crate::i18n::t;
use dioxus::prelude::*;
use poly_client::Role;
use poly_ui_macros::{context_menu, ui_action};

/// Roles tab component — shows the role list for a server (read-only, v1).
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn RolesTab(server_id: String, account_id: String) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let roles_resource = {
        let server_id = server_id.clone();
        let account_id = account_id.clone();
        use_resource(move || {
            let server_id = server_id.clone();
            let account_id = account_id.clone();
            let client_manager = client_manager;
            async move {
                client_manager.peek().with_backend(&account_id, async |b| {
                    b.get_server_roles(&server_id).await
                }).await.map_err(|e| e.to_string())
            }
        })
    };

    let roles_snapshot = roles_resource.read_unchecked().as_ref().cloned();

    rsx! {
        div { class: "roles-tab",
            match &roles_snapshot {
                None => rsx! {
                    p { class: "tab-loading", "{t(\"roles-tab-loading\")}" }
                },
                Some(Err(e)) => rsx! {
                    p { class: "tab-error", "{e}" }
                },
                Some(Ok(roles)) if roles.is_empty() => rsx! {
                    p { class: "tab-empty", "{t(\"roles-tab-empty\")}" }
                },
                Some(Ok(roles)) => rsx! {
                    div { class: "roles-list",
                        for role in roles.iter() {
                            {render_role_row(role.clone())}
                        }
                    }
                },
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_role_row(role: Role) -> Element {
    let color_style = role.color
        .as_deref()
        .map(|c| format!("color: {c}"))
        .unwrap_or_default();

    rsx! {
        div { class: "role-row",
            span { class: "role-color-dot", style: "{color_style}", "●" }
            span { class: "role-name", "{role.name}" }
            span { class: "role-position", "#{role.position}" }
        }
    }
}
