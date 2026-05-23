//! `PluginRegistry` — the grouping type for plugin-registration fields.
//!
//! **Single reason to change:** a plugin activates or deactivates (registering /
//! unregistering its settings page or signup entry), a test account is added, or
//! the demo mode toggle fires. This sub-store is completely orthogonal to
//! backend routing ([`BackendRegistry`]) and account identity ([`AccountIdentity`]).
//!
//! This type is provided as a **documentation aid and future-migration target**.
//! `ClientManager` currently retains its flat field layout so existing call
//! sites (`cm.plugin_settings`, `cm.demo_active`, etc.) continue to compile
//! without changes. A future caller-migration pass will move `ClientManager`
//! to embed a `PluginRegistry` value.
//!
//! Part of the SRP split of `ClientManager` — see
//! `docs/plans/plan-solid-audit-core-state.md` Phase B.1.

use poly_client::TestAccountEntry;

use crate::client_manager::{PluginSettingsEntry, SignupEntry};

/// Logical grouping for the plugin-registration fields of `ClientManager`.
///
/// Contains the fields whose **only reason to change** is that a plugin
/// activates, deactivates, or registers/unregisters something:
///
/// | Field | Purpose |
/// |-------|---------|
/// | `plugin_settings` | Settings pages registered by active backends |
/// | `signup_entries` | Signup picker entries registered by plugins |
/// | `test_account_entries` | Quick-add dev-panel entries |
/// | `demo_active` | Whether the demo client is currently active |
#[derive(Default)]
pub struct PluginRegistry {
    /// Settings pages registered by active plugin backends at runtime.
    ///
    /// Populated via `ClientManager::register_plugin_settings` when a backend
    /// activates and cleared via `ClientManager::unregister_plugin_settings`
    /// when it deactivates. The settings nav sidebar and content area iterate
    /// this list to render plugin settings — nothing is hardcoded in the host.
    pub plugin_settings: Vec<PluginSettingsEntry>,
    /// Signup entries registered by compiled-in or WASM plugins at startup.
    ///
    /// The signup picker (`/signup` route) reads this list at runtime to show
    /// the available backends. The host has no compile-time knowledge of any
    /// specific backend — each plugin registers itself via
    /// `ClientManager::register_signup_entry`.
    pub signup_entries: Vec<SignupEntry>,
    /// Test accounts registered by native plugins for the quick-add dev panel.
    pub test_account_entries: Vec<TestAccountEntry>,
    /// Whether the demo client is currently active.
    pub demo_active: bool,
}

impl Clone for PluginRegistry {
    fn clone(&self) -> Self {
        Self {
            plugin_settings: self.plugin_settings.clone(),
            signup_entries: self.signup_entries.clone(),
            test_account_entries: self.test_account_entries.clone(),
            demo_active: self.demo_active,
        }
    }
}
