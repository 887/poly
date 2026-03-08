//! Accounts settings section.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::i18n::t;
use dioxus::prelude::*;

/// Accounts settings section.
///
/// Lists active messenger accounts grouped by backend and provides
/// an "Add Account" entry point.
// TODO(phase-2.7.9.2): Account list grouped by backend
#[rustfmt::skip]
#[component]
pub(super) fn AccountsSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-accounts\")}" }
            p { class: "settings-description", "{t(\"settings-accounts-description\")}" }
            // TODO(phase-2.7.9.2): Account list grouped by backend
            button { class: "btn btn-primary", "{t(\"settings-add-account\")}" }
        }
    }
}
