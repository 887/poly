//! `VersionOverrideEditor` — version override toggle styled to match the
//! polished plugin-section toggle rows (e.g. Poly Server's "Use WebSocket").
//!
//! Reactive hygiene:
//! - Local-only signals; `.set()` only.
//! - No raw `use_effect` with non-Signal captures.

use super::mcp::{client_settings_get_version, client_settings_set_version_override};
use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Version override toggle. Off → backend uses default version.
/// On → reveals an inline input + Save / Clear buttons.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
pub fn VersionOverrideEditor(
    backend_id: String,
    current_version: String,
    current_override: Option<String>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut override_on = use_signal(|| current_override.is_some());
    let initial_text = current_override
        .clone()
        .unwrap_or_else(|| current_version.clone());
    let mut draft = use_signal(move || initial_text);
    let mut saving = use_signal(|| false);
    let mut save_error: Signal<Option<String>> = use_signal(|| None);

    let bid_save  = backend_id.clone();
    let bid_clear = backend_id.clone();
    let bid_get   = backend_id.clone();

    let toggle_label = t("client-settings-override-toggle");
    let desc = format!(
        "{}: {}",
        t("client-settings-effective-version"),
        current_version
    );

    rsx! {
        div { class: "settings-toggle-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{toggle_label}" }
                p { class: "settings-toggle-desc", "{desc}" }
            }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    "data-testid": "client-settings-backend-{backend_id}-version-override-toggle",
                    checked: *override_on.read(),
                    onchange: move |e| {
                        override_on.set(e.checked());
                        if e.checked() {
                            let bid = bid_get.clone();
                            spawn(async move {
                                if let Ok(json) = client_settings_get_version(&bid).await
                                    && let Some(v) = json
                                        .get("effective_version")
                                        .and_then(|v| v.as_str())
                                    {
                                        draft.set(v.to_string());
                                    }
                            });
                        } else {
                            // Clear the override when toggling off.
                            let bid = bid_clear.clone();
                            saving.set(true);
                            save_error.set(None);
                            spawn(async move {
                                match client_settings_set_version_override(&bid, None).await {
                                    Ok(_) => on_changed.call(()),
                                    Err(e) => save_error.set(Some(e)),
                                }
                                saving.set(false);
                            });
                        }
                    },
                }
                span { class: "toggle-slider" }
            }
        }

        if *override_on.read() {
            div { class: "settings-toggle-row client-settings-version-input-row",
                input {
                    r#type: "text",
                    class: "client-settings-version-input",
                    "data-testid": "client-settings-backend-{backend_id}-version-override-input",
                    value: "{draft.read()}",
                    oninput: move |e| draft.set(e.value()),
                    placeholder: "e.g. 9.9.9",
                }
                button {
                    class: "btn btn-sm btn-primary client-settings-save-btn",
                    "data-testid": "client-settings-backend-{backend_id}-version-override-save",
                    disabled: *saving.read(),
                    onclick: {
                        let bid = bid_save.clone();
                        move |_| {
                            let bid = bid.clone();
                            let val = draft.read().clone();
                            saving.set(true);
                            save_error.set(None);
                            spawn(async move {
                                match client_settings_set_version_override(&bid, Some(&val)).await {
                                    Ok(_) => on_changed.call(()),
                                    Err(e) => save_error.set(Some(e)),
                                }
                                saving.set(false);
                            });
                        }
                    },
                    if *saving.read() { "…" } else { "{t(\"client-settings-override-save\")}" }
                }
            }
            if let Some(err) = save_error.read().clone() {
                div { class: "settings-toggle-row client-settings-save-error", "{err}" }
            }
        }
    }
}
