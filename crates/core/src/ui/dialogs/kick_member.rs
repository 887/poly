//! Kick member confirmation dialog.
//!
//! Stub — Wave 2/3 agents will fill the body once `kick_member` is wired.
//! Gated by `BackendCapabilities::has_kick`.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Kick member confirmation dialog.
///
/// Empty stub: renders nothing until Wave 2/3 fills the body.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn KickMemberDialog(
    server_id: String,
    member_id: String,
    on_close: EventHandler<()>,
) -> Element {
    let _ = (&server_id, &member_id, &on_close);
    rsx! {}
}
