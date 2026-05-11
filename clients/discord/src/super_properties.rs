//! Discord `X-Super-Properties` header builder.
//!
//! `X-Super-Properties` is a base64-encoded JSON fingerprint Discord uses to
//! verify that requests originate from a genuine desktop client. Its fields
//! must be self-consistent with the gateway IDENTIFY payload.
//!
//! ## Single source of truth
//!
//! `SuperProperties` is constructed once per backend session and shared between
//! the HTTP layer (where it is base64-encoded into the header) and the gateway
//! layer (where it is inlined as JSON in op-2 IDENTIFY `properties`). Sending
//! different values on HTTP vs WS is the highest-confidence ban signal per the
//! discord.py-self issue tracker.
//!
//! ## Schema reference
//! KhafraDev/discord-verify wiki + greg6775/Discord-Api-Endpoints.

use crate::build_info::BuildInfo;

// ── Struct ────────────────────────────────────────────────────────────────────

/// All fields present in a real Discord desktop client `X-Super-Properties`.
///
/// Field presence matches what the official 0.0.354 stable client sends per
/// KhafraDev's discord-verify wiki. `client_event_source` is literal JSON null.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SuperProperties {
    pub os: String,
    pub browser: String,
    pub device: String,
    pub system_locale: String,
    pub browser_user_agent: String,
    pub browser_version: String,
    pub os_version: String,
    pub referrer: String,
    pub referring_domain: String,
    pub referrer_current: String,
    pub referring_domain_current: String,
    pub release_channel: String,
    pub client_build_number: u32,
    /// Always JSON null in the real client.
    pub client_event_source: serde_json::Value,
}

impl SuperProperties {
    /// Build the `SuperProperties` for the current host platform, injecting the
    /// live `build_number` from `build_info`.
    ///
    /// Platform is selected at compile time via `cfg!(target_os = ...)`.
    /// The `system_locale` argument is the BCP-47 locale string; pass
    /// `"en-US"` if detection fails.
    #[must_use]
    pub fn for_platform(build_info: &BuildInfo, system_locale: &str) -> Self {
        let build_number = build_info.build_number;
        let chromium = crate::build_info::STABLE_CHROMIUM_VERSION;
        let electron = crate::build_info::STABLE_ELECTRON_VERSION;

        #[cfg(target_os = "macos")]
        return Self::mac_desktop_template(build_number, chromium, electron, system_locale);

        #[cfg(target_os = "windows")]
        return Self::windows_desktop_template(build_number, chromium, electron, system_locale);

        // Linux (default — also used for WASM where target_os is unknown).
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Self::linux_chrome_desktop_template(build_number, chromium, electron, system_locale)
    }

    /// Linux / Electron desktop template — used by Poly's Wry and Electron
    /// shells running on Linux, and as the WASM fallback.
    #[must_use]
    pub fn linux_chrome_desktop_template(
        build_number: u32,
        chromium: u32,
        electron: &str,
        locale: &str,
    ) -> Self {
        let ua = format!(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
             (KHTML, like Gecko) discord/0.0.{build_number} \
             Chrome/{chromium}.0.0.0 Electron/{electron} Safari/537.36"
        );
        Self {
            os: "Linux".into(),
            browser: "Discord Client".into(),
            device: "".into(),
            system_locale: locale.to_string(),
            browser_user_agent: ua,
            browser_version: format!("{chromium}.0.0.0"),
            os_version: "".into(),
            referrer: "".into(),
            referring_domain: "".into(),
            referrer_current: "".into(),
            referring_domain_current: "".into(),
            release_channel: "stable".into(),
            client_build_number: build_number,
            client_event_source: serde_json::Value::Null,
        }
    }

    /// macOS desktop template.
    #[must_use]
    pub fn mac_desktop_template(
        build_number: u32,
        chromium: u32,
        electron: &str,
        locale: &str,
    ) -> Self {
        let ua = format!(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
             (KHTML, like Gecko) discord/0.0.{build_number} \
             Chrome/{chromium}.0.0.0 Electron/{electron} Safari/537.36"
        );
        Self {
            os: "Mac OS X".into(),
            browser: "Discord Client".into(),
            device: "".into(),
            system_locale: locale.to_string(),
            browser_user_agent: ua,
            browser_version: format!("{chromium}.0.0.0"),
            os_version: "10.15.7".into(),
            referrer: "".into(),
            referring_domain: "".into(),
            referrer_current: "".into(),
            referring_domain_current: "".into(),
            release_channel: "stable".into(),
            client_build_number: build_number,
            client_event_source: serde_json::Value::Null,
        }
    }

    /// Windows desktop template.
    #[must_use]
    pub fn windows_desktop_template(
        build_number: u32,
        chromium: u32,
        electron: &str,
        locale: &str,
    ) -> Self {
        let ua = format!(
            "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 \
             (KHTML, like Gecko) discord/0.0.{build_number} \
             Chrome/{chromium}.0.0.0 Electron/{electron} Safari/537.36"
        );
        Self {
            os: "Windows".into(),
            browser: "Discord Client".into(),
            device: "".into(),
            system_locale: locale.to_string(),
            browser_user_agent: ua,
            browser_version: format!("{chromium}.0.0.0"),
            os_version: "10".into(),
            referrer: "".into(),
            referring_domain: "".into(),
            referrer_current: "".into(),
            referring_domain_current: "".into(),
            release_channel: "stable".into(),
            client_build_number: build_number,
            client_event_source: serde_json::Value::Null,
        }
    }

    /// Encode for the `X-Super-Properties` HTTP header: compact JSON then base64.
    ///
    /// Works on both native and WASM — the base64 crate is always available via
    /// the workspace dep (we removed the `#[cfg(feature = "native")]` gate).
    #[must_use]
    pub fn to_header_value(&self) -> String {
        use base64::Engine as _;
        let json = serde_json::to_string(self).unwrap_or_default();
        base64::engine::general_purpose::STANDARD.encode(json.as_bytes())
    }

    /// Return the properties as a raw JSON `Value` for embedding inside a
    /// gateway IDENTIFY (op 2) `d.properties` field.  Same JSON object as the
    /// HTTP header, without the base64 wrapping.
    #[must_use]
    pub fn to_identify_properties(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Object(Default::default()))
    }

    /// Apply a User-Agent override.  The UA string is propagated into both
    /// `browser_user_agent` and is what's sent as the HTTP `User-Agent` header.
    /// Callers who want to honour `client.config.discord.version_override` should
    /// call this after construction.
    pub fn apply_ua_override(&mut self, ua_override: &str) {
        self.browser_user_agent = ua_override.to_string();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::build_info::{BuildInfo, LATEST_KNOWN_STABLE_BUILD};

    fn sample_build() -> BuildInfo {
        BuildInfo {
            build_number: LATEST_KNOWN_STABLE_BUILD,
            version_hash: "3eb5b4a".to_string(),
            scraped_at: 1_000_000,
        }
    }

    #[test]
    fn linux_template_no_discordbot_in_ua() {
        let props = SuperProperties::for_platform(&sample_build(), "en-US");
        assert!(
            !props.browser_user_agent.contains("DiscordBot"),
            "User-Agent must not contain 'DiscordBot' on a user-token request. Got: {}",
            props.browser_user_agent
        );
    }

    #[test]
    fn to_header_value_not_empty() {
        let props = SuperProperties::for_platform(&sample_build(), "en-US");
        let encoded = props.to_header_value();
        assert!(!encoded.is_empty(), "X-Super-Properties header must not be empty");
    }

    #[test]
    fn header_round_trips_to_valid_json() {
        use base64::Engine as _;
        let props = SuperProperties::for_platform(&sample_build(), "en-US");
        let encoded = props.to_header_value();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .expect("valid base64");
        let json: serde_json::Value =
            serde_json::from_slice(&decoded).expect("valid JSON after base64 decode");

        // Check required fields.
        assert!(json.get("os").is_some(), "os field missing");
        assert!(json.get("browser").is_some(), "browser field missing");
        assert!(json.get("client_build_number").is_some(), "client_build_number missing");
        assert!(json.get("system_locale").is_some(), "system_locale missing");
        assert!(json.get("browser_user_agent").is_some(), "browser_user_agent missing");
        assert!(json.get("release_channel").is_some(), "release_channel missing");
        assert!(
            json.get("client_event_source")
                .is_some_and(|v| v.is_null()),
            "client_event_source must be JSON null"
        );
    }

    #[test]
    fn build_number_in_header_matches_build_info() {
        use base64::Engine as _;
        let build = sample_build();
        let props = SuperProperties::for_platform(&build, "en-US");
        let encoded = props.to_header_value();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .expect("base64 decode");
        let json: serde_json::Value =
            serde_json::from_slice(&decoded).expect("json parse");
        let bn = json["client_build_number"].as_u64().expect("u64");
        assert_eq!(
            bn,
            u64::from(build.build_number),
            "client_build_number in header must equal BuildInfo.build_number"
        );
    }

    #[test]
    fn to_identify_properties_matches_header_json() {
        use base64::Engine as _;
        let build = sample_build();
        let props = SuperProperties::for_platform(&build, "en-US");

        // Decode the header to get the JSON object.
        let encoded = props.to_header_value();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .expect("base64 decode");
        let header_json: serde_json::Value =
            serde_json::from_slice(&decoded).expect("json parse");

        // The identify properties should match the header JSON exactly.
        let identify_json = props.to_identify_properties();
        assert_eq!(
            header_json, identify_json,
            "IDENTIFY properties must be byte-equal to X-Super-Properties JSON"
        );
    }

    #[test]
    fn ua_override_propagates() {
        let build = sample_build();
        let mut props = SuperProperties::for_platform(&build, "en-US");
        props.apply_ua_override("Mozilla/5.0 (Custom Override)");
        assert_eq!(props.browser_user_agent, "Mozilla/5.0 (Custom Override)");
        // Confirm no DiscordBot in the overridden UA.
        assert!(!props.browser_user_agent.contains("DiscordBot"));
    }
}
