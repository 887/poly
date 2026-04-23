//! Timeout member dialog.
//!
//! Stub — Wave 2/3 agents will fill the body once `timeout_member` is wired.
//! Gated by `BackendCapabilities::has_timed_ban`.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Timeout member dialog — duration picker and reason field.
///
/// Empty stub: renders nothing until Wave 2/3 fills the body.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn TimeoutMemberDialog(
    server_id: String,
    member_id: String,
    on_close: EventHandler<()>,
) -> Element {
    let _ = (&server_id, &member_id, &on_close);
    rsx! {}
}
