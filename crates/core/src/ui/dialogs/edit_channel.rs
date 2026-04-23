//! Edit channel dialog.
//!
//! Stub — Wave 2/3 agents will fill the body once `update_channel` is wired.
//! Gated by `BackendCapabilities::has_channel_mgmt`.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Edit channel dialog — name, topic, slow-mode, NSFW toggle.
///
/// Empty stub: renders nothing until Wave 2/3 fills the body.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn EditChannelDialog(
    channel_id: String,
    account_id: String,
    on_close: EventHandler<()>,
) -> Element {
    let _ = (&channel_id, &account_id, &on_close);
    rsx! {}
}
