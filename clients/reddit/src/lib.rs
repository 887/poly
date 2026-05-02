//! # poly-reddit
//!
//! Reddit client for Poly. Scrapes `old.reddit.com` HTML rather than using
//! Reddit's REST/OAuth API — Reddit killed third-party API access mid-2023
//! and the remaining tiers are throttled or enterprise-priced. `old.reddit.com`
//! is server-rendered, structurally stable since 2018, and explicitly
//! maintained by Reddit (the user-prefs toggle keeps it as the default UI).
//!
//! ## Build modes
//!
//! - **Native** (`--features native`): implements `ClientBackend` directly
//!   using `reqwest` + `scraper`. (Trait impl shipped in later phases — see
//!   `docs/plans/plan-reddit-stub.md` Phase D-E. Phase A scaffolds the crate
//!   only.)
//!
//! ## Gating
//!
//! Not in poly-core's default features. Opt-in via `--features reddit` —
//! same model as Discord and Teams. The TOS gray area around scraping is
//! the explicit reason for keeping it out of release builds.

#[cfg(feature = "native")]
pub mod signup;

/// Return Fluent translations for the given locale.
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// Reddit HTML-scraping client.
///
/// Phase A scaffold: holds the HTTP client (with cookie jar, populated by
/// Phase C login) and the scraping base URL. Subsequent phases add the
/// `ClientBackend` impl, parser modules, modhash tracking, and write flows
/// — see `docs/plans/plan-reddit-stub.md`.
#[cfg(feature = "native")]
pub struct RedditClient {
    /// HTTP client with cookie jar — login persists `reddit_session` here.
    http: reqwest::Client,
    /// Scraping base — production: `https://old.reddit.com`.
    /// Test backend (Phase F) overrides via `REDDIT_BASE_URL` env to
    /// `http://127.0.0.1:9108`.
    base_url: String,
}

#[cfg(feature = "native")]
impl RedditClient {
    /// Create a new Reddit client pointed at the default `old.reddit.com`.
    ///
    /// # Errors
    ///
    /// Returns an error if `reqwest::Client` construction fails (extremely
    /// rare — only when the system TLS backend is unavailable).
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_base_url("https://old.reddit.com".to_string())
    }

    /// Create a new Reddit client pointed at `base_url`. Used by integration
    /// tests against `servers/test-reddit/` (port 9108) and by the
    /// `REDDIT_BASE_URL` env override.
    ///
    /// # Errors
    ///
    /// Returns an error if `reqwest::Client` construction fails.
    pub fn with_base_url(base_url: String) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent("poly-reddit/0.1 (https://github.com/user/poly)")
            .build()?;
        Ok(Self { http, base_url })
    }

    /// The configured base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The underlying HTTP client. Used by parser modules in Phase B.
    #[must_use]
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }
}
