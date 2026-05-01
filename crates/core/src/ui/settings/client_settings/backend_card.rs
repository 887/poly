//! `BackendCard` — per-backend client settings, rendered inline (no collapse).
//!
//! Embedded inside each plugin's settings section. Renders a `settings-toggle-row`
//! for the version override followed by one row per mechanism. Mirrors the
//! styling of the polished plugin toggles (e.g. Poly Server's "Use WebSocket").
//!
//! Reactive hygiene:
//! - `use_future` for one-shot mechanism load on mount.
//! - All signal writes use `.set()` — no raw `Signal::write()`.
//! - No `use_effect` with non-Signal captures.

use super::mechanism_toggle::MechanismToggle;
use super::mcp::{client_settings_list_mechanisms, client_settings_set_mechanism};
use super::version_override::VersionOverrideEditor;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};
use serde_json::Value;

/// Parse `(mechanism_id, enabled)` pairs from a `client_settings_list_mechanisms`
/// JSON response. Returns an empty vec on parse error.
fn parse_mechanisms(json: &Value) -> Vec<(String, bool)> {
    json.get("mechanisms")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("mechanism_id")?.as_str()?.to_owned();
                    let enabled = m.get("enabled").and_then(serde_json::Value::as_bool).unwrap_or(false);
                    Some((id, enabled))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One backend's client-config rows (version override + mechanisms), inlined
/// into the parent plugin section. No collapse — everything is visible.
///
/// Props:
/// - `backend_id`: slug used for data-testid + MCP calls.
/// - `version_override`: active override string, or `None`.
/// - `effective_version`: the version string currently in effect.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
pub fn BackendCard(
    backend_id: String,
    effective_version: String,
    version_override: Option<String>,
) -> Element {
    let mut mechanisms: Signal<Vec<(String, bool)>> = use_signal(Vec::new);

    // Eager mount load — keeps the rows visible without a click.
    let bid_load = backend_id.clone();
    use_future(move || {
        let bid = bid_load.clone();
        async move {
            match client_settings_list_mechanisms(&bid).await {
                Ok(json) => mechanisms.set(parse_mechanisms(&json)),
                Err(e) => tracing::warn!("BackendCard: list_mechanisms failed for {bid}: {e}"),
            }
        }
    });

    let backend_id_reload = backend_id.clone();
    let backend_id_set    = backend_id.clone();
    let backend_id_testid = backend_id.clone();

    let reload_mechs = {
        let bid = backend_id_reload.clone();
        move || {
            let bid = bid.clone();
            spawn(async move {
                match client_settings_list_mechanisms(&bid).await {
                    Ok(json) => mechanisms.set(parse_mechanisms(&json)),
                    Err(e) => tracing::warn!("BackendCard: reload mechs failed for {bid}: {e}"),
                }
            });
        }
    };

    rsx! {
        div {
            class: "client-settings-backend-rows",
            "data-testid": "client-settings-backend-{backend_id_testid}",

            VersionOverrideEditor {
                backend_id: backend_id.clone(),
                current_version: effective_version.clone(),
                current_override: version_override.clone(),
                on_changed: {
                    let reload = reload_mechs.clone();
                    move |_| reload()
                },
            }

            for (mech_id, enabled) in mechanisms.read().clone() {
                {
                    let mech_id_clone = mech_id.clone();
                    let bid = backend_id_set.clone();
                    rsx! {
                        MechanismToggle {
                            key: "{mech_id_clone}",
                            backend_id: bid.clone(),
                            mechanism_id: mech_id_clone.clone(),
                            label: mech_id_clone.clone(),
                            enabled,
                            requires_host_cap: None,
                            on_toggle: {
                                let bid2 = bid.clone();
                                let mid = mech_id_clone.clone();
                                move |new_val: bool| {
                                    let bid3 = bid2.clone();
                                    let mid2 = mid.clone();
                                    spawn(async move {
                                        if let Err(e) = client_settings_set_mechanism(
                                            &bid3, &mid2, new_val
                                        ).await {
                                            tracing::warn!(
                                                "set_mechanism {mid2} on {bid3} failed: {e}"
                                            );
                                        }
                                        match client_settings_list_mechanisms(&bid3).await {
                                            Ok(json) => mechanisms.set(parse_mechanisms(&json)),
                                            Err(e) => tracing::warn!("reload mechs failed: {e}"),
                                        }
                                    });
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
