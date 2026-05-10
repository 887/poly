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
//! - `account_id: String` — selects the [`IsBackend`] via
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

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::state::BatchedSignal;
use crate::i18n::{has_key, t};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::toast::{push_toast, ToastMessage};
use dioxus::prelude::*;
use poly_client::{
    IsBackend, ClientError, SettingDescriptor, SettingKind, SettingsScope, SettingsSection,
    ToastTone,
};
use poly_ui_macros::{context_menu, ui_action};
use std::sync::Arc;
use tokio::sync::RwLock;

/// D15 — action fired when a user edits a plugin-declared setting.
///
/// Carries everything the async save needs (scope / scope-id / key / account).
/// Applied via the standard `UiAction::apply` pipeline; the actual plugin
/// round-trip happens via `spawn` because the host `IsBackend` is async.
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

/// P23 — resolve an optional FTL description key.
///
/// Returns `Some(resolved)` when the key is present in the bundle, `None`
/// when `t()` echoes back the raw key (indicating the plugin omitted the FTL
/// entry). Callers use this to omit the desc `<p>` entirely rather than
/// showing a raw kebab-case key in the UI.
#[must_use] 
pub fn lookup_optional_desc(key: &str) -> Option<String> {
    if has_key(key) { Some(t(key)) } else { None }
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
    // P48: unique id for aria-labelledby on the region.
    let header_id = format!("plugin-section-header-{section_key}");

    rsx! {
        // P48: settings section is a landmark region labelled by its heading.
        div {
            class: "settings-section plugin-section",
            role: "region",
            aria_labelledby: "{header_id}",
            // 1. Section header (icon + label).
            div { class: "plugin-section-header",
                if !icon.is_empty() {
                    span { class: "plugin-section-icon", "{icon}" }
                }
                h2 { id: "{header_id}", class: "plugin-section-title", "{header_label}" }
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
#[context_menu(allow_default)]
#[component]
fn PluginSettingField(
    field: SettingDescriptor,
    account_id: String,
    scope: SettingsScope,
    scope_id: String,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let field_key = field.key.clone();
    // Resolve both FTL keys through the host i18n store. `t()` returns the
    // raw key on miss so empty plugin bundles still render something.
    // P23: use lookup_optional_desc so the desc is omitted when the plugin
    // hasn't provided an FTL entry for it (t() would echo the key back).
    let label_key = t(&format!("setting-{}-label", field.key));
    let desc_opt = lookup_optional_desc(&format!("setting-{}-desc", field.key));

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
            render_widget(RenderArgs {
                field: field.clone(),
                label_key,
                desc_opt,
                current_json,
                account_id,
                scope,
                scope_id,
            })
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

/// Bundled arguments for the per-widget render helpers.
///
/// These args are shared by `render_toggle`, `render_text_input`,
/// `render_select`, `render_slider`. Bundling avoids `too_many_arguments`
/// and keeps the dispatch site (`render_widget`) terse.
#[derive(Clone)]
struct RenderArgs {
    field: SettingDescriptor,
    label_key: String,
    desc_opt: Option<String>,
    current_json: String,
    account_id: String,
    scope: SettingsScope,
    scope_id: String,
}

fn render_widget(args: RenderArgs) -> Element {
    match args.field.kind {
        SettingKind::Toggle => render_toggle(args),
        SettingKind::TextInput => render_text_input(args),
        SettingKind::Select => render_select(args),
        SettingKind::Slider => render_slider(args),
        SettingKind::InfoLabel => render_info_label(&args.label_key, args.desc_opt, &args.current_json),
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_toggle(args: RenderArgs) -> Element {
    let RenderArgs { field, label_key, desc_opt, current_json, account_id, scope, scope_id } = args;
    let checked = serde_json::from_str::<bool>(&current_json).unwrap_or(false);
    let checked_str = if checked { "true" } else { "false" };
    let key = field.key.clone();
    rsx! {
        div { class: "settings-toggle-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label_key}" }
                // P23: only render desc when the FTL key resolves to a real string.
                if let Some(desc) = desc_opt {
                    p { class: "settings-toggle-desc", "{desc}" }
                }
            }
            label { class: "toggle-switch",
                // P48: role=switch + aria-checked for screen readers.
                input {
                    r#type: "checkbox",
                    role: "switch",
                    aria_checked: "{checked_str}",
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_text_input(args: RenderArgs) -> Element {
    let RenderArgs { field, label_key, desc_opt, current_json, account_id, scope, scope_id } = args;
    let value = serde_json::from_str::<String>(&current_json).unwrap_or_default();
    let key = field.key.clone();
    rsx! {
        div { class: "settings-text-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label_key}" }
                // P23: only render desc when FTL key resolves to a real string.
                if let Some(desc) = desc_opt {
                    p { class: "settings-toggle-desc", "{desc}" }
                }
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_select(args: RenderArgs) -> Element {
    let RenderArgs { field, label_key, desc_opt, current_json, account_id, scope, scope_id } = args;
    let current = serde_json::from_str::<String>(&current_json).unwrap_or_default();
    let options: Vec<String> =
        serde_json::from_str::<Vec<String>>(&field.extra).unwrap_or_default();
    let key = field.key.clone();
    rsx! {
        div { class: "settings-select-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label_key}" }
                // P23: only render desc when FTL key resolves to a real string.
                if let Some(desc) = desc_opt {
                    p { class: "settings-toggle-desc", "{desc}" }
                }
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_slider(args: RenderArgs) -> Element {
    let RenderArgs { field, label_key, desc_opt, current_json, account_id, scope, scope_id } = args;
    let value = serde_json::from_str::<f64>(&current_json).unwrap_or(0.0_f64);
    // `extra` is a JSON object like {"min":0,"max":100,"step":1}
    let bounds: serde_json::Value =
        serde_json::from_str(&field.extra).unwrap_or_else(|_| serde_json::json!({}));
    let min = bounds.get("min").and_then(serde_json::Value::as_f64).unwrap_or(0.0_f64);
    let max = bounds.get("max").and_then(serde_json::Value::as_f64).unwrap_or(100.0_f64);
    let step = bounds.get("step").and_then(serde_json::Value::as_f64).unwrap_or(1.0_f64);
    let key = field.key.clone();
    rsx! {
        div { class: "settings-slider-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label_key}" }
                // P23: only render desc when FTL key resolves to a real string.
                if let Some(desc) = desc_opt {
                    p { class: "settings-toggle-desc", "{desc}" }
                }
            }
            input {
                class: "settings-slider",
                r#type: "range",
                // P48: slider ARIA attributes.
                role: "slider",
                aria_valuemin: "{min}",
                aria_valuemax: "{max}",
                aria_valuenow: "{value}",
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
                        let parsed: f64 = raw.parse().unwrap_or(0.0_f64);
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

fn render_info_label(label_key: &str, desc_opt: Option<String>, current_json: &str) -> Element {
    // InfoLabel is non-interactive. The stored JSON string is the body text
    // if any; otherwise fall back to the desc (if present).
    let body = serde_json::from_str::<String>(current_json)
        .ok()
        .or(desc_opt)
        .unwrap_or_default();
    rsx! {
        div { class: "settings-info-row",
            label { class: "settings-toggle-label", "{label_key}" }
            p { class: "settings-info-body", "{body}" }
        }
    }
}

/// Load the current JSON-encoded value for a single field via the plugin.
async fn load_value(
    client_manager: &BatchedSignal<ClientManager>,
    account_id: &str,
    scope: SettingsScope,
    scope_id: &str,
    key: &str,
) -> Result<String, ClientError> {
    let backend = resolve_backend(client_manager, account_id)?;
    let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
        Ok(g) => g,
        Err(_) => {
            tracing::warn!("settings_section: backend read timed out in load_value");
            return Err(ClientError::Internal("backend read timed out".into()));
        }
    };
    guard.get_setting_value(scope, scope_id, key).await
}

/// Save a JSON-encoded value via the plugin. Errors are logged; the caller
/// does not surface them inline (the next load will show the persisted state).
///
/// On success pushes a `ui-settings-saved` Success toast; on error pushes a
/// `ui-settings-save-failed` Error toast (Pack C.3 / P22).
async fn save_value(
    account_id: &str,
    scope: SettingsScope,
    scope_id: &str,
    key: &str,
    value_json: &str,
) {
    let client_manager: BatchedSignal<ClientManager> = match try_consume_context() {
        Some(cm) => cm,
        None => {
            tracing::warn!(
                "PluginSettingsSection: no ClientManager in context during save"
            );
            return;
        }
    };
    let toast_queue: Option<Signal<Vec<ToastMessage>>> = try_consume_context();
    let Ok(backend) = resolve_backend(&client_manager, account_id) else {
        tracing::warn!(
            "PluginSettingsSection: no backend for account {account_id} during save"
        );
        if let Some(q) = toast_queue {
            push_toast(
                q,
                ToastMessage::new("ui-settings-save-failed", ToastTone::Error),
            );
        }
        return;
    };
    let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
        Ok(g) => g,
        Err(_) => {
            tracing::warn!("settings_section: backend read timed out in save_value");
            return;
        }
    };
    match guard.set_setting_value(scope, scope_id, key, value_json).await {
        Ok(()) => {
            if let Some(q) = toast_queue {
                push_toast(q, ToastMessage::new("ui-settings-saved", ToastTone::Success));
            }
        }
        Err(err) => {
            tracing::warn!(
                "PluginSettingsSection: save {key}={value_json} failed: {err:?}"
            );
            if let Some(q) = toast_queue {
                push_toast(
                    q,
                    ToastMessage::new("ui-settings-save-failed", ToastTone::Error),
                );
            }
        }
    }
}

fn resolve_backend(
    client_manager: &BatchedSignal<ClientManager>,
    account_id: &str,
) -> Result<Arc<RwLock<Box<dyn IsBackend>>>, ClientError> {
    client_manager
        .read()
        .get_backend(account_id)
        .ok_or_else(|| ClientError::NotFound(format!("no backend for account {account_id}")))
}

/// Pure helper for Pack C.3 / P22 toast-mutation tests.
///
/// Mirrors the branch in [`save_value`] that pushes a toast onto the queue
/// after the plugin responds. Kept as a free function operating on the
/// underlying `Vec<ToastMessage>` so unit tests don't need a Dioxus
/// `VirtualDom` to construct a `Signal`.
#[cfg(test)]
fn push_save_outcome_toast(queue: &mut Vec<ToastMessage>, result: Result<(), ClientError>) {
    match result {
        Ok(()) => queue.push(ToastMessage::new("ui-settings-saved", ToastTone::Success)),
        Err(_) => queue.push(ToastMessage::new(
            "ui-settings-save-failed",
            ToastTone::Error,
        )),
    }
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::{lookup_optional_desc, push_save_outcome_toast};
    use crate::ui::client_ui::toast::ToastMessage;
    use poly_client::{ClientError, ToastTone};

    /// P23: when the FTL key is missing, lookup_optional_desc returns None.
    #[test]
    fn optional_desc_missing_returns_none() {
        let result = lookup_optional_desc("__nonexistent-key-for-test-desc__");
        assert!(result.is_none(), "missing FTL key should yield None");
    }

    /// P23: a key equal to its own resolved value means no FTL entry → None.
    #[test]
    fn optional_desc_echoed_key_returns_none() {
        let fake_key = "plugin-test-setting-noop-desc";
        let result = lookup_optional_desc(fake_key);
        assert!(result.is_none());
    }

    /// Pack C.3 / P22: an Ok save pushes exactly one Success toast with
    /// the `ui-settings-saved` label key.
    #[test]
    fn save_ok_pushes_success_toast() {
        let mut queue: Vec<ToastMessage> = Vec::new();
        push_save_outcome_toast(&mut queue, Ok(()));
        assert_eq!(queue.len(), 1, "Ok save should push exactly one toast");
        assert_eq!(queue[0].label_key, "ui-settings-saved");
        assert_eq!(queue[0].tone, ToastTone::Success);
    }

    /// Pack C.3 / P22: an Err save pushes exactly one Error toast with
    /// the `ui-settings-save-failed` label key.
    #[test]
    fn save_err_pushes_error_toast() {
        let mut queue: Vec<ToastMessage> = Vec::new();
        push_save_outcome_toast(
            &mut queue,
            Err(ClientError::NotFound("plugin unavailable".into())),
        );
        assert_eq!(queue.len(), 1, "Err save should push exactly one toast");
        assert_eq!(queue[0].label_key, "ui-settings-save-failed");
        assert_eq!(queue[0].tone, ToastTone::Error);
    }

    /// Pack C.3 / P22: repeated Ok saves grow the queue monotonically
    /// (one success toast per save).
    #[test]
    fn repeated_saves_grow_queue() {
        let mut queue: Vec<ToastMessage> = Vec::new();
        for _ in 0..3 {
            push_save_outcome_toast(&mut queue, Ok(()));
        }
        assert_eq!(queue.len(), 3);
        assert!(
            queue
                .iter()
                .all(|m| m.label_key == "ui-settings-saved"
                    && m.tone == ToastTone::Success)
        );
    }
}
