//! Bans tab for per-server settings.
//!
//! Stub — Wave 2/3 agents will fill the body once per-backend ban APIs land.
//! Gated by `BackendCapabilities::has_ban`.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Bans tab component — shows the list of banned members and unban controls.
///
/// Empty stub: renders nothing until Wave 2/3 fills the body.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn BansTab(server_id: String, account_id: String) -> Element {
    let _ = (&server_id, &account_id);
    rsx! {}
}
