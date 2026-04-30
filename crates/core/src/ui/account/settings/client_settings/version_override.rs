//! `VersionOverrideEditor` — toggle + optional text input for per-backend
//! version override.
//!
//! Reactive hygiene:
//! - Draft input text uses `use_signal` (local, single-component).
//! - All writes to local signals use `.set()` — no `Signal::write()`.
//! - `set_if_changed` not needed here (no self-write effect).
//! - No raw `use_effect` with non-Signal captures.

use super::mcp::{client_settings_get_version, client_settings_set_version_override};
use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Version override editor: shows the effective version and optionally
/// a text field + Save / Clear buttons when the user enables the override toggle.
///
/// Props:
/// - `backend_id`: used in `data-testid` attrs and MCP calls.
/// - `current_version`: effective version string (from MCP snapshot).
/// - `current_override`: active override string, or `None` if using the default.
/// - `on_changed`: fired after a successful set/clear so the parent can reload.
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
    // Override toggle: on when an override is currently set.
    let mut override_on = use_signal(|| current_override.is_some());
    // Draft text in the input — pre-filled with active override or effective version.
    let initial_text = current_override
        .clone()
        .unwrap_or_else(|| current_version.clone());
    let mut draft = use_signal(move || initial_text);
    let mut saving = use_signal(|| false);
    let mut save_error: Signal<Option<String>> = use_signal(|| None);

    let backend_id_save = backend_id.clone();
    let backend_id_clear = backend_id.clone();
    let backend_id_get = backend_id.clone();

    rsx! {
        div { class: "client-settings-version-editor",
            div {
                class: "client-settings-version-effective",
                "data-testid": "client-settings-backend-{backend_id}-version-effective",
                span { class: "label", "{t(\"client-settings-effective-version\")}: " }
                span { class: "value version-string", "{current_version}" }
            }

            div { class: "client-settings-version-toggle-row",
                label { class: "client-settings-override-toggle-label",
                    input {
                        r#type: "checkbox",
                        "data-testid": "client-settings-backend-{backend_id}-version-override-toggle",
                        checked: *override_on.read(),
                        onchange: move |e| {
                            override_on.set(e.checked());
                            if e.checked() {
                                // Refresh the effective version when enabling.
                                let bid = backend_id_get.clone();
                                spawn(async move {
                                    if let Ok(json) = client_settings_get_version(&bid).await {
                                        if let Some(v) = json
                                            .get("effective_version")
                                            .and_then(|v| v.as_str())
                                        {
                                            draft.set(v.to_string());
                                        }
                                    }
                                });
                            }
                        },
                    }
                    "{t(\"client-settings-override-toggle\")}"
                }
            }

            if *override_on.read() {
                div { class: "client-settings-version-input-row",
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
                            let bid = backend_id_save.clone();
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
                    button {
                        class: "btn btn-sm btn-secondary client-settings-clear-btn",
                        "data-testid": "client-settings-backend-{backend_id}-version-override-clear",
                        disabled: *saving.read(),
                        onclick: {
                            let bid = backend_id_clear.clone();
                            move |_| {
                                let bid = bid.clone();
                                saving.set(true);
                                save_error.set(None);
                                spawn(async move {
                                    match client_settings_set_version_override(&bid, None).await {
                                        Ok(_) => {
                                            override_on.set(false);
                                            on_changed.call(());
                                        }
                                        Err(e) => save_error.set(Some(e)),
                                    }
                                    saving.set(false);
                                });
                            }
                        },
                        "{t(\"client-settings-override-clear\")}"
                    }
                }
                if let Some(err) = save_error.read().clone() {
                    div { class: "client-settings-save-error", "{err}" }
                }
            }
        }
    }
}
