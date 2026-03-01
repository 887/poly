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
/// Detects the system locale via `sys_locale`, then pre-loads **all**
/// supported locales so that switching languages at runtime never hits a
/// "bundle not found" state (which would silently fall back to English).
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

    // Activate the detected locale
    set_locale(initial_locale);

    tracing::info!("i18n initialized with locale: {initial_locale} (all locales pre-loaded)");
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
    let mut state = I18N
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if SUPPORTED_LOCALES.contains(&locale) {
        state.current_locale = locale.to_string();
        tracing::info!("Locale changed to: {locale}");
    } else {
        tracing::warn!("Unsupported locale: {locale}, keeping current");
    }
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
