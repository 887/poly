use crate::ui::account::{AccountBar, VoiceBar};
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn VoiceAccountFooter() -> Element {
    rsx! {
        div { class: "voice-account-footer",
            VoiceBar {},
            AccountBar {}
        }
    }
}
