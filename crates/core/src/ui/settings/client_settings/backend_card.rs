//! `BackendCard` — per-backend settings card: version override + mechanisms.
//!
//! Collapsed by default (with a toggle header) so 10-backend lists stay tidy.
//!
//! Reactive hygiene:
//! - `use_signal` for collapse state — single-component local, `.set()` only.
//! - Mechanism enable/disable fires an MCP call and reloads the backend snapshot.
//! - No raw `Signal::write()` or stale-capture `use_effect`.

use super::mechanism_toggle::MechanismToggle;
use super::mcp::{client_settings_list_mechanisms, client_settings_set_mechanism};
use super::version_override::VersionOverrideEditor;
use crate::i18n::t;
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
                    let enabled = m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                    Some((id, enabled))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One backend settings card (version + mechanisms).
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
    // Collapsed by default.
    let mut expanded = use_signal(|| false);

    // Mechanism list — loaded lazily when the card is expanded.
    let mut mechanisms: Signal<Vec<(String, bool)>> = use_signal(Vec::new);
    let mut mechs_loading = use_signal(|| false);
    let mut mechs_error: Signal<Option<String>> = use_signal(|| None);

    let backend_id_expand = backend_id.clone();
    let backend_id_reload = backend_id.clone();
    let backend_id_testid = backend_id.clone();
    let backend_id_set = backend_id.clone();

    // Load mechanisms when expanding.
    let mut load_mechs = move |bid: String| {
        let bid = bid.clone();
        mechs_loading.set(true);
        mechs_error.set(None);
        spawn(async move {
            match client_settings_list_mechanisms(&bid).await {
                Ok(json) => {
                    mechanisms.set(parse_mechanisms(&json));
                }
                Err(e) => {
                    tracing::warn!("BackendCard: list_mechanisms failed for {bid}: {e}");
                    mechs_error.set(Some(e));
                }
            }
            mechs_loading.set(false);
        });
    };

    rsx! {
        div {
            class: "client-settings-backend-card",
            "data-testid": "client-settings-backend-{backend_id_testid}-card",

            // Card header / collapse toggle
            button {
                class: if *expanded.read() {
                    "client-settings-card-header client-settings-card-header-expanded"
                } else {
                    "client-settings-card-header"
                },
                onclick: {
                    let bid = backend_id_expand.clone();
                    move |_| {
                        let next = !*expanded.read();
                        expanded.set(next);
                        if next && mechanisms.read().is_empty() && !*mechs_loading.read() {
                            load_mechs(bid.clone());
                        }
                    }
                },
                span { class: "client-settings-card-title", "{backend_id}" }
                span { class: "client-settings-card-chevron",
                    if *expanded.read() { "▾" } else { "▸" }
                }
            }

            if *expanded.read() {
                div { class: "client-settings-card-body",
                    // Version override editor
                    VersionOverrideEditor {
                        backend_id: backend_id.clone(),
                        current_version: effective_version.clone(),
                        current_override: version_override.clone(),
                        on_changed: {
                            let bid = backend_id_reload.clone();
                            move |_| {
                                // Reload mechanisms after override change (effective version may update).
                                load_mechs(bid.clone());
                            }
                        },
                    }

                    // Mechanisms section
                    div { class: "client-settings-mechanisms-section",
                        h4 { class: "client-settings-mechanisms-heading",
                            "{t(\"client-settings-mechanisms-heading\")}"
                        }

                        if *mechs_loading.read() {
                            div { class: "client-settings-loading", "…" }
                        } else if let Some(err) = mechs_error.read().clone() {
                            div { class: "client-settings-error", "{err}" }
                        } else if mechanisms.read().is_empty() {
                            div { class: "client-settings-empty",
                                "—"
                            }
                        } else {
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
                                                        // Reload mechanism list to reflect persisted state.
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
            }
        }
    }
}
