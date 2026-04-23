//! Mod Log (audit log) tab for per-server settings.
//!
//! Stub — Wave 2/3 agents will fill the body once per-backend modlog APIs land.
//! Gated by `BackendCapabilities::has_moderation_log`.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Mod Log tab component — shows the moderation/audit log for a server.
///
/// Empty stub: renders nothing until Wave 2/3 fills the body.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn ModLogTab(server_id: String, account_id: String) -> Element {
    let _ = (&server_id, &account_id);
    rsx! {}
}
