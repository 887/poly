use crate::ui::account::{AccountBar, VoiceBar};
use dioxus::prelude::*;

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
