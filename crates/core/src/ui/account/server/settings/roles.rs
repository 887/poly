//! Roles tab for per-server settings.
//!
//! Fetches and displays the server's role list (read-only v1).
//! Gated by `BackendCapabilities::has_roles`.

use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::ui::client_ui::use_view_resource::{use_view_resource, ViewQuery};
use dioxus::prelude::*;
use poly_client::{ClientBackend, ClientError, ClientResult, Role};
use poly_ui_macros::{context_menu, ui_action};

// ── ViewQuery impl ────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct ServerRolesQuery {
    account_id: String,
    server_id: String,
}

impl ViewQuery for ServerRolesQuery {
    type Output = Vec<Role>;
    fn account_id(&self) -> &str { &self.account_id }
    async fn fetch(&self, b: &dyn ClientBackend) -> ClientResult<Self::Output> {
        b.get_server_roles(&self.server_id).await
    }
}

/// Roles tab component — shows the role list for a server (read-only, v1).
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn RolesTab(server_id: String, account_id: String) -> Element {
    let roles_resource: Resource<ClientResult<Vec<Role>>> = use_view_resource(ServerRolesQuery {
        account_id,
        server_id,
    });

    // `ClientError` is not `Clone`, so we can't snapshot via `.cloned()`.
    // Read through `read_unchecked` and clone only the `Vec<Role>` on success.
    let roles = roles_resource.read_unchecked();

    rsx! {
        div { class: "roles-tab",
            match &*roles {
                None => rsx! {
                    p { class: "tab-loading", "{t(\"roles-tab-loading\")}" }
                },
                Some(Err(e)) => rsx! {
                    p { class: "tab-error", "{e}" }
                },
                Some(Ok(list)) if list.is_empty() => rsx! {
                    p { class: "tab-empty", "{t(\"roles-tab-empty\")}" }
                },
                Some(Ok(list)) => rsx! {
                    div { class: "roles-list",
                        for role in list.iter() {
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
