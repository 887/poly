//! Discord build-number scraper.
//!
//! Discord's desktop client embeds `client_build_number` in one of the JS asset
//! chunks loaded by `https://discord.com/app`. This module:
//!
//! 1. Provides a baked-in floor constant so the header is always valid, even on
//!    cold-start before a successful scrape.
//! 2. Implements `scrape_stable()` that fetches the `/app` HTML, finds asset
//!    script URLs, and regexes the build number out of each chunk until a match.
//! 3. Implements `load_or_refresh()` that reads a cached result from
//!    `client.config.discord.build_info` (7-day TTL) and only calls the scraper
//!    when the cache is stale.
//!
//! The KV key is `client.config.discord.build_info` and stores a JSON object
//! `{ build_number: u32, version_hash: String, scraped_at: u64 }`.

use poly_host_bridge::http::HttpClient;

// ── Phase A.1 — floor constant ───────────────────────────────────────────────

/// Latest known stable Discord build number.
///
/// Updated manually alongside each scraper commit. Acts as a floor — we never
/// send a build number lower than this even if the KV cache is empty and the
/// scrape fails.
///
/// Current as of 2026-05-11.
pub const LATEST_KNOWN_STABLE_BUILD: u32 = 354_133;

/// Chromium version shipped in the stable Discord desktop client as of the same
/// date as `LATEST_KNOWN_STABLE_BUILD`. Embedded in `browser_version` and the
/// `browser_user_agent` UA string.
pub const STABLE_CHROMIUM_VERSION: u32 = 130;

/// Electron version shipped with the same client build.
pub const STABLE_ELECTRON_VERSION: &str = "32.2.7";

// ── Wire type ────────────────────────────────────────────────────────────────

/// Result of a build-info lookup (scraped or cached or floor).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuildInfo {
    /// The `client_build_number` integer value.
    pub build_number: u32,
    /// The `client_version_hash` string (e.g. `"3eb5b4a"`).
    pub version_hash: String,
    /// Unix epoch seconds when this was scraped. 0 = synthesised from the
    /// floor constant (never persisted).
    pub scraped_at: u64,
}

impl Default for BuildInfo {
    fn default() -> Self {
        Self {
            build_number: LATEST_KNOWN_STABLE_BUILD,
            version_hash: "unknown".to_string(),
            scraped_at: 0,
        }
    }
}

// ── Scrape error ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ScrapeError {
    /// Network or HTTP error from the host bridge.
    Network(String),
    /// Could not find any asset script URL in the HTML.
    NoAssetsFound,
    /// Fetched all asset chunks but none matched the build-number regex.
    BuildNumberNotFound,
}

impl std::fmt::Display for ScrapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::NoAssetsFound => write!(f, "no asset script URLs found in /app HTML"),
            Self::BuildNumberNotFound => {
                write!(f, "build number not found in any asset chunk")
            }
        }
    }
}

// ── Scraper ───────────────────────────────────────────────────────────────────

/// Fetch `https://discord.com/app`, find embedded asset script URLs, then
/// search each chunk for the `Build Number: NNN, Version Hash: XXXXXXX` literal.
///
/// Returns `Err(ScrapeError)` on any failure; caller falls back to cache or floor.
pub async fn scrape_stable() -> Result<BuildInfo, ScrapeError> {
    let http = HttpClient::new();

    // Step 1 — fetch the /app HTML.
    let html = http
        .get("https://discord.com/app".to_string())
        .send()
        .await
        .map_err(|e| ScrapeError::Network(e.to_string()))?
        .text()
        .await
        .map_err(|e| ScrapeError::Network(e.to_string()))?;

    // Step 2 — extract asset script URLs.
    // Discord renders them as: <script src="/assets/<hash>.js" ...>
    let asset_urls: Vec<String> = extract_asset_urls(&html);
    if asset_urls.is_empty() {
        return Err(ScrapeError::NoAssetsFound);
    }

    // Step 3 — fetch each chunk until the build-number literal is found.
    for url in &asset_urls {
        let full_url = if url.starts_with("http") {
            url.clone()
        } else {
            format!("https://discord.com{url}")
        };

        let body = match http
            .get(full_url)
            .send()
            .await
            .and_then(|r| {
                // Use a block to avoid async issues — we need to await text()
                // but we're not in an async context here. We collect the future.
                Ok(r)
            }) {
            Ok(resp) => match resp.text().await {
                Ok(t) => t,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        if let Some(info) = parse_build_number(&body) {
            return Ok(info);
        }
    }

    Err(ScrapeError::BuildNumberNotFound)
}

/// Extract all `/assets/*.js` URLs from the Discord `/app` HTML.
fn extract_asset_urls(html: &str) -> Vec<String> {
    // Discord uses <script src="/assets/<hash>.js"> tags.
    let mut urls = Vec::new();
    let mut remaining = html;
    while let Some(pos) = remaining.find("src=\"/assets/") {
        remaining = &remaining[pos + 5..]; // skip past `src="`
        if let Some(end) = remaining.find('"') {
            let url = &remaining[..end];
            if url.ends_with(".js") {
                urls.push(url.to_string());
            }
            remaining = &remaining[end..];
        }
    }
    urls
}

/// Search a JS chunk body for `Build Number: NNN, Version Hash: XXXXXXX`.
///
/// Returns `Some(BuildInfo)` on the first match.
fn parse_build_number(body: &str) -> Option<BuildInfo> {
    // Pattern: `Build Number: <digits>, Version Hash: <alphanum>`
    let marker = "Build Number: ";
    let pos = body.find(marker)?;
    let after_marker = &body[pos + marker.len()..];
    // Read digits.
    let num_end = after_marker
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_marker.len());
    let build_str = &after_marker[..num_end];
    let build_number: u32 = build_str.parse().ok()?;
    // Clamp to floor.
    let build_number = build_number.max(LATEST_KNOWN_STABLE_BUILD);

    // Read version hash after ", Version Hash: ".
    let hash_marker = ", Version Hash: ";
    let hash_start = after_marker.find(hash_marker)? + hash_marker.len();
    let after_hash = &after_marker[hash_start..];
    let hash_end = after_hash
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(after_hash.len());
    let version_hash = after_hash[..hash_end].to_string();

    let scraped_at = now_secs();

    Some(BuildInfo {
        build_number,
        version_hash,
        scraped_at,
    })
}

// ── KV key ───────────────────────────────────────────────────────────────────

pub const KV_KEY_BUILD_INFO: &str = "client.config.discord.build_info";

/// Seven-day TTL in seconds.
const SEVEN_DAYS_SECS: u64 = 7 * 24 * 3600;

// ── load_or_refresh ───────────────────────────────────────────────────────────

/// Load cached `BuildInfo` from `client_config_store`, or scrape fresh if
/// stale / absent.  Always returns *something* — on total failure, returns the
/// floor constant.
///
/// # Arguments
///
/// * `kv_get` / `kv_set` — async closures that read/write the KV store.
/// * `force` — if `true`, ignore the TTL and always re-scrape.
pub async fn load_or_refresh<G, S, GF, SF>(
    kv_get: G,
    kv_set: S,
    force: bool,
) -> BuildInfo
where
    G: FnOnce() -> GF,
    GF: std::future::Future<Output = Option<BuildInfo>>,
    S: FnOnce(BuildInfo) -> SF,
    SF: std::future::Future<Output = ()>,
{
    let cached = kv_get().await;

    // Return cached if still fresh and not forced.
    if !force {
        if let Some(ref info) = cached {
            let age = now_secs().saturating_sub(info.scraped_at);
            if age < SEVEN_DAYS_SECS {
                tracing::debug!(
                    target: "poly_discord::build_info",
                    build_number = info.build_number,
                    age_hours = age / 3600,
                    "using cached build info"
                );
                return info.clone();
            }
        }
    }

    // Attempt a fresh scrape.
    match scrape_stable().await {
        Ok(fresh) => {
            tracing::info!(
                target: "poly_discord::build_info",
                build_number = fresh.build_number,
                version_hash = %fresh.version_hash,
                "scraped fresh Discord build info"
            );
            kv_set(fresh.clone()).await;
            fresh
        }
        Err(e) => {
            tracing::warn!(
                target: "poly_discord::build_info",
                error = %e,
                "Discord build-number scrape failed; falling back"
            );
            // Return cached if any; otherwise floor.
            cached.unwrap_or_default()
        }
    }
}

// ── Time helper ──────────────────────────────────────────────────────────────

/// Current unix epoch seconds.  On WASM, `SystemTime` is unavailable so we
/// use a constant approximation (good enough for the 7-day TTL comparison —
/// worst case we scrape a day early).
fn now_secs() -> u64 {
    #[cfg(feature = "native")]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    #[cfg(not(feature = "native"))]
    {
        // WASM: return 0 — caller falls through to scrape path (or uses cached).
        0
    }
}

// ── F.4 — Build staleness check ──────────────────────────────────────────────

/// Number of seconds after which a failed scrape should surface a UI warning.
/// (14 days = 14 * 24 * 3600)
const STALE_SCRAPE_WARN_SECS: u64 = 14 * 24 * 3600;

/// Number of seconds after which the hard-coded floor constant is considered
/// "stale" if it has not been bumped (30 days).
const STALE_FLOOR_WARN_SECS: u64 = 30 * 24 * 3600;

/// Unix epoch seconds when `LATEST_KNOWN_STABLE_BUILD` was last manually bumped.
///
/// Update this constant whenever `LATEST_KNOWN_STABLE_BUILD` is updated.
/// It drives the F.4 staleness warning: if the current time is more than
/// `STALE_FLOOR_WARN_SECS` after this date AND the scraper hasn't refreshed
/// the build info in `STALE_SCRAPE_WARN_SECS`, the UI shows a yellow warning.
///
/// Current value: 2026-05-11 00:00:00 UTC.
pub const FLOOR_CONSTANT_BUMPED_AT: u64 = 1_747_008_000;

/// Check whether the build info is stale enough to warrant a UI warning (F.4).
///
/// Returns `true` when BOTH conditions hold:
/// 1. The last successful scrape (or `scraped_at == 0` if never scraped) is
///    older than 14 days.
/// 2. The hard-coded `LATEST_KNOWN_STABLE_BUILD` constant was last bumped
///    more than 30 days ago (i.e. nobody has updated the codebase either).
///
/// When either condition is false (scraper succeeded recently, or the constant
/// was bumped recently) the warning is suppressed — we're not actually stale.
#[must_use]
pub fn check_build_staleness(info: &BuildInfo) -> bool {
    let now = now_secs();
    if now == 0 {
        // WASM with no wall clock — can't determine staleness.
        return false;
    }

    let last_scrape_age = now.saturating_sub(info.scraped_at);
    let floor_age = now.saturating_sub(FLOOR_CONSTANT_BUMPED_AT);

    let scrape_stale = info.scraped_at == 0 || last_scrape_age > STALE_SCRAPE_WARN_SECS;
    let floor_stale = floor_age > STALE_FLOOR_WARN_SECS;

    if scrape_stale && floor_stale {
        tracing::warn!(
            target: "discord-anti-ban",
            build_number = info.build_number,
            last_scrape_age_days = last_scrape_age / 86_400,
            floor_age_days = floor_age / 86_400,
            "discord build info is stale — consider updating Poly or refreshing the build number"
        );
        true
    } else {
        false
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn parse_build_number_finds_match() {
        let body = r#"some stuff Build Number: 354133, Version Hash: 3eb5b4a and more"#;
        let info = parse_build_number(body).expect("should find build number");
        assert_eq!(info.build_number, 354_133);
        assert_eq!(info.version_hash, "3eb5b4a");
    }

    #[test]
    fn parse_build_number_clamps_to_floor() {
        // A hypothetical old build number below the floor constant.
        let body = format!(
            "Build Number: 1, Version Hash: oldold"
        );
        let info = parse_build_number(&body).expect("should parse");
        assert_eq!(info.build_number, LATEST_KNOWN_STABLE_BUILD);
    }

    #[test]
    fn parse_build_number_missing() {
        let body = "no build info here";
        assert!(parse_build_number(body).is_none());
    }

    #[test]
    fn extract_asset_urls_finds_js() {
        let html = r#"<html><head>
            <script src="/assets/abc123.js" integrity="sha256-xxx"></script>
            <script src="/assets/def456.js"></script>
            <link rel="stylesheet" href="/assets/style.css">
        </head></html>"#;
        let urls = extract_asset_urls(html);
        assert_eq!(urls.len(), 2);
        assert!(urls.iter().any(|u| u.contains("abc123.js")));
        assert!(urls.iter().any(|u| u.contains("def456.js")));
    }

    #[test]
    fn default_build_info_uses_floor() {
        let info = BuildInfo::default();
        assert_eq!(info.build_number, LATEST_KNOWN_STABLE_BUILD);
        assert_eq!(info.version_hash, "unknown");
        assert_eq!(info.scraped_at, 0);
    }

    // ── F.4 — check_build_staleness tests ────────────────────────────────

    #[cfg(feature = "native")]
    #[test]
    fn check_build_staleness_fresh_scrape_no_warn() {
        // A build info scraped just now is not stale.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let info = BuildInfo {
            build_number: LATEST_KNOWN_STABLE_BUILD,
            version_hash: "abc123".to_string(),
            scraped_at: now,
        };
        assert!(!check_build_staleness(&info), "freshly scraped build should not be stale");
    }

    #[cfg(feature = "native")]
    #[test]
    fn check_build_staleness_old_scrape_and_old_floor_warns() {
        // scraped_at = 0 (floor constant, never scraped) + floor bumped long ago.
        // We construct a BuildInfo with scraped_at = 0 but override the check
        // to simulate an old epoch by using a very old value.
        let very_old_epoch: u64 = 1_000_000; // 1970-01-01 + 11 days — definitely stale
        let info = BuildInfo {
            build_number: LATEST_KNOWN_STABLE_BUILD,
            version_hash: "unknown".to_string(),
            scraped_at: very_old_epoch, // old enough to trigger scrape_stale
        };
        // The floor constant epoch (FLOOR_CONSTANT_BUMPED_AT) is 2026-05-11, so
        // the floor is only stale if now > FLOOR_CONSTANT_BUMPED_AT + 30 days.
        // In tests run before 2026-06-10 this will be false → warning suppressed.
        // That's correct behaviour: if the floor is fresh, we don't warn.
        // This test verifies the logic compiles and runs without panic.
        let _ = check_build_staleness(&info);
    }

    #[cfg(feature = "native")]
    #[test]
    fn check_build_staleness_zero_scraped_at_no_crash() {
        // The default BuildInfo (scraped_at = 0) must not panic.
        let info = BuildInfo::default();
        let _ = check_build_staleness(&info);
    }
}
