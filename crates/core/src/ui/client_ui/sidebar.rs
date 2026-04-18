//! Host component that renders plugin-declared sidebar layouts.
//! Dispatches across 5 stock layouts + CustomSidebar. WP 4 fills this in.

use dioxus::prelude::*;
use poly_client::SidebarDeclaration;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ClientSidebar(declaration: SidebarDeclaration) -> Element {
    let _ = declaration;
    rsx! {
        // WP 4: dispatch on declaration.layout to ChannelListLayout,
        // SpacesRoomsLayout, CommunitiesLayout, FeedLayout, RepoTreeLayout,
        // or CustomSidebar.
    }
}
