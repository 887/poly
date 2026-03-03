//! Server-scoped UI components for Poly.
//!
//! Components here are scoped to a specific server within an account.
//! They include right-click context menus and per-server settings pages.
//!
//! ## Sub-modules
//! | Module | Contents |
//! |---|---|
//! | `settings` | Per-server settings page (notifications, profile, general/leave) |
//! | `context_menu` | Right-click context menu for server icons in both sidebars |

pub mod context_menu;
pub mod settings;

pub use context_menu::ServerContextMenu;
pub use settings::ServerSettingsPage;
