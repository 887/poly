//! Shared plugin / account administration helpers.
//!
//! This module is the **single source of truth** for the mutations the
//! settings UI ([`crate::ui::settings::plugins`]) and the host-bridge
//! `/host/plugins` / `/host/accounts` MCP routes both need to perform on
//! [`AppSettings`] / [`AccountToken`] storage:
//!
//! - **Sideload a plugin from a URL** ([`add_wasm_plugin`])
//! - **Remove a plugin by URL or slug** ([`remove_wasm_plugin`])
//! - **List all plugins** ([`list_plugins`])
//! - **Toggle a plugin on/off** ([`set_plugin_enabled`])
//! - **Compute the effective backend slug set** ([`available_backend_slugs`])
//! - **Programmatically create an account** ([`add_account_token`])
//!
//! Every helper comes in two flavours:
//!
//! 1. A **pure function** that takes (and returns) the relevant settings
//!    structs — fully unit-testable, no I/O.
//! 2. An async **`*_with_storage`** wrapper that loads from
//!    [`crate::storage::Storage`], applies the pure mutation, and
//!    persists the result.
//!
//! Splitting it this way keeps the pure logic deterministic + cheap to
//! test, while still giving callers a one-liner for the common
//! "load → mutate → persist" round-trip.
//!
//! ## Why not in `bundled_plugins.rs`?
//!
//! `bundled_plugins.rs` is narrowly scoped to *Discord/Teams auto-injection*
//! at startup. Generic add/remove/list/toggle is a separate axis and
//! gets its own module to keep responsibilities single (SOLID/SRP).

use crate::bundled_plugins::{is_bundled_url, slug_from_url};
use crate::client_manager::builtin_backend_slugs;
use crate::storage::{AccountToken, AppSettings, Storage, StorageError, WasmPluginEntry};

// ─── Pure helpers ────────────────────────────────────────────────────────────

/// Outcome of an [`add_wasm_plugin`] mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddPluginOutcome {
    /// New entry inserted into `settings.wasm_plugins`.
    Added,
    /// The URL was already present — `enabled` may have been flipped on,
    /// but no new entry was created. Idempotent re-adds land here.
    AlreadyPresent,
}

/// Pure: append a new WASM plugin entry to `settings.wasm_plugins`.
///
/// Returns:
/// - `Ok(AddPluginOutcome::Added)` if a new entry was inserted.
/// - `Ok(AddPluginOutcome::AlreadyPresent)` if a row with the same URL
///   was already present. The existing row's `enabled` is forced to
///   `true` (re-adding a disabled plugin re-enables it) and any
///   tombstone in `removed_bundled_plugins` for that slug is cleared.
///   Other fields (name, bundled) are left intact.
/// - `Err(AddPluginError::EmptyUrl)` for an empty / whitespace URL.
/// - `Err(AddPluginError::InvalidUrl)` for a malformed URL.
///
/// Adding a `bundled://<slug>` URL clears the matching entry from
/// `removed_bundled_plugins` so the next `ensure_bundled_plugins` run
/// keeps the entry in place.
pub fn add_wasm_plugin(
    settings: &mut AppSettings,
    url: &str,
    name: Option<String>,
) -> Result<AddPluginOutcome, AddPluginError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AddPluginError::EmptyUrl);
    }
    if !is_acceptable_plugin_url(url) {
        return Err(AddPluginError::InvalidUrl(url.to_string()));
    }

    // Tombstone clearance — adding a bundled plugin back must lift the
    // user's prior "removed" intent so subsequent restarts don't drop it.
    if let Some(slug) = slug_from_url(url) {
        settings.removed_bundled_plugins.retain(|s| s != slug);
    }

    if let Some(existing) = settings.wasm_plugins.iter_mut().find(|e| e.url == url) {
        existing.enabled = true;
        return Ok(AddPluginOutcome::AlreadyPresent);
    }

    settings.wasm_plugins.push(WasmPluginEntry {
        url: url.to_string(),
        name,
        enabled: true,
        bundled: is_bundled_url(url),
    });
    Ok(AddPluginOutcome::Added)
}

/// Pure: remove a WASM plugin by URL.
///
/// Returns `Ok(true)` if a row was removed, `Ok(false)` if no row
/// matched. On a successful removal of a bundled plugin
/// (`bundled://<slug>`), the slug is added to
/// `settings.removed_bundled_plugins` so [`crate::bundled_plugins::ensure_bundled_plugins`]
/// respects the user's intent on the next launch.
pub fn remove_wasm_plugin(settings: &mut AppSettings, url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() {
        return false;
    }
    let mut removed_slug: Option<String> = None;
    let before = settings.wasm_plugins.len();
    settings.wasm_plugins.retain(|e| {
        if e.url == url {
            if e.bundled {
                if let Some(slug) = slug_from_url(&e.url) {
                    removed_slug = Some(slug.to_string());
                }
            }
            false
        } else {
            true
        }
    });
    let removed = before != settings.wasm_plugins.len();
    if let Some(slug) = removed_slug {
        if !settings.removed_bundled_plugins.iter().any(|s| s == &slug) {
            settings.removed_bundled_plugins.push(slug);
        }
    }
    removed
}

/// Pure: toggle the `enabled` field on the entry matching `url`.
///
/// Returns the new `enabled` value (`Some(bool)`) or `None` if no row
/// matched.
pub fn set_plugin_enabled(
    settings: &mut AppSettings,
    url: &str,
    enabled: bool,
) -> Option<bool> {
    let url = url.trim();
    settings
        .wasm_plugins
        .iter_mut()
        .find(|e| e.url == url)
        .map(|e| {
            e.enabled = enabled;
            e.enabled
        })
}

/// Pure: snapshot of every plugin known to the app, classified by source.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginListing {
    /// Compile-time built-in backends (demo, stoat, matrix, …) with
    /// their effective enabled state.
    pub builtin: Vec<BuiltinPluginInfo>,
    /// Sideloaded WASM plugin entries (user-added + bundled).
    pub sideloaded: Vec<WasmPluginEntry>,
}

/// Single built-in backend's user-visible state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BuiltinPluginInfo {
    /// Slug (`"demo"`, `"stoat"`, …).
    pub slug: String,
    /// `true` if the backend was compiled into this build.
    pub available: bool,
    /// `true` if currently enabled (i.e. not in `disabled_native_backends`).
    pub enabled: bool,
}

/// Pure: classify every known plugin from `settings`.
#[must_use]
pub fn list_plugins(settings: &AppSettings) -> PluginListing {
    let disabled = &settings.disabled_native_backends;
    let builtin = builtin_backend_slugs()
        .iter()
        .map(|slug| BuiltinPluginInfo {
            slug: (*slug).to_string(),
            available: backend_is_available(slug),
            enabled: !disabled.iter().any(|s| s == slug),
        })
        .collect();
    PluginListing {
        builtin,
        sideloaded: settings.wasm_plugins.clone(),
    }
}

/// Pure: slugs of every bundled plugin currently enabled in
/// `settings.wasm_plugins`. Used by code paths that need to know which
/// runtime-toggleable backends are usable (e.g. signup picker).
#[must_use]
pub fn bundled_enabled_slugs(settings: &AppSettings) -> Vec<String> {
    settings
        .wasm_plugins
        .iter()
        .filter(|e| e.bundled && e.enabled)
        .filter_map(|e| slug_from_url(&e.url).map(str::to_string))
        .collect()
}

/// Pure: union of compiled-in builtin backends and bundled-enabled
/// runtime backends, minus any that the user has disabled.
///
/// This is the canonical "what backends can the user actually create
/// an account on" set. Used by the signup picker, MCP `list_plugins`,
/// and the test that asserts toggling Discord off blocks signup.
#[must_use]
pub fn available_backend_slugs(settings: &AppSettings) -> Vec<String> {
    let disabled = &settings.disabled_native_backends;
    let mut out: Vec<String> = builtin_backend_slugs()
        .iter()
        .filter(|slug| backend_is_available(slug))
        .filter(|slug| !disabled.iter().any(|d| d == *slug))
        .map(|s| (*s).to_string())
        .collect();
    for slug in bundled_enabled_slugs(settings) {
        if !out.iter().any(|s| s == &slug) && !disabled.iter().any(|d| d == &slug) {
            out.push(slug);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Errors returned by [`add_wasm_plugin`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AddPluginError {
    /// The URL string was empty after trimming.
    #[error("plugin URL is empty")]
    EmptyUrl,
    /// The URL didn't match an accepted scheme
    /// (`http`, `https`, `file`, `bundled`).
    #[error("invalid plugin URL: {0}")]
    InvalidUrl(String),
}

/// Errors returned by [`add_account_token_with_storage`] and
/// [`remove_account_token_with_storage`].
#[derive(Debug, thiserror::Error)]
pub enum AccountAdminError {
    /// The requested backend slug isn't available — either compiled out
    /// or the user has it disabled.
    #[error("backend `{slug}` is not available (not compiled in or disabled)")]
    BackendUnavailable { slug: String },
    /// Persisted storage layer failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// `account_id` was empty.
    #[error("account_id is required")]
    EmptyAccountId,
}

fn is_acceptable_plugin_url(url: &str) -> bool {
    [
        "https://",
        "http://",
        "file://",
        "bundled://",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
}

/// Whether a built-in backend slug was compiled into this build.
///
/// Mirrors the `cfg!(feature = "...")` checks in
/// [`crate::client_manager::BUILTIN_BACKENDS`] so external callers
/// (tests, MCP) don't need to walk the descriptor array themselves.
fn backend_is_available(slug: &str) -> bool {
    for desc in crate::client_manager::builtin_backend_descriptors() {
        if desc.slug == slug {
            return desc.available;
        }
    }
    false
}

// ─── Async storage-bound wrappers ────────────────────────────────────────────

/// Load `AppSettings`, append a plugin, persist, return outcome + the
/// row that was inserted (or already present).
pub async fn add_wasm_plugin_with_storage(
    storage: &Storage,
    url: &str,
    name: Option<String>,
) -> Result<(AddPluginOutcome, WasmPluginEntry), PluginAdminError> {
    let mut settings = storage.get_app_settings().await?;
    let outcome = add_wasm_plugin(&mut settings, url, name)?;
    storage.set_app_settings(&settings).await?;
    let url_trim = url.trim().to_string();
    let entry = settings
        .wasm_plugins
        .iter()
        .find(|e| e.url == url_trim)
        .cloned()
        .ok_or_else(|| PluginAdminError::NotFound(format!(
            "just-added plugin entry not found in settings: {url_trim} (race or storage write failed)"
        )))?;
    Ok((outcome, entry))
}

/// Load `AppSettings`, remove the plugin, persist. Returns whether a
/// row was removed.
pub async fn remove_wasm_plugin_with_storage(
    storage: &Storage,
    url_or_slug: &str,
) -> Result<bool, PluginAdminError> {
    let mut settings = storage.get_app_settings().await?;
    // Allow callers to pass either the full URL or the bare slug for
    // bundled plugins. We try the URL match first, then fall back to
    // mapping a bare slug to its `bundled://<slug>` URL.
    let direct = remove_wasm_plugin(&mut settings, url_or_slug);
    let removed = if direct {
        true
    } else if !url_or_slug.contains("://") {
        let candidate = format!("bundled://{url_or_slug}");
        remove_wasm_plugin(&mut settings, &candidate)
    } else {
        false
    };
    if removed {
        storage.set_app_settings(&settings).await?;
    }
    Ok(removed)
}

/// Load `AppSettings`, toggle, persist.
pub async fn set_plugin_enabled_with_storage(
    storage: &Storage,
    url: &str,
    enabled: bool,
) -> Result<bool, PluginAdminError> {
    let mut settings = storage.get_app_settings().await?;
    let new_state = set_plugin_enabled(&mut settings, url, enabled)
        .ok_or_else(|| PluginAdminError::NotFound(url.to_string()))?;
    storage.set_app_settings(&settings).await?;
    Ok(new_state)
}

/// Load `AppSettings`, return the classified listing.
pub async fn list_plugins_with_storage(
    storage: &Storage,
) -> Result<PluginListing, PluginAdminError> {
    let settings = storage.get_app_settings().await?;
    Ok(list_plugins(&settings))
}

/// Persist a new account token after validating that the backend is
/// available. Used by the MCP `create_account` tool / `/host/accounts`
/// route to programmatically register a fully-formed credential
/// (typically the agent has already obtained one via OAuth or by
/// calling the backend's signup API directly).
///
/// Returns the persisted token.
pub async fn add_account_token_with_storage(
    storage: &Storage,
    token: AccountToken,
) -> Result<AccountToken, AccountAdminError> {
    if token.account_id.trim().is_empty() {
        return Err(AccountAdminError::EmptyAccountId);
    }
    let settings = storage.get_app_settings().await?;
    let allowed = available_backend_slugs(&settings);
    if !allowed.iter().any(|s| s == &token.backend) {
        return Err(AccountAdminError::BackendUnavailable {
            slug: token.backend.clone(),
        });
    }
    storage.upsert_account_token(&token).await?;
    Ok(token)
}

/// Remove a stored account token.
///
/// Returns `true` if a row was removed.
pub async fn remove_account_token_with_storage(
    storage: &Storage,
    backend: &str,
    account_id: &str,
) -> Result<bool, AccountAdminError> {
    if account_id.trim().is_empty() {
        return Err(AccountAdminError::EmptyAccountId);
    }
    let tokens_before = storage.get_account_tokens().await?;
    let exists = tokens_before
        .iter()
        .any(|t| t.backend == backend && t.account_id == account_id);
    if !exists {
        return Ok(false);
    }
    storage.remove_account_token(backend, account_id).await?;
    Ok(true)
}

/// Combined error for the `_with_storage` helpers.
#[derive(Debug, thiserror::Error)]
pub enum PluginAdminError {
    /// Validation failure — see [`AddPluginError`] for variants.
    #[error(transparent)]
    Add(#[from] AddPluginError),
    /// Storage backend error.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// `set_plugin_enabled_with_storage` couldn't find a row matching `url`.
    #[error("plugin not found: {0}")]
    NotFound(String),
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
    use super::*;
    use crate::storage::AppSettings;

    fn http_url() -> &'static str {
        "https://example.com/p.wasm"
    }

    // ─── add_wasm_plugin pure ─────────────────────────────────────────────

    #[test]
    fn add_pure_inserts_new_entry() {
        let mut s = AppSettings::default();
        let outcome = add_wasm_plugin(&mut s, http_url(), Some("Test".into())).unwrap();
        assert_eq!(outcome, AddPluginOutcome::Added);
        assert_eq!(s.wasm_plugins.len(), 1);
        let e = &s.wasm_plugins[0];
        assert_eq!(e.url, http_url());
        assert_eq!(e.name.as_deref(), Some("Test"));
        assert!(e.enabled);
        assert!(!e.bundled);
    }

    #[test]
    fn add_pure_is_idempotent_on_duplicate_url() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, http_url(), None).unwrap();
        let outcome = add_wasm_plugin(&mut s, http_url(), None).unwrap();
        assert_eq!(outcome, AddPluginOutcome::AlreadyPresent);
        assert_eq!(s.wasm_plugins.len(), 1);
    }

    #[test]
    fn add_pure_re_enables_disabled_existing_entry() {
        let mut s = AppSettings::default();
        s.wasm_plugins.push(WasmPluginEntry {
            url: http_url().into(),
            name: None,
            enabled: false,
            bundled: false,
        });
        let outcome = add_wasm_plugin(&mut s, http_url(), None).unwrap();
        assert_eq!(outcome, AddPluginOutcome::AlreadyPresent);
        assert!(s.wasm_plugins[0].enabled);
    }

    #[test]
    fn add_pure_rejects_empty_url() {
        let mut s = AppSettings::default();
        assert_eq!(
            add_wasm_plugin(&mut s, "   ", None).unwrap_err(),
            AddPluginError::EmptyUrl
        );
        assert!(s.wasm_plugins.is_empty());
    }

    #[test]
    fn add_pure_rejects_malformed_url() {
        let mut s = AppSettings::default();
        let err = add_wasm_plugin(&mut s, "not-a-url", None).unwrap_err();
        assert!(matches!(err, AddPluginError::InvalidUrl(_)));
        assert!(s.wasm_plugins.is_empty());
    }

    #[test]
    fn add_pure_accepts_https_http_file_bundled() {
        for url in [
            "https://x.com/a.wasm",
            "http://localhost:8080/p.wasm",
            "file:///tmp/p.wasm",
            "bundled://discord",
        ] {
            let mut s = AppSettings::default();
            add_wasm_plugin(&mut s, url, None).unwrap();
            assert_eq!(s.wasm_plugins.len(), 1, "should accept {url}");
        }
    }

    #[test]
    fn add_pure_bundled_url_sets_bundled_true() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        assert!(s.wasm_plugins[0].bundled);
    }

    #[test]
    fn add_pure_bundled_clears_tombstone() {
        let mut s = AppSettings::default();
        s.removed_bundled_plugins.push("discord".into());
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        assert!(s.removed_bundled_plugins.is_empty());
    }

    // ─── remove_wasm_plugin pure ──────────────────────────────────────────

    #[test]
    fn remove_pure_drops_matching_row() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, http_url(), None).unwrap();
        assert!(remove_wasm_plugin(&mut s, http_url()));
        assert!(s.wasm_plugins.is_empty());
    }

    #[test]
    fn remove_pure_returns_false_for_unknown_url() {
        let mut s = AppSettings::default();
        assert!(!remove_wasm_plugin(&mut s, http_url()));
    }

    #[test]
    fn remove_pure_bundled_records_tombstone() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        assert!(remove_wasm_plugin(&mut s, "bundled://discord"));
        assert_eq!(s.removed_bundled_plugins, vec!["discord".to_string()]);
    }

    #[test]
    fn remove_pure_user_added_does_not_record_tombstone() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, http_url(), None).unwrap();
        assert!(remove_wasm_plugin(&mut s, http_url()));
        assert!(s.removed_bundled_plugins.is_empty());
    }

    #[test]
    fn remove_pure_does_not_double_record_tombstone() {
        let mut s = AppSettings::default();
        s.removed_bundled_plugins.push("discord".into());
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        // Tombstone was cleared on add. Now remove again — must reappear once only.
        assert!(remove_wasm_plugin(&mut s, "bundled://discord"));
        assert_eq!(s.removed_bundled_plugins, vec!["discord".to_string()]);
        // And again — not duplicated.
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        assert!(remove_wasm_plugin(&mut s, "bundled://discord"));
        assert_eq!(s.removed_bundled_plugins, vec!["discord".to_string()]);
    }

    // ─── set_plugin_enabled pure ──────────────────────────────────────────

    #[test]
    fn toggle_pure_flips_enabled() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, http_url(), None).unwrap();
        assert_eq!(set_plugin_enabled(&mut s, http_url(), false), Some(false));
        assert!(!s.wasm_plugins[0].enabled);
        assert_eq!(set_plugin_enabled(&mut s, http_url(), true), Some(true));
        assert!(s.wasm_plugins[0].enabled);
    }

    #[test]
    fn toggle_pure_returns_none_for_unknown() {
        let mut s = AppSettings::default();
        assert_eq!(set_plugin_enabled(&mut s, http_url(), false), None);
    }

    // ─── list_plugins / bundled_enabled_slugs / available_backend_slugs ──

    #[test]
    fn list_plugins_includes_all_builtin_descriptors() {
        let s = AppSettings::default();
        let listing = list_plugins(&s);
        let listed: Vec<&str> = listing.builtin.iter().map(|b| b.slug.as_str()).collect();
        for slug in builtin_backend_slugs() {
            assert!(listed.contains(&slug), "missing {slug}");
        }
    }

    #[test]
    fn list_plugins_marks_disabled_native_backends() {
        let mut s = AppSettings::default();
        s.disabled_native_backends.push("stoat".into());
        let listing = list_plugins(&s);
        let stoat = listing
            .builtin
            .iter()
            .find(|b| b.slug == "stoat")
            .expect("stoat present");
        assert!(!stoat.enabled);
    }

    #[test]
    fn list_plugins_returns_sideloaded() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        add_wasm_plugin(&mut s, http_url(), None).unwrap();
        let listing = list_plugins(&s);
        assert_eq!(listing.sideloaded.len(), 2);
    }

    #[test]
    fn bundled_enabled_empty_when_no_bundled() {
        let s = AppSettings::default();
        assert!(bundled_enabled_slugs(&s).is_empty());
    }

    #[test]
    fn bundled_enabled_lists_enabled_only() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        add_wasm_plugin(&mut s, "bundled://teams", None).unwrap();
        // Disable teams.
        set_plugin_enabled(&mut s, "bundled://teams", false);
        let slugs = bundled_enabled_slugs(&s);
        assert_eq!(slugs, vec!["discord".to_string()]);
    }

    #[test]
    fn bundled_enabled_excludes_user_plugins() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, http_url(), None).unwrap();
        assert!(bundled_enabled_slugs(&s).is_empty());
    }

    #[test]
    fn available_backend_includes_compiled_builtins() {
        let s = AppSettings::default();
        let av = available_backend_slugs(&s);
        // demo is feature-gated but at least "demo" descriptor is always
        // present; cfg!(feature = "demo") drives availability. We can't
        // assert strict membership without knowing the build features,
        // so assert: every slug in `av` corresponds to an available
        // builtin OR an enabled bundled one.
        let valid: Vec<String> = builtin_backend_slugs()
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        for slug in &av {
            assert!(valid.contains(slug), "{slug} in availability set unexpected");
        }
    }

    #[test]
    fn available_backend_includes_bundled_when_enabled() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        let av = available_backend_slugs(&s);
        assert!(av.contains(&"discord".to_string()));
    }

    #[test]
    fn available_backend_excludes_bundled_when_disabled() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        set_plugin_enabled(&mut s, "bundled://discord", false);
        let av = available_backend_slugs(&s);
        assert!(!av.contains(&"discord".to_string()));
    }

    #[test]
    fn available_backend_excludes_user_disabled_native() {
        let mut s = AppSettings::default();
        s.disabled_native_backends.push("demo".into());
        let av = available_backend_slugs(&s);
        assert!(!av.contains(&"demo".to_string()));
    }

    #[test]
    fn available_backend_is_sorted_unique() {
        let mut s = AppSettings::default();
        add_wasm_plugin(&mut s, "bundled://discord", None).unwrap();
        add_wasm_plugin(&mut s, "bundled://teams", None).unwrap();
        let av = available_backend_slugs(&s);
        let mut sorted = av.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(av, sorted);
    }

    // ─── URL acceptance ────────────────────────────────────────────────────

    #[test]
    fn is_acceptable_known_schemes() {
        assert!(is_acceptable_plugin_url("https://x"));
        assert!(is_acceptable_plugin_url("http://x"));
        assert!(is_acceptable_plugin_url("file:///x"));
        assert!(is_acceptable_plugin_url("bundled://discord"));
        assert!(!is_acceptable_plugin_url("ftp://x"));
        assert!(!is_acceptable_plugin_url("plain"));
        assert!(!is_acceptable_plugin_url(""));
    }

    // ─── Storage-bound integration tests (native + sqlite) ──────────────

    #[cfg(all(not(target_arch = "wasm32"), not(feature = "storage-surreal")))]
    mod storage_tests {
        use super::super::*;
        use crate::storage::{AccountToken, Storage};

        async fn fresh_storage() -> Storage {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.keep();
            Storage::init_with_path(path).await.expect("storage init")
        }

        #[tokio::test]
        async fn add_persists_and_round_trips() {
            let storage = fresh_storage().await;
            let (outcome, entry) =
                add_wasm_plugin_with_storage(&storage, "https://example.com/p.wasm", None)
                    .await
                    .unwrap();
            assert_eq!(outcome, AddPluginOutcome::Added);
            assert_eq!(entry.url, "https://example.com/p.wasm");

            let s = storage.get_app_settings().await.unwrap();
            assert_eq!(s.wasm_plugins.len(), 1);
            assert!(s.wasm_plugins[0].enabled);
        }

        #[tokio::test]
        async fn add_validation_propagates() {
            let storage = fresh_storage().await;
            let err = add_wasm_plugin_with_storage(&storage, "ftp://x", None)
                .await
                .unwrap_err();
            assert!(
                matches!(err, PluginAdminError::Add(AddPluginError::InvalidUrl(_))),
                "{err:?}"
            );
        }

        #[tokio::test]
        async fn add_then_remove_cycle() {
            let storage = fresh_storage().await;
            let url = "https://example.com/p.wasm";
            add_wasm_plugin_with_storage(&storage, url, None).await.unwrap();
            let removed = remove_wasm_plugin_with_storage(&storage, url).await.unwrap();
            assert!(removed);
            let s = storage.get_app_settings().await.unwrap();
            assert!(s.wasm_plugins.is_empty());
        }

        #[tokio::test]
        async fn remove_by_bare_slug_for_bundled() {
            let storage = fresh_storage().await;
            add_wasm_plugin_with_storage(&storage, "bundled://discord", None)
                .await
                .unwrap();
            let removed = remove_wasm_plugin_with_storage(&storage, "discord")
                .await
                .unwrap();
            assert!(removed);
            let s = storage.get_app_settings().await.unwrap();
            assert!(
                s.removed_bundled_plugins.iter().any(|x| x == "discord"),
                "tombstone must persist"
            );
        }

        #[tokio::test]
        async fn toggle_persists_across_loads() {
            let storage = fresh_storage().await;
            let url = "https://example.com/p.wasm";
            add_wasm_plugin_with_storage(&storage, url, None).await.unwrap();
            let new_state = set_plugin_enabled_with_storage(&storage, url, false)
                .await
                .unwrap();
            assert!(!new_state);
            let s = storage.get_app_settings().await.unwrap();
            assert!(!s.wasm_plugins[0].enabled);
        }

        #[tokio::test]
        async fn toggle_unknown_url_returns_not_found() {
            let storage = fresh_storage().await;
            let err = set_plugin_enabled_with_storage(&storage, "https://nope.test/x", true)
                .await
                .unwrap_err();
            assert!(matches!(err, PluginAdminError::NotFound(_)));
        }

        #[tokio::test]
        async fn account_add_validates_backend_availability() {
            let storage = fresh_storage().await;
            let bogus = AccountToken {
                backend: "no-such-backend".into(),
                account_id: "alice".into(),
                token: "t".into(),
                display_name: "A".into(),
                instance_id: None,
                refresh_token: None,
                token_expires_at: None,
                scope: None,
            };
            let err = add_account_token_with_storage(&storage, bogus)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                AccountAdminError::BackendUnavailable { .. }
            ));
        }

        #[cfg(feature = "demo")]
        #[tokio::test]
        async fn account_add_round_trip_for_available_backend() {
            let storage = fresh_storage().await;
            let token = AccountToken {
                backend: "demo".into(),
                account_id: "alice".into(),
                token: "tok".into(),
                display_name: "Alice".into(),
                instance_id: None,
                refresh_token: None,
                token_expires_at: None,
                scope: None,
            };
            add_account_token_with_storage(&storage, token).await.unwrap();
            let stored = storage.get_account_tokens().await.unwrap();
            assert_eq!(stored.len(), 1);
            assert_eq!(stored[0].account_id, "alice");

            let removed =
                remove_account_token_with_storage(&storage, "demo", "alice")
                    .await
                    .unwrap();
            assert!(removed);
            assert!(storage.get_account_tokens().await.unwrap().is_empty());
        }

        #[cfg(feature = "demo")]
        #[tokio::test]
        async fn full_flow_add_plugin_then_login_then_list() {
            let storage = fresh_storage().await;
            // Bundled plugin first.
            add_wasm_plugin_with_storage(&storage, "bundled://discord", None)
                .await
                .unwrap();
            let listing = list_plugins_with_storage(&storage).await.unwrap();
            assert!(
                listing
                    .sideloaded
                    .iter()
                    .any(|e| e.url == "bundled://discord")
            );

            // Login with the demo backend (the test that bundled discord
            // creates a usable account would need actual plugin loading,
            // which the daemon doesn't do — see plan-bundled-plugins.md).
            let token = AccountToken {
                backend: "demo".into(),
                account_id: "agent-test".into(),
                token: "t".into(),
                display_name: "Agent Test".into(),
                instance_id: None,
                refresh_token: None,
                token_expires_at: None,
                scope: None,
            };
            add_account_token_with_storage(&storage, token).await.unwrap();

            let stored = storage.get_account_tokens().await.unwrap();
            assert_eq!(stored.len(), 1);
            assert_eq!(stored[0].backend, "demo");
        }
    }
}
