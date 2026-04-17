use crate::ui::account::{AccountBar, VoiceBar};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
pub fn VoiceAccountFooter() -> Element {
    rsx! {
        div { class: "voice-account-footer",
            VoiceBar {},
            AccountBar {}
        }
    }
}
