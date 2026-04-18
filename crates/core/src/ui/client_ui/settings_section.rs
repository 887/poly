//! Host component that renders a plugin-declared settings section.
//!
//! Consumes a [`SettingsSection`] — plugin-declared, scoped (account-global /
//! per-server / per-channel / per-user) — and renders each
//! [`SettingDescriptor`] via a host-owned widget. Reads/writes the current
//! value through the plugin's `get_setting_value` / `set_setting_value` (D15
//! — storage owned by the plugin).
//!
//! ## Prop shape (WP 3.A)
//!
//! - `section: SettingsSection` — the declared section (scope/key/icon/fields).
//! - `account_id: String` — selects the [`ClientBackend`] via
//!   [`crate::client_manager::ClientManager::get_backend`].
//! - `scope_id: String` — `""` for `AccountGlobal`; server-id / channel-id /
//!   user-id for the other scopes.
//!
//! ## Error handling (D22 / D26)
//!
//! If the plugin errors on load or save, the field renders an inline error row
//! instead of crashing. Errors are also logged via `tracing::warn!`. Other
//! fields in the same section continue to work.
//!
//! ## Widget styling
//!
//! Widgets reuse existing settings CSS classes from
//! [`crate::ui::settings::plugin_settings`] (`settings-section`,
//! `plugin-section-header`, `settings-toggle-row`, `toggle-switch`, …) so the
//! plugin-declared panels match the host-owned panels visually.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_client::{
    ClientBackend, ClientError, SettingDescriptor, SettingKind, SettingsScope, SettingsSection,
};
use poly_ui_macros::{context_menu, ui_action};
use std::sync::Arc;
use tokio::sync::RwLock;

/// D15 — action fired when a user edits a plugin-declared setting.
///
/// Carries everything the async save needs (scope / scope-id / key / account).
/// Applied via the standard `UiAction::apply` pipeline; the actual plugin
/// round-trip happens via `spawn` because the host `ClientBackend` is async.
#[derive(Debug, Clone)]
pub enum PluginSettingFieldAction {
    /// Persist a new JSON-encoded value for a plugin setting.
    SetValue {
        account_id: String,
        scope: SettingsScope,
        scope_id: String,
        key: String,
        value_json: String,
    },
}

impl UiAction for PluginSettingFieldAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetValue {
                account_id,
                scope,
                scope_id,
                key,
                value_json,
            } => {
                // Fire-and-forget async save via the plugin backend.
                // Errors are logged inside `save_value`.
                dioxus::core::spawn_forever(async move {
                    save_value(&account_id, scope, &scope_id, &key, &value_json).await;
                });
            }
        }
    }
}

/// Render a plugin-declared settings section.
///
/// See the module docs for the prop contract. One `use_resource` per field
/// fetches the current value at mount; edits call back into the plugin via
/// `set_setting_value`.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn PluginSettingsSection(
    section: SettingsSection,
    account_id: String,
    scope_id: String,
) -> Element {
    let icon = section.icon.clone().unwrap_or_default();
    let section_key = section.section_key.clone();
    // FTL keys for plugin settings follow `setting-<section>-label` /
    // `setting-<field>-label` / `-desc`. Plugin FTL bundles are merged into
    // the host i18n store at plugin-init time, so `t()` resolves them
    // (returning the raw key on miss).
    let header_label = t(&format!("setting-{section_key}-label"));
    let has_info_block = section.info_block.is_some();

    let fields = section.fields.clone();

    rsx! {
        div { class: "settings-section plugin-section",
            // 1. Section header (icon + label).
            div { class: "plugin-section-header",
                if !icon.is_empty() {
                    span { class: "plugin-section-icon", "{icon}" }
                }
                h2 { class: "plugin-section-title", "{header_label}" }
            }
            // 2. Optional info-block — stub until WP 5 ships CustomBlock.
            if has_info_block {
                div { class: "plugin-section-info-stub",
                    "[custom-block pending WP 5]"
                }
            }
            // 3. One row per declared field.
            {fields.into_iter().map(|field| {
                let key = format!("{}-{}", section.scope_key(), field.key);
                rsx! {
                    PluginSettingField {
                        key: "{key}",
                        field: field,
                        account_id: account_id.clone(),
                        scope: section.scope,
                        scope_id: scope_id.clone(),
                    }
                }
            })}
        }
    }
}

/// String form of [`SettingsScope`] — used as the FTL prefix chunk and as
/// the unique key part when composing keys across scopes in one page.
trait ScopeKeyExt {
    fn scope_key(&self) -> &'static str;
}

impl ScopeKeyExt for SettingsSection {
    fn scope_key(&self) -> &'static str {
        match self.scope {
            SettingsScope::AccountGlobal => "account-global",
            SettingsScope::PerServer => "per-server",
            SettingsScope::PerChannel => "per-channel",
            SettingsScope::PerUser => "per-user",
        }
    }
}

/// One field inside a [`PluginSettingsSection`].
///
/// Split out so each field can own its own `use_resource` for the current
/// value, and so widget dispatch stays local to the row.
#[ui_action(PluginSettingFieldAction)]
#[context_menu(inherit)]
#[component]
fn PluginSettingField(
    field: SettingDescriptor,
    account_id: String,
    scope: SettingsScope,
    scope_id: String,
) -> Element {
    let client_manager: Signal<ClientManager> = use_context();

    let field_key = field.key.clone();
    // Resolve both FTL keys through the host i18n store. `t()` returns the
    // raw key on miss so empty plugin bundles still render something.
    let label_key = t(&format!("setting-{}-label", field.key));
    let desc_key = t(&format!("setting-{}-desc", field.key));

    // Fetch current value from the plugin on mount.
    let value_res = {
        let account_id = account_id.clone();
        let scope_id = scope_id.clone();
        let field_key = field_key.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            let scope_id = scope_id.clone();
            let field_key = field_key.clone();
            async move { load_value(&client_manager, &account_id, scope, &scope_id, &field_key).await }
        })
    };

    match &*value_res.read_unchecked() {
        None => rsx! {
            div { class: "settings-toggle-row disabled",
                div { class: "settings-toggle-label-group",
                    label { class: "settings-toggle-label", "{label_key}" }
                    p { class: "settings-toggle-desc", "loading…" }
                }
            }
        },
        Some(Err(err)) => {
            tracing::warn!(
                "PluginSettingsSection: load {field_key} failed: {err:?}"
            );
            render_error_row(&label_key, "plugin error: failed to load setting")
        }
        Some(Ok(current_json)) => {
            let current_json = current_json.clone();
            render_widget(
                field.clone(),
                label_key,
                desc_key,
                current_json,
                account_id,
                scope,
                scope_id,
            )
        }
    }
}

fn render_error_row(label: &str, msg: &str) -> Element {
    rsx! {
        div { class: "settings-toggle-row plugin-setting-error",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label}" }
                p { class: "settings-toggle-desc", "{msg}" }
            }
        }
    }
}

fn render_widget(
    field: SettingDescriptor,
    label_key: String,
    desc_key: String,
    current_json: String,
    account_id: String,
    scope: SettingsScope,
    scope_id: String,
) -> Element {
    match field.kind {
        SettingKind::Toggle => render_toggle(
            field, label_key, desc_key, current_json, account_id, scope, scope_id,
        ),
        SettingKind::TextInput => render_text_input(
            field, label_key, desc_key, current_json, account_id, scope, scope_id,
        ),
        SettingKind::Select => render_select(
            field, label_key, desc_key, current_json, account_id, scope, scope_id,
        ),
        SettingKind::Slider => render_slider(
            field, label_key, desc_key, current_json, account_id, scope, scope_id,
        ),
        SettingKind::InfoLabel => render_info_label(label_key, desc_key, current_json),
    }
}

fn render_toggle(
    field: SettingDescriptor,
    label_key: String,
    desc_key: String,
    current_json: String,
    account_id: String,
    scope: SettingsScope,
    scope_id: String,
) -> Element {
    let checked = serde_json::from_str::<bool>(&current_json).unwrap_or(false);
    let key = field.key.clone();
    rsx! {
        div { class: "settings-toggle-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label_key}" }
                p { class: "settings-toggle-desc", "{desc_key}" }
            }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked,
                    onchange: move |evt| {
                        let new_val = evt.checked();
                        let account_id = account_id.clone();
                        let scope_id = scope_id.clone();
                        let key = key.clone();
                        spawn(async move {
                            let json = serde_json::to_string(&new_val).unwrap_or_else(|_| "false".to_string());
                            save_value(&account_id, scope, &scope_id, &key, &json).await;
                        });
                    },
                }
                span { class: "toggle-slider" }
            }
        }
    }
}

fn render_text_input(
    field: SettingDescriptor,
    label_key: String,
    desc_key: String,
    current_json: String,
    account_id: String,
    scope: SettingsScope,
    scope_id: String,
) -> Element {
    let value = serde_json::from_str::<String>(&current_json).unwrap_or_default();
    let key = field.key.clone();
    rsx! {
        div { class: "settings-text-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label_key}" }
                p { class: "settings-toggle-desc", "{desc_key}" }
            }
            input {
                class: "settings-text-input",
                r#type: "text",
                value,
                onchange: move |evt| {
                    let new_val = evt.value();
                    let account_id = account_id.clone();
                    let scope_id = scope_id.clone();
                    let key = key.clone();
                    spawn(async move {
                        let json = serde_json::to_string(&new_val)
                            .unwrap_or_else(|_| "\"\"".to_string());
                        save_value(&account_id, scope, &scope_id, &key, &json).await;
                    });
                },
            }
        }
    }
}

fn render_select(
    field: SettingDescriptor,
    label_key: String,
    desc_key: String,
    current_json: String,
    account_id: String,
    scope: SettingsScope,
    scope_id: String,
) -> Element {
    let current = serde_json::from_str::<String>(&current_json).unwrap_or_default();
    let options: Vec<String> =
        serde_json::from_str::<Vec<String>>(&field.extra).unwrap_or_default();
    let key = field.key.clone();
    rsx! {
        div { class: "settings-select-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label_key}" }
                p { class: "settings-toggle-desc", "{desc_key}" }
            }
            select {
                class: "settings-select",
                onchange: move |evt| {
                    let new_val = evt.value();
                    let account_id = account_id.clone();
                    let scope_id = scope_id.clone();
                    let key = key.clone();
                    spawn(async move {
                        let json = serde_json::to_string(&new_val)
                            .unwrap_or_else(|_| "\"\"".to_string());
                        save_value(&account_id, scope, &scope_id, &key, &json).await;
                    });
                },
                {options.into_iter().map(|opt| {
                    let selected = opt == current;
                    rsx! {
                        option {
                            value: "{opt}",
                            selected,
                            "{opt}"
                        }
                    }
                })}
            }
        }
    }
}

fn render_slider(
    field: SettingDescriptor,
    label_key: String,
    desc_key: String,
    current_json: String,
    account_id: String,
    scope: SettingsScope,
    scope_id: String,
) -> Element {
    let value = serde_json::from_str::<f64>(&current_json).unwrap_or(0.0);
    // `extra` is a JSON object like {"min":0,"max":100,"step":1}
    let bounds: serde_json::Value =
        serde_json::from_str(&field.extra).unwrap_or_else(|_| serde_json::json!({}));
    let min = bounds.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max = bounds.get("max").and_then(|v| v.as_f64()).unwrap_or(100.0);
    let step = bounds.get("step").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let key = field.key.clone();
    rsx! {
        div { class: "settings-slider-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label_key}" }
                p { class: "settings-toggle-desc", "{desc_key}" }
            }
            input {
                class: "settings-slider",
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                onchange: move |evt| {
                    let raw = evt.value();
                    let account_id = account_id.clone();
                    let scope_id = scope_id.clone();
                    let key = key.clone();
                    spawn(async move {
                        let parsed: f64 = raw.parse().unwrap_or(0.0);
                        let json = serde_json::to_string(&parsed)
                            .unwrap_or_else(|_| "0".to_string());
                        save_value(&account_id, scope, &scope_id, &key, &json).await;
                    });
                },
            }
            span { class: "settings-slider-value", "{value}" }
        }
    }
}

fn render_info_label(label_key: String, desc_key: String, current_json: String) -> Element {
    // InfoLabel is non-interactive. The stored JSON string is the body text
    // if any; otherwise fall back to the desc-key.
    let body = serde_json::from_str::<String>(&current_json).unwrap_or(desc_key);
    rsx! {
        div { class: "settings-info-row",
            label { class: "settings-toggle-label", "{label_key}" }
            p { class: "settings-info-body", "{body}" }
        }
    }
}

/// Load the current JSON-encoded value for a single field via the plugin.
async fn load_value(
    client_manager: &Signal<ClientManager>,
    account_id: &str,
    scope: SettingsScope,
    scope_id: &str,
    key: &str,
) -> Result<String, ClientError> {
    let backend = resolve_backend(client_manager, account_id)?;
    let guard = backend.read().await;
    guard.get_setting_value(scope, scope_id, key).await
}

/// Save a JSON-encoded value via the plugin. Errors are logged; the caller
/// does not surface them inline (the next load will show the persisted state).
async fn save_value(
    account_id: &str,
    scope: SettingsScope,
    scope_id: &str,
    key: &str,
    value_json: &str,
) {
    let client_manager: Signal<ClientManager> = match try_consume_context() {
        Some(cm) => cm,
        None => {
            tracing::warn!(
                "PluginSettingsSection: no ClientManager in context during save"
            );
            return;
        }
    };
    let Ok(backend) = resolve_backend(&client_manager, account_id) else {
        tracing::warn!(
            "PluginSettingsSection: no backend for account {account_id} during save"
        );
        return;
    };
    let guard = backend.read().await;
    if let Err(err) = guard.set_setting_value(scope, scope_id, key, value_json).await {
        tracing::warn!(
            "PluginSettingsSection: save {key}={value_json} failed: {err:?}"
        );
    }
}

fn resolve_backend(
    client_manager: &Signal<ClientManager>,
    account_id: &str,
) -> Result<Arc<RwLock<Box<dyn ClientBackend>>>, ClientError> {
    client_manager
        .read()
        .get_backend(account_id)
        .ok_or_else(|| ClientError::NotFound(format!("no backend for account {account_id}")))
}
