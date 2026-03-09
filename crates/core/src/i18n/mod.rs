//! Internationalization (i18n) system for Poly.
//!
//! Custom thin wrapper over `fluent-bundle` for Project Fluent `.ftl` files.
//! All user-facing strings MUST go through this system.
//!
//! Locale strings are embedded at compile time via `include_str!`.
//! ## Usage
//!
//! ```rust,ignore
//! use poly_core::i18n::t;
//!
//! // Simple key lookup
//! let greeting = t("app-title");
//!
//! // With arguments
//! let hello = t_args("hello-user", &[("name", "Alice")]);
//!
//! // With the macro (from lib.rs)
//! let s = poly_core::t!("app-title");
//! let s = poly_core::t!("hello-user", name => "Alice");
//! ```
//!
//! ## Reactive locale switching (Dioxus components)
//!
//! ```rust,ignore
//! // In App root — call once:
//! provide_locale_context();
//!
//! // In any child component:
//! let (locale_sig, set_locale_fn) = use_locale();
//! // reading locale_sig.read() subscribes to locale changes
//! set_locale_fn("de"); // updates global + signal, triggers re-renders
//! ```

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use unic_langid::LanguageIdentifier;

// Dioxus hooks (for reactive locale context)
use dioxus::prelude::*;

/// Supported locales.
pub const SUPPORTED_LOCALES: &[&str] = &["en", "de", "fr", "es"];

/// Default locale (English).
pub const DEFAULT_LOCALE: &str = "en";

/// Global i18n state.
static I18N: LazyLock<RwLock<I18nState>> = LazyLock::new(|| RwLock::new(I18nState::new()));

/// Internal i18n state holding loaded bundles per locale.
struct I18nState {
    current_locale: String,
    bundles: HashMap<String, FluentBundle<FluentResource>>,
    /// Extra FTL sources added by plugins at runtime.
    /// Keyed by (locale, plugin_id) → raw FTL source string.
    /// On locale switch / reload, these are merged back into the bundles.
    plugin_ftl: HashMap<(String, String), String>,
}

impl I18nState {
    fn new() -> Self {
        Self {
            current_locale: DEFAULT_LOCALE.to_string(),
            bundles: HashMap::new(),
            plugin_ftl: HashMap::new(),
        }
    }
}

/// Initialize the i18n system.
///
/// Detects the system locale via `sys_locale`, then pre-loads **all**
/// supported locales so that switching languages at runtime never hits a
/// "bundle not found" state (which would silently fall back to English).
///
/// Also registers native plugin FTL translations for any feature-gated native
/// backends (e.g. `demo`). This mirrors what the WASM plugin host does for
/// WASM plugins via the `plugin-metadata.get-translations` WIT interface, and
/// must run at `init()` time so that ALL entry points (web WASM, desktop,
/// mobile) get the plugin FTL registered before any component renders.
pub fn init() {
    let system_locale = sys_locale::get_locale().unwrap_or_else(|| DEFAULT_LOCALE.to_string());

    // Extract just the language code (e.g., "en-US" -> "en")
    let lang_code = system_locale.split('-').next().unwrap_or(DEFAULT_LOCALE);

    let initial_locale = if SUPPORTED_LOCALES.contains(&lang_code) {
        lang_code
    } else {
        DEFAULT_LOCALE
    };

    // Pre-load every supported locale so that runtime language switching
    // always finds a bundle in the map — no lazy "first-switch" fallback
    // to English bug.
    for locale in SUPPORTED_LOCALES {
        load_locale(locale);
    }

    // Register FTL for native backend plugins, mirroring the WIT
    // `plugin-metadata.get-translations(locale)` contract.
    // DECISION(DX): Native plugin FTL must be registered in i18n::init() so
    // all entry points (web/desktop/mobile) get translations regardless of
    // whether they call poly_core::init() or poly_core::i18n::init() directly.
    register_native_plugin_ftl();

    // Activate the detected locale
    set_locale(initial_locale);

    tracing::info!("i18n initialized with locale: {initial_locale} (all locales pre-loaded)");
}

/// Register FTL translations for all native backend plugins that are
/// compiled in via feature flags. Called from [`init`] to ensure every
/// entry point (web, desktop, mobile) gets the translations.
fn register_native_plugin_ftl() {
    #[cfg(feature = "demo")]
    {
        for locale in SUPPORTED_LOCALES {
            let src = poly_demo::plugin_translations(locale);
            if !src.is_empty() {
                register_plugin_ftl("demo", locale, src);
            }
        }
        tracing::debug!("Native demo plugin FTL registered for all locales");
    }
}

/// Load `.ftl` resources for a locale into the bundle store.
fn load_locale(locale: &str) {
    let langid: LanguageIdentifier = locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().unwrap_or_default());

    let mut bundle = FluentBundle::new_concurrent(vec![langid]);

    // Load embedded FTL strings
    // TODO(phase-2.4.1.4): Load from actual .ftl files (embedded or runtime)
    let ftl_source = get_embedded_ftl(locale);
    if let Ok(resource) = FluentResource::try_new(ftl_source)
        && let Err(errors) = bundle.add_resource(resource)
    {
        for err in errors {
            tracing::warn!("FTL error in locale {locale}: {err:?}");
        }
    }

    let mut state = I18N
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Also inject any plugin FTL that was registered for this locale
    let plugin_sources: Vec<String> = state
        .plugin_ftl
        .iter()
        .filter(|((loc, _), _)| loc == locale)
        .map(|(_, src)| src.clone())
        .collect();

    for src in plugin_sources {
        match FluentResource::try_new(src) {
            Ok(res) => {
                if let Err(errs) = bundle.add_resource(res) {
                    for e in errs {
                        tracing::warn!("Plugin FTL error in locale {locale}: {e:?}");
                    }
                }
            }
            Err((_res, errs)) => {
                for e in errs {
                    tracing::warn!("Plugin FTL parse error in locale {locale}: {e:?}");
                }
            }
        }
    }

    state.bundles.insert(locale.to_string(), bundle);
}

/// Register a plugin's FTL translation source for a given locale.
///
/// The host calls this during plugin load after calling the plugin's
/// `get-translations(locale)` WIT export for all supported locales.
/// The FTL is immediately merged into the live bundle for that locale.
///
/// ## Key convention
///
/// All message IDs in plugin FTL MUST be prefixed with `plugin-<plugin_id>-`.
/// Example for plugin `demo`:  
/// ```text
/// plugin-demo-title = Demo Settings
/// plugin-demo-setting-enabled-label = Enable Demo Data
/// ```
///
/// The host enforces nothing — plugins are responsible for correct prefixing.
/// Violating this may overwrite global keys, which is considered a plugin bug.
pub fn register_plugin_ftl(plugin_id: &str, locale: &str, ftl_source: String) {
    if ftl_source.trim().is_empty() {
        return;
    }
    {
        let mut state = I18N
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.plugin_ftl.insert(
            (locale.to_string(), plugin_id.to_string()),
            ftl_source.clone(),
        );
    }

    // Merge immediately into the live bundle if it already exists
    let langid: LanguageIdentifier = locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().unwrap_or_default());

    match FluentResource::try_new(ftl_source) {
        Ok(resource) => {
            let mut state = I18N
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(bundle) = state.bundles.get_mut(locale) {
                bundle.add_resource_overriding(resource);
                tracing::debug!("Plugin '{plugin_id}' FTL merged into locale '{locale}'");
            } else {
                // Bundle not loaded yet — create it
                drop(state);
                let mut bundle = FluentBundle::new_concurrent(vec![langid]);
                if let Ok(resource) = FluentResource::try_new(get_embedded_ftl(locale))
                    && let Err(errors) = bundle.add_resource(resource)
                {
                    for e in errors {
                        tracing::warn!("FTL error creating bundle for {locale}: {e:?}");
                    }
                }
                let mut state = I18N
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.bundles.insert(locale.to_string(), bundle);
            }
        }
        Err((_res, errs)) => {
            for e in errs {
                tracing::warn!("Plugin '{plugin_id}' FTL parse error for locale {locale}: {e:?}");
            }
        }
    }
}

/// Get embedded FTL source for a locale.
fn get_embedded_ftl(locale: &str) -> String {
    match locale {
        "en" => include_str!("../../../../locales/en/main.ftl").to_string(),
        "de" => include_str!("../../../../locales/de/main.ftl").to_string(),
        "fr" => include_str!("../../../../locales/fr/main.ftl").to_string(),
        "es" => include_str!("../../../../locales/es/main.ftl").to_string(),
        _ => include_str!("../../../../locales/en/main.ftl").to_string(),
    }
}

/// Set the current locale.
///
/// Also reloads the target locale bundle (re-merging all plugin FTL).
pub fn set_locale(locale: &str) {
    {
        let mut state = I18N
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if SUPPORTED_LOCALES.contains(&locale) {
            state.current_locale = locale.to_string();
            tracing::info!("Locale changed to: {locale}");
        } else {
            tracing::warn!("Unsupported locale: {locale}, keeping current");
            return;
        }
    }
    // Reload the bundle so any registered plugin FTL is (re-)merged
    load_locale(locale);
}

/// Get the current locale.
pub fn current_locale() -> String {
    let state = I18N
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.current_locale.clone()
}

/// Translate a key to the current locale.
///
/// Falls back to English if the key is not found in the current locale.
pub fn t(key: &str) -> String {
    t_args(key, &[])
}

/// Translate a key with named arguments.
///
/// Falls back to English if the key is not found in the current locale.
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    let state = I18N
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let fluent_args = if args.is_empty() {
        None
    } else {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(*k, *v);
        }
        Some(fa)
    };

    // Try current locale first
    if let Some(bundle) = state.bundles.get(&state.current_locale)
        && let Some(msg) = bundle.get_message(key)
        && let Some(pattern) = msg.value()
    {
        let mut errors = vec![];
        let result = bundle.format_pattern(pattern, fluent_args.as_ref(), &mut errors);
        if errors.is_empty() {
            return result.to_string();
        }
    }

    // Fallback to English
    if state.current_locale != DEFAULT_LOCALE
        && let Some(bundle) = state.bundles.get(DEFAULT_LOCALE)
        && let Some(msg) = bundle.get_message(key)
        && let Some(pattern) = msg.value()
    {
        let mut errors = vec![];
        let result = bundle.format_pattern(pattern, fluent_args.as_ref(), &mut errors);
        return result.to_string();
    }

    // Key not found anywhere — return the key itself as fallback
    tracing::warn!("Missing i18n key: {key}");
    key.to_string()
}

// ── Dioxus reactive hooks ─────────────────────────────────────────────────────

/// Provide a reactive locale [`Signal<String>`] as Dioxus context.
///
/// **Call once from the root [`crate::ui::App`] component.** Child components
/// can access the signal via [`use_locale`] and will automatically re-render
/// when the locale changes.
///
/// ```rust,ignore
/// // In App component:
/// provide_locale_context();
/// ```
pub fn provide_locale_context() {
    let sig: Signal<String> = use_signal(current_locale);
    provide_context(sig);
}

/// Access the reactive locale [`Signal<String>`] from Dioxus context.
///
/// Must be called inside a component (after [`provide_locale_context`] was
/// called in the root). Reading `sig.read()` subscribes the component —
/// it re-renders whenever the locale changes.
///
/// ```rust,ignore
/// let mut locale = use_locale();
/// let current = locale.read().clone(); // subscribes
/// locale.set("de".to_string());         // updates + triggers re-render
/// ```
pub fn use_locale() -> Signal<String> {
    use_context::<Signal<String>>()
}
