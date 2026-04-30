//! PersonaListPanel — compact list of all personas shown inside AgentPanel.
//!
//! Each row: avatar emoji, name, enabled/disabled status dot, "Talk to" button,
//! and a gear icon that opens PersonaEditModal.
//!
//! Phase E (E.4): "Talk to" button fires `on_talk(PersonaSummary)` instead of
//! the Phase D `tracing::info!` stub.
//!
//! Reactive hygiene:
//! - `use_future` for one-shot load on mount (no stale-capture risk; no deps).
//! - `Signal<Vec<PersonaSummary>>` — all writes via `.set(…)` (single-component local).
//! - No `Signal::write()` or raw `use_effect` with non-Signal captures.

use super::mcp::call_persona_mcp;
use super::types::{parse_persona_list, PersonaSummary};
use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

// ─── PersonaStatusDot ────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn PersonaStatusDot(enabled: bool) -> Element {
    let (class, title) = if enabled {
        ("persona-status-dot persona-status-enabled", t("persona-status-enabled"))
    } else {
        ("persona-status-dot persona-status-paused", t("persona-status-paused"))
    };
    rsx! {
        span { class, title: "{title}" }
    }
}

// ─── PersonaListRow ──────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn PersonaListRow(
    persona: PersonaSummary,
    on_edit: EventHandler<String>,
    /// E.4: fires with the selected PersonaSummary to open the talk overlay.
    on_talk: EventHandler<PersonaSummary>,
) -> Element {
    let slug = persona.slug.clone();
    let persona_talk = persona.clone();
    rsx! {
        div { class: "persona-list-row",
            span { class: "persona-avatar", "{persona.avatar_emoji}" }
            div { class: "persona-row-info",
                span { class: "persona-row-name", "{persona.name}" }
                PersonaStatusDot { enabled: persona.enabled }
            }
            div { class: "persona-row-actions",
                button {
                    class: "btn btn-sm btn-secondary persona-talk-btn",
                    title: t("persona-action-talk-to"),
                    onclick: move |_| on_talk.call(persona_talk.clone()),
                    {t("persona-action-talk-to")}
                }
                button {
                    class: "btn btn-sm btn-icon persona-edit-btn",
                    title: t("persona-action-edit"),
                    onclick: move |_| on_edit.call(slug.clone()),
                    "⚙"
                }
            }
        }
    }
}

// ─── PersonaListPanel ────────────────────────────────────────────────────────

/// Compact list of personas — mounts inside AgentPanel between Drafts and Style.
///
/// Props:
/// - `on_talk`: called when the user clicks "Talk to" on a persona row.
///              The caller should set a `Signal<Option<TalkSession>>` to open
///              the overlay (Phase E wire-up).
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonaListPanel(
    /// E.4: callback fired with the chosen PersonaSummary.
    on_talk: EventHandler<PersonaSummary>,
) -> Element {
    let mut personas: Signal<Vec<PersonaSummary>> = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut edit_slug: Signal<Option<String>> = use_signal(|| None);

    // One-shot load on mount.
    use_future(move || async move {
        match call_persona_mcp("meta_persona_list", serde_json::json!({})).await {
            Ok(json) => {
                let list = parse_persona_list(&json);
                personas.set(list);
            }
            Err(e) => {
                tracing::warn!("PersonaListPanel: meta_persona_list failed: {e}");
                error.set(Some(e));
            }
        }
        loading.set(false);
    });

    // Snapshot of the edit slug — peek to avoid render-time subscription
    // (we only need the value to decide whether to show modal).
    let current_edit = edit_slug.peek().clone();

    rsx! {
        div { class: "agent-panel-section agent-panel-personas",
            div { class: "agent-panel-section-header",
                h4 { class: "agent-panel-section-title", {t("persona-panel-title")} }
                button {
                    class: "btn btn-sm btn-secondary persona-create-btn",
                    title: t("persona-action-create"),
                    onclick: move |_| edit_slug.set(Some("__new__".to_string())),
                    "+ {t(\"persona-action-create\")}"
                }
            }

            if *loading.read() {
                div { class: "agent-panel-empty-state", {t("persona-loading")} }
            } else if let Some(err) = error.read().clone() {
                div { class: "agent-panel-empty-state agent-panel-error",
                    "{t(\"persona-error-load\")}: {err}"
                }
            } else if personas.read().is_empty() {
                div { class: "agent-panel-empty-state", {t("persona-panel-empty")} }
            } else {
                div { class: "persona-list",
                    for persona in personas.read().clone() {
                        PersonaListRow {
                            key: "{persona.slug}",
                            persona: persona.clone(),
                            on_edit: move |slug: String| edit_slug.set(Some(slug)),
                            on_talk: move |p: PersonaSummary| on_talk.call(p),
                        }
                    }
                }
            }

            // Mount edit modal when a slug is selected.
            if let Some(slug) = current_edit {
                super::edit_modal::PersonaEditModal {
                    slug: slug.clone(),
                    on_close: move |_| edit_slug.set(None),
                    on_saved: move |_| {
                        // Reload the list after a save.
                        edit_slug.set(None);
                        let mut p = personas;
                        let mut l = loading;
                        spawn(async move {
                            l.set(true);
                            match call_persona_mcp("meta_persona_list", serde_json::json!({})).await {
                                Ok(json) => p.set(parse_persona_list(&json)),
                                Err(e) => tracing::warn!("reload after save failed: {e}"),
                            }
                            l.set(false);
                        });
                    },
                }
            }
        }
    }
}
