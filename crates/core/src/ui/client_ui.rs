//! Client-provided UI surface host components.
//!
//! Each component consumes plugin-declared data (from the new WIT interfaces
//! client-menus, client-settings, client-sidebar, client-views, client-composer)
//! and renders it via host Dioxus components. Plugins never touch the DOM —
//! they only declare structured records (see plan §4 for the WIT interfaces).
//!
//! All six components are skeletons at WP 1; filled in WPs 2-6.

pub mod action_outcome;
pub mod composer;
pub mod custom_block;
pub mod menu;
pub mod settings_section;
pub mod sidebar;
pub mod toast;
pub mod view;

pub use action_outcome::{handle_action_outcome, ActionOutcomeCx};
pub use composer::{ClientComposerAction, ClientMessageAction, ComposerHooks, MessageActions};
pub use custom_block::CustomBlock;
pub use menu::ClientMenu;
pub use settings_section::PluginSettingsSection;
pub use sidebar::ClientSidebar;
pub use toast::{push_toast, ToastMessage, ToastOverlay};
pub use view::ClientView;
