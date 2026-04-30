//! PersonaEditModal — full create/edit modal for a persona.
//!
//! Collapsible sections in order:
//!   Identity | Sources | Tools | Behaviour | Outbound | Memory | Audit
//!
//! Create: pass slug `"__new__"` — the form starts empty and calls
//!   `meta_persona_create` on save.  Edit: passes a real slug, loads
//!   the full persona via `meta_persona_get`, then saves via `meta_persona_update`.
//!
//! ## Reactive hygiene
//! - `use_reactive_effect` for the slug-keyed load so re-opening a different
//!   persona re-fires the load.
//! - All local signals are plain `Signal<T>` (single-component-scoped,
//!   no cross-component subscribers → no hang risk).
//! - No raw `Signal::write()` in `crates/core/src/ui/` scope — we use `.set()`
//!   which is safe for single-subscriber local signals.

use super::mcp::call_persona_mcp;
use super::sources_editor::PersonaSourcesEditor;
use super::tool_whitelist_editor::PersonaToolWhitelistEditor;
use super::types::{parse_persona_detail, AuditRow, PersonaDetail, PersonaFact};
use crate::i18n::t;
use crate::state::use_reactive_effect;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

// ─── Section collapse state ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Identity,
    Sources,
    Tools,
    Behaviour,
    Outbound,
    Memory,
    Audit,
}

// ─── Identity section ─────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn IdentitySection(
    name: Signal<String>,
    slug_display: String,
    avatar_emoji: Signal<String>,
    system_prompt: Signal<String>,
    style_notes: Signal<String>,
    enabled: Signal<bool>,
    is_new: bool,
) -> Element {
    rsx! {
        div { class: "persona-modal-section",
            // Name
            div { class: "settings-field",
                label { class: "settings-label", {t("persona-field-name")} }
                input {
                    r#type: "text",
                    class: "settings-input",
                    value: "{name.read()}",
                    oninput: move |e| name.set(e.value()),
                }
            }
            // Slug (read-only after creation)
            div { class: "settings-field",
                label { class: "settings-label", {t("persona-field-slug")} }
                input {
                    r#type: "text",
                    class: "settings-input",
                    value: "{slug_display}",
                    disabled: !is_new,
                    readonly: !is_new,
                }
            }
            // Avatar emoji
            div { class: "settings-field",
                label { class: "settings-label", {t("persona-field-avatar")} }
                input {
                    r#type: "text",
                    class: "settings-input persona-avatar-input",
                    value: "{avatar_emoji.read()}",
                    maxlength: "4",
                    oninput: move |e| avatar_emoji.set(e.value()),
                }
            }
            // Enabled toggle
            div { class: "settings-toggle-row",
                label { class: "settings-toggle-label", {t("persona-field-enabled")} }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *enabled.read(),
                        onchange: move |e| enabled.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
            // System prompt
            div { class: "settings-field",
                label { class: "settings-label", {t("persona-field-system-prompt")} }
                textarea {
                    class: "settings-textarea",
                    rows: "5",
                    value: "{system_prompt.read()}",
                    oninput: move |e| system_prompt.set(e.value()),
                }
            }
            // Style notes
            div { class: "settings-field",
                label { class: "settings-label", {t("persona-field-style-notes")} }
                textarea {
                    class: "settings-textarea settings-textarea-sm",
                    rows: "2",
                    value: "{style_notes.read()}",
                    oninput: move |e| style_notes.set(e.value()),
                }
            }
        }
    }
}

// ─── Behaviour section (read-only stub for Phase D) ──────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn BehaviourSection(detail: Option<PersonaDetail>) -> Element {
    let interval = detail
        .as_ref()
        .and_then(|d| d.heartbeat_interval_secs)
        .map(|s| format!("{s}s"))
        .unwrap_or_else(|| "Off".to_string());
    let proactivity = detail
        .as_ref()
        .map(|d| d.proactivity.clone())
        .unwrap_or_else(|| "drafts-only".to_string());
    let rate = detail.as_ref().map(|d| d.rate_limit_per_hour).unwrap_or(4);

    rsx! {
        div { class: "persona-modal-section",
            p { class: "persona-phase-note", {t("persona-behaviour-phase-f-note")} }
            div { class: "settings-field",
                label { class: "settings-label", {t("persona-field-heartbeat")} }
                input {
                    r#type: "text",
                    class: "settings-input",
                    value: "{interval}",
                    readonly: true,
                    disabled: true,
                }
            }
            div { class: "settings-field",
                label { class: "settings-label", {t("persona-field-proactivity")} }
                input {
                    r#type: "text",
                    class: "settings-input",
                    value: "{proactivity}",
                    readonly: true,
                    disabled: true,
                }
            }
            div { class: "settings-field",
                label { class: "settings-label", {t("persona-field-rate-limit")} }
                input {
                    r#type: "text",
                    class: "settings-input",
                    value: "{rate}/hr",
                    readonly: true,
                    disabled: true,
                }
            }
        }
    }
}

// ─── Memory section ───────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn MemorySection(facts: Vec<PersonaFact>) -> Element {
    rsx! {
        div { class: "persona-modal-section",
            if facts.is_empty() {
                div { class: "agent-panel-empty-state", {t("persona-memory-empty")} }
            } else {
                div { class: "persona-fact-list",
                    for fact in &facts {
                        div { class: "persona-fact-row", key: "{fact.id}",
                            span { class: "persona-fact-text", "{fact.fact_text}" }
                            if fact.pinned {
                                span { class: "persona-fact-pinned", "📌" }
                            }
                            if let Some(cat) = &fact.category {
                                span { class: "persona-fact-category", "{cat}" }
                            }
                        }
                    }
                }
            }
            p { class: "persona-phase-note", {t("persona-memory-phase-h-note")} }
        }
    }
}

// ─── Audit section ────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AuditSection(rows: Vec<AuditRow>) -> Element {
    rsx! {
        div { class: "persona-modal-section",
            if rows.is_empty() {
                div { class: "agent-panel-empty-state", {t("persona-audit-empty")} }
            } else {
                div { class: "persona-audit-list",
                    for row in &rows {
                        div { class: "persona-audit-row", key: "{row.id}",
                            span { class: "persona-audit-time", "{row.occurred_at}" }
                            span { class: "persona-audit-actor", "{row.actor}" }
                            span { class: "persona-audit-action", "{row.action}" }
                            span {
                                class: if row.result == "ok" { "persona-audit-result ok" } else { "persona-audit-result err" },
                                "{row.result}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── CollapsibleSection ───────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn CollapsibleSection(
    label: String,
    open: bool,
    on_toggle: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        div { class: if open { "persona-collapsible open" } else { "persona-collapsible" },
            button {
                class: "persona-collapsible-header",
                onclick: move |_| on_toggle.call(()),
                span { class: "persona-collapsible-arrow", if open { "▼" } else { "▶" } }
                span { class: "persona-collapsible-label", "{label}" }
            }
            if open {
                div { class: "persona-collapsible-body",
                    {children}
                }
            }
        }
    }
}

// ─── PersonaEditModal ─────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct PersonaEditModalProps {
    /// Pass `"__new__"` for create mode; existing slug for edit mode.
    pub slug: String,
    pub on_close: EventHandler<()>,
    pub on_saved: EventHandler<()>,
}

/// Full create/edit modal.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonaEditModal(props: PersonaEditModalProps) -> Element {
    let slug = props.slug.clone();
    let is_new = slug == "__new__";

    // Form fields
    let mut field_name = use_signal(String::new);
    let mut field_avatar = use_signal(|| "🤖".to_string());
    let mut field_prompt = use_signal(String::new);
    let mut field_notes = use_signal(String::new);
    let mut field_enabled = use_signal(|| true);

    // Loaded detail (None until loaded)
    let mut detail: Signal<Option<PersonaDetail>> = use_signal(|| None);
    let mut loading = use_signal(|| !is_new);
    let mut save_error: Signal<Option<String>> = use_signal(|| None);
    let mut saving = use_signal(|| false);

    // Open/closed state for each section
    let mut open_identity = use_signal(|| true);
    let mut open_sources = use_signal(|| false);
    let mut open_tools = use_signal(|| false);
    let mut open_behaviour = use_signal(|| false);
    let mut open_outbound = use_signal(|| false);
    let mut open_memory = use_signal(|| false);
    let mut open_audit = use_signal(|| false);

    // Load existing persona on mount (edit mode).
    // use_reactive_effect re-fires if slug changes (class #6 countermeasure).
    let slug_dep = slug.clone();
    use_reactive_effect(slug_dep, move |slug_load| {
        spawn(async move {
            if slug_load != "__new__" {
                match call_persona_mcp("meta_persona_get", serde_json::json!({ "slug": slug_load })).await {
                    Ok(json) => {
                        if let Some(d) = parse_persona_detail(&json) {
                            field_name.set(d.name.clone());
                            field_avatar.set(d.avatar_emoji.clone());
                            field_prompt.set(d.system_prompt.clone());
                            field_notes.set(d.style_notes.clone().unwrap_or_default());
                            field_enabled.set(d.enabled);
                            detail.set(Some(d));
                        }
                    }
                    Err(e) => tracing::warn!("PersonaEditModal load failed: {e}"),
                }
            }
            loading.set(false);
        });
    });

    let on_close = props.on_close.clone();
    let on_saved = props.on_saved.clone();
    let slug_save = slug.clone();

    // Snapshot for sub-editors (use peek — just data, no subscription needed here).
    let sources = detail.peek().as_ref().map(|d| d.sources.clone()).unwrap_or_default();
    let tools = detail.peek().as_ref().map(|d| d.tool_whitelist.clone()).unwrap_or_default();
    let facts = detail.peek().as_ref().map(|d| d.pinned_facts.clone()).unwrap_or_default();
    let audit = detail.peek().as_ref().map(|d| d.recent_audit.clone()).unwrap_or_default();

    rsx! {
        div { class: "persona-modal-overlay",
            onclick: move |_| on_close.call(()),
            div { class: "persona-modal",
                // Prevent click-through
                onclick: move |evt| evt.stop_propagation(),

                // Header
                div { class: "persona-modal-header",
                    h3 { class: "persona-modal-title",
                        if is_new { {t("persona-modal-title-create")} }
                        else { {t("persona-modal-title-edit")} }
                    }
                    button {
                        class: "persona-modal-close btn btn-icon",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }

                if *loading.read() {
                    div { class: "persona-modal-loading", {t("persona-loading")} }
                } else {
                    div { class: "persona-modal-body",
                        // Identity
                        CollapsibleSection {
                            label: t("persona-section-identity").to_string(),
                            open: *open_identity.read(),
                            on_toggle: move |_| {
                                let v = *open_identity.read();
                                open_identity.set(!v);
                            },
                            IdentitySection {
                                name: field_name,
                                slug_display: if is_new { String::new() } else { slug.clone() },
                                avatar_emoji: field_avatar,
                                system_prompt: field_prompt,
                                style_notes: field_notes,
                                enabled: field_enabled,
                                is_new,
                            }
                        }

                        // Sources
                        CollapsibleSection {
                            label: t("persona-section-sources").to_string(),
                            open: *open_sources.read(),
                            on_toggle: move |_| {
                                let v = *open_sources.read();
                                open_sources.set(!v);
                            },
                            PersonaSourcesEditor {
                                persona_slug: slug.clone(),
                                existing_sources: sources.clone(),
                                account_ids: vec![],
                                on_saved: move |_| tracing::info!("sources saved"),
                            }
                        }

                        // Tools
                        CollapsibleSection {
                            label: t("persona-section-tools").to_string(),
                            open: *open_tools.read(),
                            on_toggle: move |_| {
                                let v = *open_tools.read();
                                open_tools.set(!v);
                            },
                            PersonaToolWhitelistEditor {
                                persona_slug: slug.clone(),
                                existing_whitelist: tools.clone(),
                                on_saved: move |_| tracing::info!("tools saved"),
                            }
                        }

                        // Behaviour (Phase F — read-only stub)
                        CollapsibleSection {
                            label: t("persona-section-behaviour").to_string(),
                            open: *open_behaviour.read(),
                            on_toggle: move |_| {
                                let v = *open_behaviour.read();
                                open_behaviour.set(!v);
                            },
                            BehaviourSection {
                                detail: detail.peek().clone(),
                            }
                        }

                        // Outbound (Phase F — stub)
                        CollapsibleSection {
                            label: t("persona-section-outbound").to_string(),
                            open: *open_outbound.read(),
                            on_toggle: move |_| {
                                let v = *open_outbound.read();
                                open_outbound.set(!v);
                            },
                            div { class: "persona-modal-section",
                                p { class: "persona-phase-note", {t("persona-outbound-phase-f-note")} }
                            }
                        }

                        // Memory (read-only list, Phase H adds delete buttons)
                        CollapsibleSection {
                            label: t("persona-section-memory").to_string(),
                            open: *open_memory.read(),
                            on_toggle: move |_| {
                                let v = *open_memory.read();
                                open_memory.set(!v);
                            },
                            MemorySection { facts: facts.clone() }
                        }

                        // Audit (collapsed by default)
                        CollapsibleSection {
                            label: t("persona-section-audit").to_string(),
                            open: *open_audit.read(),
                            on_toggle: move |_| {
                                let v = *open_audit.read();
                                open_audit.set(!v);
                            },
                            AuditSection { rows: audit.clone() }
                        }
                    }

                    // Footer: Save / Cancel
                    div { class: "persona-modal-footer",
                        if let Some(err) = save_error.read().clone() {
                            span { class: "persona-save-error", "{err}" }
                        }
                        button {
                            class: "btn btn-secondary",
                            onclick: move |_| on_close.call(()),
                            {t("persona-action-cancel")}
                        }
                        button {
                            class: "btn btn-primary",
                            disabled: *saving.read(),
                            onclick: {
                                let slug_s = slug_save.clone();
                                move |_| {
                                    let name = field_name.read().clone();
                                    let avatar = field_avatar.read().clone();
                                    let prompt = field_prompt.read().clone();
                                    let notes = field_notes.read().clone();
                                    let enabled = *field_enabled.read();
                                    let slug_inner = slug_s.clone();
                                    let on_saved = on_saved.clone();
                                    saving.set(true);
                                    save_error.set(None);
                                    spawn(async move {
                                        let result = if slug_inner == "__new__" {
                                            // auto-derive slug from name
                                            let derived = name
                                                .to_lowercase()
                                                .replace(' ', "-")
                                                .chars()
                                                .filter(|c| c.is_alphanumeric() || *c == '-')
                                                .collect::<String>();
                                            call_persona_mcp("meta_persona_create", serde_json::json!({
                                                "slug": derived,
                                                "name": name,
                                                "avatar_emoji": avatar,
                                                "system_prompt": prompt,
                                                "style_notes": if notes.is_empty() { serde_json::Value::Null } else { serde_json::json!(notes) },
                                                "enabled": enabled,
                                            })).await
                                        } else {
                                            call_persona_mcp("meta_persona_update", serde_json::json!({
                                                "slug": slug_inner,
                                                "name": name,
                                                "avatar_emoji": avatar,
                                                "system_prompt": prompt,
                                                "style_notes": if notes.is_empty() { serde_json::Value::Null } else { serde_json::json!(notes) },
                                                "enabled": enabled,
                                            })).await
                                        };
                                        match result {
                                            Ok(_) => on_saved.call(()),
                                            Err(e) => {
                                                tracing::warn!("persona save failed: {e}");
                                                save_error.set(Some(e));
                                            }
                                        }
                                        saving.set(false);
                                    });
                                }
                            },
                            if *saving.read() { {t("persona-saving")} } else { {t("persona-action-save")} }
                        }
                    }
                }
            }
        }
    }
}
