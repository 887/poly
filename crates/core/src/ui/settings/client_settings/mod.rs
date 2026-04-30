//! Client settings section — per-backend version override + mechanism toggles.
//!
//! Mounted in `account/settings/mod.rs` between the Profile block and the
//! Notifications block, under `id="acct-section-client-config"`.
//!
//! ## Architecture
//! ```text
//! ClientSettingsSection          ← top-level: fetches client_settings_list
//!   BackendCard (× N)            ← one card per backend, collapsed by default
//!     VersionOverrideEditor      ← toggle + text input + save/clear
//!     MechanismToggle (× M)      ← per-mechanism checkbox
//! ```
//!
//! ## Reactive hygiene
//! - `use_future` for one-shot mount load (no stale-capture risk).
//! - All signal writes use `.set()` — no raw `Signal::write()`.
//! - `.peek()` for backend-id reads used as hook keys (hang-class #7).
//! - No `use_effect` with non-Signal captures (hang-class #6).

pub mod backend_card;
pub mod mechanism_toggle;
pub mod mcp;
pub mod version_override;

use backend_card::BackendCard;
use crate::i18n::t;
use dioxus::prelude::*;
use mcp::client_settings_list;
use poly_ui_macros::{context_menu, ui_action};
use serde_json::Value;

/// Parse the backend list from a `client_settings_list` (all-backends) JSON response.
/// Returns a vec of `(backend_id, effective_version, version_override)`.
fn parse_backend_list(json: &Value) -> Vec<(String, String, Option<String>)> {
    let arr = match json.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .filter_map(|item| {
            let id = item.get("backend_id")?.as_str()?.to_owned();
            // `version_override` is null when not set; stringify when present.
            let version_override = item
                .get("version_override")
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned());
            // The snapshot doesn't carry `effective_version` directly —
            // use the override if set, otherwise use the fallback "default".
            let effective = version_override
                .clone()
                .unwrap_or_else(|| "default".to_owned());
            Some((id, effective, version_override))
        })
        .collect()
}

/// Top-level client settings section.
///
/// Loads all backends on mount via `client_settings_list` (no backend_id),
/// then renders one `BackendCard` per backend.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
pub fn ClientSettingsSection() -> Element {
    let mut backends: Signal<Vec<(String, String, Option<String>)>> = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    // One-shot load on mount — no deps capture, no stale-effect risk.
    use_future(move || async move {
        match client_settings_list().await {
            Ok(json) => {
                backends.set(parse_backend_list(&json));
            }
            Err(e) => {
                tracing::warn!("ClientSettingsSection: client_settings_list failed: {e}");
                error.set(Some(e));
            }
        }
        loading.set(false);
    });

    rsx! {
        div {
            class: "settings-section client-settings-section",
            "data-testid": "client-settings-section",

            h2 { class: "settings-section-title", "{t(\"client-settings-title\")}" }
            p { class: "settings-section-blurb", "{t(\"client-settings-blurb\")}" }

            if *loading.read() {
                div { class: "client-settings-loading-state", "…" }
            } else if let Some(err) = error.read().clone() {
                div { class: "client-settings-error-state",
                    "{t(\"client-settings-title\")}: {err}"
                }
            } else if backends.read().is_empty() {
                div { class: "client-settings-empty-state",
                    "No backends configured."
                }
            } else {
                div { class: "client-settings-backend-list",
                    for (backend_id, effective_version, version_override) in backends.read().clone() {
                        BackendCard {
                            key: "{backend_id}",
                            backend_id: backend_id.clone(),
                            effective_version: effective_version.clone(),
                            version_override: version_override.clone(),
                        }
                    }
                }
            }
        }
    }
}
