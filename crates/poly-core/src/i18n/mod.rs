//! Internationalization (i18n) system for Poly.
//!
//! Custom thin wrapper over `fluent-bundle` for Project Fluent `.ftl` files.
//! All user-facing strings MUST go through this system.
//!
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
//! ```

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use unic_langid::LanguageIdentifier;

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
}

impl I18nState {
    fn new() -> Self {
        Self {
            current_locale: DEFAULT_LOCALE.to_string(),
            bundles: HashMap::new(),
        }
    }
}

/// Initialize the i18n system.
///
/// Detects the system locale and loads the appropriate `.ftl` files.
/// Falls back to English if the system locale is not supported.
pub fn init() {
    let system_locale = sys_locale::get_locale().unwrap_or_else(|| DEFAULT_LOCALE.to_string());

    // Extract just the language code (e.g., "en-US" -> "en")
    let lang_code = system_locale.split('-').next().unwrap_or(DEFAULT_LOCALE);

    let locale = if SUPPORTED_LOCALES.contains(&lang_code) {
        lang_code
    } else {
        DEFAULT_LOCALE
    };

    // Load the locale bundle
    load_locale(locale);
    set_locale(locale);

    // Always load English as fallback
    if locale != DEFAULT_LOCALE {
        load_locale(DEFAULT_LOCALE);
    }

    tracing::info!("i18n initialized with locale: {locale}");
}

/// Load `.ftl` resources for a locale into the bundle store.
fn load_locale(locale: &str) {
    let langid: LanguageIdentifier = locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().unwrap());

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

    let mut state = I18N.write().expect("i18n lock poisoned");
    state.bundles.insert(locale.to_string(), bundle);
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
pub fn set_locale(locale: &str) {
    let mut state = I18N.write().expect("i18n lock poisoned");
    if SUPPORTED_LOCALES.contains(&locale) {
        state.current_locale = locale.to_string();
        tracing::info!("Locale changed to: {locale}");
    } else {
        tracing::warn!("Unsupported locale: {locale}, keeping current");
    }
}

/// Get the current locale.
pub fn current_locale() -> String {
    let state = I18N.read().expect("i18n lock poisoned");
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
    let state = I18N.read().expect("i18n lock poisoned");

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
