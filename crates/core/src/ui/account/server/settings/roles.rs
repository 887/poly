//! Roles tab for per-server settings.
//!
//! Stub — Wave 2/3 agents will fill the body once per-backend role APIs land.
//! Gated by `BackendCapabilities::has_roles`.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Roles tab component — shows the role list and role editor for a server.
///
/// Empty stub: renders nothing until Wave 2/3 fills the body.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn RolesTab(server_id: String, account_id: String) -> Element {
    let _ = (&server_id, &account_id);
    rsx! {}
}
