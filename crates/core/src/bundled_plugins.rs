//! Bundled WASM plugin auto-injection.
//!
//! Some plugins ship as **sideloaded** WASM blobs but the user shouldn't
//! have to discover and add them by hand — they should appear in the
//! Sideloaded WASM Plugins section automatically the first time the app
//! launches. Discord and Microsoft Teams are the canonical examples
//! (excluded from the built-in registry for app-store / TOS reasons but
//! shipped with the app for convenience).
//!
//! At startup, [`ensure_bundled_plugins`] walks [`BUNDLED_PLUGINS`] and
//! appends any missing entry into [`AppSettings::wasm_plugins`] using a
//! stable `bundled://<slug>` URL. The function is idempotent:
//!
//! - If an entry with the same `bundled://<slug>` URL already exists,
//!   nothing changes (the user's `enabled` toggle is preserved).
//! - If the slug appears in [`AppSettings::removed_bundled_plugins`],
//!   the user explicitly removed it — respect their intent and skip
//!   re-injection.
//!
//! ## URL scheme
//!
//! Bundled plugins use `bundled://<slug>` URLs, e.g. `bundled://discord`.
//! The plugin loader recognises the `bundled://` scheme and reads bytes
//! from the in-binary asset rather than fetching over HTTP. (TODO: when
//! the actual `.wasm` artifacts ship, the loader branch goes in
//! `crates/plugin-host` — for now the URL is just an identifier so the
//! UX is in place.)
//!
//! ## Why not the built-in registry?
//!
//! [`crate::client_manager::builtin_backend_descriptors`] is locked
//! against Discord and Teams (see test
//! `discord_and_teams_are_never_builtin`). They must remain absent from
//! the built-in list for app-store policy reasons. Bundled WASM plugins
//! are a separate axis: they're WASM blobs shipped alongside the binary
//! that surface in the Sideloaded section, with the same toggle/remove
//! affordances as user-added plugins.

use crate::client_manager::{ClientManager, SignupEntry};
use crate::storage::{AppSettings, WasmPluginEntry};

/// Compile-time descriptor for a single bundled WASM plugin.
#[derive(Clone, Copy, Debug)]
pub struct BundledPlugin {
    /// Stable slug used in the `bundled://<slug>` URL and as the key in
    /// [`AppSettings::removed_bundled_plugins`]. Lowercase ASCII.
    pub slug: &'static str,
    /// Human-readable display name shown in the Sideloaded plugin row.
    pub display_name: &'static str,
}

impl BundledPlugin {
    /// The stable URL used as the entry's identifier in
    /// [`AppSettings::wasm_plugins`]. e.g. `bundled://discord`.
    #[must_use]
    pub fn url(&self) -> String {
        format!("bundled://{}", self.slug)
    }
}

/// All plugins auto-injected at app startup.
///
/// Discord and Teams are bundled (not built-in) so they show up in the
/// Sideloaded section with the same UX as user-added WASM plugins.
pub const BUNDLED_PLUGINS: &[BundledPlugin] = &[
    BundledPlugin {
        slug: "discord",
        display_name: "Discord",
    },
    BundledPlugin {
        slug: "teams",
        display_name: "Microsoft Teams",
    },
];

/// Returns `true` if the given URL targets a bundled plugin.
#[must_use]
pub fn is_bundled_url(url: &str) -> bool {
    url.starts_with("bundled://")
}

/// Slugs of every bundled plugin whose `WasmPluginEntry` is currently
/// `enabled = true` in `settings.wasm_plugins`.
///
/// This is the runtime view of "which bundled backends should the user
/// be able to add accounts on right now?". It walks `settings.wasm_plugins`
/// (the persisted, user-toggleable list) and returns the slug of every
/// entry with `bundled = true && enabled = true`.
///
/// Used by the signup picker (so toggling a bundled plugin OFF in
/// `/settings/plugins` immediately hides it from the "+" account picker)
/// and by capability lookup (already slug-keyed in `clients/client`).
#[must_use]
pub fn bundled_enabled_slugs(settings: &AppSettings) -> Vec<&'static str> {
    let mut out = Vec::new();
    for plugin in BUNDLED_PLUGINS {
        let url = format!("bundled://{}", plugin.slug);
        if settings
            .wasm_plugins
            .iter()
            .any(|e| e.url == url && e.enabled)
        {
            out.push(plugin.slug);
        }
    }
    out
}

/// Look up the [`BundledPlugin`] descriptor by slug.
///
/// Returns `None` if `slug` is not the slug of a bundled plugin.
#[must_use]
pub fn bundled_plugin_by_slug(slug: &str) -> Option<&'static BundledPlugin> {
    BUNDLED_PLUGINS.iter().find(|p| p.slug == slug)
}

/// Convert an existing `WasmPluginEntry` into a `BundledPlugin` slug,
/// if its URL matches the bundled scheme.
#[must_use]
pub fn slug_from_url(url: &str) -> Option<&str> {
    url.strip_prefix("bundled://")
}

/// Inject every entry in [`BUNDLED_PLUGINS`] into `settings.wasm_plugins`
/// unless the user has explicitly removed it.
///
/// Returns `true` if any entries were added (i.e. the caller should
/// persist `settings`). Idempotent on re-runs.
///
/// **Does not** change the `enabled` flag of pre-existing entries — if
/// the user disabled Discord on a previous launch, that state is kept.
pub fn inject_bundled_into_settings(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    for plugin in BUNDLED_PLUGINS {
        if settings
            .removed_bundled_plugins
            .iter()
            .any(|s| s == plugin.slug)
        {
            // User explicitly removed this — respect their intent.
            continue;
        }
        let url = plugin.url();
        if settings.wasm_plugins.iter().any(|e| e.url == url) {
            // Already present — preserve user's enabled/disabled state.
            continue;
        }
        settings.wasm_plugins.push(WasmPluginEntry {
            url,
            name: Some(plugin.display_name.to_string()),
            enabled: true,
            bundled: true,
        });
        changed = true;
    }
    changed
}

/// Build the [`SignupEntry`] for a bundled plugin slug.
///
/// Each compiled-in bundled backend (today: Discord + Teams) returns
/// `Some(entry)`. Returns `None` for unknown slugs or for slugs whose
/// underlying client crate wasn't compiled into this build (e.g.
/// `--no-default-features` releases that strip `dev-plugins`).
///
/// Lives in `bundled_plugins` (not `client_manager`) so that swapping
/// the runtime path later — e.g. when `clients/discord` is moved into
/// a true WASM Component Model plugin loaded by `poly-plugin-host` —
/// only requires editing this function. The host code that calls it
/// (`sync_bundled_signup_entries`) is unchanged.
#[must_use]
pub fn signup_entry_for_bundled_slug(slug: &str) -> Option<SignupEntry> {
    match slug {
        #[cfg(feature = "discord")]
        "discord" => Some(SignupEntry {
            slug: "discord",
            icon: "💬",
            name_key: "plugin-discord-signup-name",
            desc_key: "plugin-discord-signup-desc",
            render: poly_discord::signup::signup_render_fn,
        }),
        #[cfg(feature = "teams")]
        "teams" => Some(SignupEntry {
            slug: "teams",
            icon: "🟦",
            name_key: "plugin-teams-signup-name",
            desc_key: "plugin-teams-signup-desc",
            render: poly_teams::signup::signup_render_fn,
        }),
        _ => None,
    }
}

/// Reconcile `cm.signup_entries` for every bundled plugin against the
/// user's enabled/disabled state in `settings.wasm_plugins`.
///
/// For each bundled plugin slug:
/// - If the slug appears in [`bundled_enabled_slugs`] AND the underlying
///   client crate is compiled in, register its [`SignupEntry`].
/// - Otherwise, unregister it.
///
/// Idempotent — safe to call from `init_storage` AND from the
/// `/settings/plugins` toggle handler. Any compile-time-absent backend
/// (e.g. `discord` feature off) is silently skipped.
pub fn sync_bundled_signup_entries(cm: &mut ClientManager, settings: &AppSettings) {
    let enabled = bundled_enabled_slugs(settings);
    for plugin in BUNDLED_PLUGINS {
        let is_enabled = enabled.iter().any(|s| *s == plugin.slug);
        match (is_enabled, signup_entry_for_bundled_slug(plugin.slug)) {
            (true, Some(entry)) => cm.register_signup_entry(entry),
            (false, _) => cm.unregister_signup_entry(plugin.slug),
            (true, None) => {
                // Bundled plugin is enabled in settings but the host
                // wasn't compiled with the underlying feature — leave
                // signup_entries untouched. Plugins page renders the
                // row with a "not in this build" affordance.
                tracing::debug!(
                    "bundled plugin {} enabled in settings but feature not compiled in",
                    plugin.slug
                );
            }
        }
    }
}

/// Storage-aware wrapper: load `AppSettings`, inject bundled entries,
/// persist if anything changed.
///
/// Called from `init_storage` after the storage handle is registered.
/// Failures log a warning and return — bundled-plugin injection is a
/// best-effort UX nicety, not load-bearing.
pub async fn ensure_bundled_plugins(storage: &crate::storage::Storage) {
    let mut settings = match storage.get_app_settings().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "ensure_bundled_plugins: failed to read AppSettings: {e} — skipping injection"
            );
            return;
        }
    };
    if !inject_bundled_into_settings(&mut settings) {
        return;
    }
    if let Err(e) = storage.set_app_settings(&settings).await {
        tracing::warn!(
            "ensure_bundled_plugins: failed to persist updated AppSettings: {e}"
        );
    } else {
        tracing::info!(
            "ensure_bundled_plugins: injected {} bundled plugin entries",
            BUNDLED_PLUGINS.len()
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn bundled_plugins_includes_discord_and_teams() {
        let slugs: Vec<&str> = BUNDLED_PLUGINS.iter().map(|p| p.slug).collect();
        assert!(slugs.contains(&"discord"));
        assert!(slugs.contains(&"teams"));
    }

    #[test]
    fn url_scheme_is_bundled_slug() {
        let p = BundledPlugin {
            slug: "discord",
            display_name: "Discord",
        };
        assert_eq!(p.url(), "bundled://discord");
        assert!(is_bundled_url(&p.url()));
        assert_eq!(slug_from_url(&p.url()), Some("discord"));
        assert_eq!(slug_from_url("https://example.com/x.wasm"), None);
        assert!(!is_bundled_url("https://example.com/x.wasm"));
    }

    #[test]
    fn injection_adds_missing_entries() {
        let mut settings = AppSettings::default();
        assert!(settings.wasm_plugins.is_empty());
        let changed = inject_bundled_into_settings(&mut settings);
        assert!(changed);
        assert_eq!(settings.wasm_plugins.len(), BUNDLED_PLUGINS.len());
        for plugin in BUNDLED_PLUGINS {
            let entry = settings
                .wasm_plugins
                .iter()
                .find(|e| e.url == plugin.url())
                .expect("bundled plugin should be present after injection");
            assert!(entry.bundled);
            assert!(entry.enabled);
            assert_eq!(entry.name.as_deref(), Some(plugin.display_name));
        }
    }

    #[test]
    fn injection_is_idempotent() {
        let mut settings = AppSettings::default();
        assert!(inject_bundled_into_settings(&mut settings));
        // Second call should be a no-op.
        assert!(!inject_bundled_into_settings(&mut settings));
        assert_eq!(settings.wasm_plugins.len(), BUNDLED_PLUGINS.len());
    }

    #[test]
    fn injection_preserves_user_disabled_state() {
        let mut settings = AppSettings::default();
        // Pre-populate with a "disabled" Discord entry as if the user
        // had toggled it off on a previous launch.
        settings.wasm_plugins.push(WasmPluginEntry {
            url: "bundled://discord".to_string(),
            name: Some("Discord".to_string()),
            enabled: false,
            bundled: true,
        });
        let _ = inject_bundled_into_settings(&mut settings);
        // Discord stays disabled. Teams gets injected as enabled.
        let discord = settings
            .wasm_plugins
            .iter()
            .find(|e| e.url == "bundled://discord")
            .unwrap();
        assert!(!discord.enabled, "user's disabled state must be preserved");
        let teams = settings
            .wasm_plugins
            .iter()
            .find(|e| e.url == "bundled://teams")
            .unwrap();
        assert!(teams.enabled);
    }

    #[test]
    fn bundled_enabled_slugs_reflects_toggle_state() {
        let mut settings = AppSettings::default();
        // Empty settings → no bundled plugins enabled.
        assert!(bundled_enabled_slugs(&settings).is_empty());

        // Inject the defaults — both should be enabled.
        let _ = inject_bundled_into_settings(&mut settings);
        let slugs = bundled_enabled_slugs(&settings);
        assert!(slugs.contains(&"discord"));
        assert!(slugs.contains(&"teams"));

        // Toggle Teams off → only Discord remains.
        let teams = settings
            .wasm_plugins
            .iter_mut()
            .find(|e| e.url == "bundled://teams")
            .unwrap();
        teams.enabled = false;
        let slugs = bundled_enabled_slugs(&settings);
        assert!(slugs.contains(&"discord"));
        assert!(!slugs.contains(&"teams"));
    }

    #[test]
    fn bundled_plugin_by_slug_lookup() {
        assert_eq!(bundled_plugin_by_slug("discord").map(|p| p.slug), Some("discord"));
        assert_eq!(bundled_plugin_by_slug("teams").map(|p| p.slug), Some("teams"));
        assert!(bundled_plugin_by_slug("matrix").is_none());
    }

    #[test]
    fn removed_plugins_are_not_re_injected() {
        let mut settings = AppSettings::default();
        settings.removed_bundled_plugins.push("discord".to_string());
        assert!(inject_bundled_into_settings(&mut settings));
        // Only Teams should be injected — Discord is on the removed list.
        assert!(
            settings
                .wasm_plugins
                .iter()
                .all(|e| e.url != "bundled://discord"),
            "removed bundled plugin must not be re-injected"
        );
        assert!(
            settings
                .wasm_plugins
                .iter()
                .any(|e| e.url == "bundled://teams"),
            "non-removed bundled plugins still get injected"
        );
    }
}
