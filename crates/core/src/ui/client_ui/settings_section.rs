//! Host component that renders plugin-declared settings sections.
//! WP 3 fills this in.

use dioxus::prelude::*;
use poly_client::SettingsSection;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn PluginSettingsSection(section: SettingsSection) -> Element {
    let _ = section;
    rsx! {
        // WP 3: render scoped settings fields via existing schema widgets
    }
}
