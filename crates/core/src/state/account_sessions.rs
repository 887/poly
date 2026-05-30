//! `AccountSessions` — reactive store for per-account identity & preferences.
//!
//! Holds session tokens, server ordering preferences, content policy,
//! and blocked-user lists. These are account-scoped and relatively
//! infrequently written compared to messages or channel lists.
//!
//! Provided as `BatchedSignal<AccountSessions>` at the `App` level
//! (Phase G.6 of plan-solid-refactor-survey.md).

use poly_client::{BlockedUser, ContentPolicy, Session};
use std::collections::HashMap;

/// Reactive store for per-account identity and preferences.
///
/// Components that only read account session info subscribe to this
/// signal and are not re-rendered when messages or channel lists change.
#[derive(Debug, Clone, Default)]
pub struct AccountSessions {
    /// Whether the app has completed first-launch setup (keys generated,
    /// first account created). Gates the SetupWizard vs Router mount.
    ///
    /// Migrated here from the deleted `AppState` struct (Phase C.3 of
    /// plan-solid-audit-core-state.md) — setup completion is fundamentally
    /// an identity-layer flag (no account exists ⇒ no setup completed).
    pub is_setup_complete: bool,
    /// Sessions keyed by account ID — one entry per active account.
    ///
    /// Used to look up `icon_emoji`, display name, and other per-account
    /// identity data in sidebar components without traversing all servers.
    pub account_sessions: HashMap<String, Session>,
    /// Server IDs that are pinned to the Favorites Bar (Bar 1).
    ///
    /// Drag a server from Bar 2 to Bar 1 to add it here. Empty means
    /// no servers are pinned (Bar 1 shows nothing in the server area).
    pub favorited_server_ids: Vec<String>,
    /// Custom server ordering per account (account_id → Vec<server_id>).
    ///
    /// Populated on first drag within the Account Server Bar. Servers not
    /// listed here appear after the explicitly ordered ones.
    pub account_server_order: HashMap<String, Vec<String>>,
    /// User-preferred order of account icons in the Favorites Bar (Bar 1).
    ///
    /// Hydrated from `AppSettings.account_order` at startup. Accounts not
    /// listed here are appended alphabetically at render time so the icon
    /// layout is stable across `HashMap`-based `ClientManager.backends`
    /// iteration order.
    pub account_order: Vec<String>,
    /// Content and social policy per account (account_id → policy).
    ///
    /// Mirrors the shape of `account_sessions` and `blocked_users` — one
    /// entry per account, keyed by account ID.
    ///
    /// Loaded from `get_content_policy()` on account switch.
    /// Falls back to `ContentPolicy::default()` if the backend returns
    /// `NotSupported` or the account has no entry yet. Written to when the
    /// user changes settings in the Content & Social settings page.
    pub content_policies: HashMap<String, ContentPolicy>,
    /// Users blocked per account (account_id → blocked list).
    pub blocked_users: HashMap<String, Vec<BlockedUser>>,
}

impl AccountSessions {
    /// Returns the content policy for the given account, or a default if the
    /// account has no policy stored yet.
    #[must_use] 
    pub fn get_content_policy(&self, account_id: &str) -> ContentPolicy {
        self.content_policies
            .get(account_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Stores the content policy for the given account.
    pub fn set_content_policy(&mut self, account_id: impl Into<String>, policy: ContentPolicy) {
        self.content_policies.insert(account_id.into(), policy);
    }
}
