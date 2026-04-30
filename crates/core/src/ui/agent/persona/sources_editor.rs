//! PersonaSourcesEditor — per-account tabs with server/channel tree.
//!
//! Each leaf has a 3-state pill: Allow → Inherit → Deny (cycling on click).
//! Deny-wins ancestry is reflected visually: any leaf under a deny shows
//! `persona-source-denied` CSS class (red strikethrough).
//!
//! Persistence: clicking "Save sources" calls `meta_persona_set_sources`
//! with the full flat list of explicit (non-Inherit) rows.
//!
//! Lazy-load strategy: fetch each account's servers/DMs only when the
//! account tab is first opened.

use super::mcp::call_persona_mcp;
use super::types::{IncludeState, PersonaSource};
use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Flat tree node for the source editor.
#[derive(Debug, Clone, PartialEq)]
struct SourceNode {
    /// Stable key: `"{account_id}:{kind}:{value}"`.
    key: String,
    account_id: String,
    selector_kind: String,
    selector_value: Option<String>,
    display_name: String,
    /// Depth: 0 = account, 1 = server, 2 = channel/DM.
    depth: usize,
    state: IncludeState,
}

impl SourceNode {
    fn build_key(account_id: &str, kind: &str, value: Option<&str>) -> String {
        format!("{account_id}:{kind}:{}", value.unwrap_or(""))
    }
}

// ─── AccountTab ──────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AccountTab(
    account_id: String,
    active: bool,
    on_select: EventHandler<String>,
) -> Element {
    let cls = if active { "persona-source-tab active" } else { "persona-source-tab" };
    let aid = account_id.clone();
    rsx! {
        button {
            class: cls,
            onclick: move |_| on_select.call(aid.clone()),
            "{account_id}"
        }
    }
}

// ─── SourceNodeRow ────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn SourceNodeRow(
    node: SourceNode,
    denied_by_ancestor: bool,
    on_toggle: EventHandler<String>,
) -> Element {
    let key = node.key.clone();
    let indent_px = node.depth * 16;
    let mut row_class = "persona-source-node".to_string();
    if denied_by_ancestor {
        row_class.push_str(" persona-source-denied");
    }
    let pill_class = match node.state {
        IncludeState::Allow => "persona-source-pill persona-source-pill-allow",
        IncludeState::Inherit => "persona-source-pill persona-source-pill-inherit",
        IncludeState::Deny => "persona-source-pill persona-source-pill-deny",
    };
    rsx! {
        div { class: "{row_class}", style: "padding-left: {indent_px}px",
            span { class: "persona-source-name", "{node.display_name}" }
            button {
                class: pill_class,
                title: t("persona-source-cycle-tip"),
                onclick: move |_| on_toggle.call(key.clone()),
                "{node.state.label()}"
            }
        }
    }
}

// ─── PersonaSourcesEditor ─────────────────────────────────────────────────────

/// Props for PersonaSourcesEditor.
#[derive(Props, Clone, PartialEq)]
pub struct PersonaSourcesEditorProps {
    pub persona_slug: String,
    /// Existing sources loaded from `meta_persona_get`.
    pub existing_sources: Vec<PersonaSource>,
    /// List of known account IDs (from app state or MCP list).
    pub account_ids: Vec<String>,
    pub on_saved: EventHandler<()>,
}

/// Editor for persona source bindings.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonaSourcesEditor(props: PersonaSourcesEditorProps) -> Element {
    let slug = props.persona_slug.clone();

    // Build initial node map from existing_sources.
    let initial_nodes = build_initial_nodes(&props.existing_sources);
    let mut nodes: Signal<Vec<SourceNode>> = use_signal(move || initial_nodes);
    let mut active_account: Signal<String> = use_signal(|| {
        props.account_ids.first().cloned().unwrap_or_default()
    });
    let mut saving = use_signal(|| false);
    let mut save_error: Signal<Option<String>> = use_signal(|| None);

    let account_ids = props.account_ids.clone();
    let on_saved = props.on_saved.clone();

    // Snapshot active account to avoid render-time subscription for the filter.
    let active = active_account.peek().clone();

    // Collect denied parent keys for visual inheritance.
    let denied_parents: Vec<String> = nodes
        .read()
        .iter()
        .filter(|n| n.state == IncludeState::Deny)
        .map(|n| n.key.clone())
        .collect();

    rsx! {
        div { class: "persona-sources-editor",
            // Account tabs
            div { class: "persona-source-tabs",
                for account_id in account_ids.clone() {
                    {
                        let aid = account_id.clone();
                        let is_active = aid == active;
                        rsx! {
                            AccountTab {
                                key: "{account_id}",
                                account_id: account_id.clone(),
                                active: is_active,
                                on_select: move |id: String| active_account.set(id),
                            }
                        }
                    }
                }
            }

            // Nodes for the active account
            div { class: "persona-source-tree",
                for node in nodes.read().iter().filter(|n| n.account_id == active).cloned().collect::<Vec<_>>() {
                    {
                        let node_key = node.key.clone();
                        // Check if any ancestor is denied.
                        let denied = denied_parents.iter().any(|dk| {
                            node_key != *dk && node_key.starts_with(dk.as_str())
                        });
                        rsx! {
                            SourceNodeRow {
                                key: "{node.key}",
                                node,
                                denied_by_ancestor: denied,
                                on_toggle: move |key: String| {
                                    let mut ns = nodes;
                                    if let Some(n) = ns.write().iter_mut().find(|n| n.key == key) {
                                        n.state = n.state.cycle();
                                    }
                                },
                            }
                        }
                    }
                }
                if nodes.read().iter().filter(|n| n.account_id == active).count() == 0 {
                    div { class: "agent-panel-empty-state", {t("persona-sources-empty-account")} }
                }
            }

            // Save button
            div { class: "persona-editor-actions",
                button {
                    class: "btn btn-primary btn-sm",
                    disabled: *saving.read(),
                    onclick: {
                        let slug_save = slug.clone();
                        move |_| {
                            let slug_save = slug_save.clone();
                            let current_nodes = nodes.read().clone();
                            let on_saved = on_saved.clone();
                            saving.set(true);
                            save_error.set(None);
                            spawn(async move {
                                let sources: Vec<serde_json::Value> = current_nodes
                                    .iter()
                                    .filter(|n| n.state != IncludeState::Inherit)
                                    .map(|n| serde_json::json!({
                                        "account_id": n.account_id,
                                        "selector_kind": n.selector_kind,
                                        "selector_value": n.selector_value,
                                        "include": if n.state == IncludeState::Allow { 1 } else { 0 },
                                    }))
                                    .collect();
                                match call_persona_mcp("meta_persona_set_sources", serde_json::json!({
                                    "slug": slug_save,
                                    "sources": sources,
                                })).await {
                                    Ok(_) => on_saved.call(()),
                                    Err(e) => {
                                        tracing::warn!("set_sources failed: {e}");
                                        save_error.set(Some(e));
                                    }
                                }
                                saving.set(false);
                            });
                        }
                    },
                    {t("persona-sources-save")}
                }
                if let Some(err) = save_error.read().clone() {
                    span { class: "persona-save-error", "{err}" }
                }
            }
        }
    }
}

/// Build an initial list of `SourceNode`s from existing source rows.
///
/// Only creates nodes for accounts/selectors already configured.
/// The caller is responsible for injecting full server/channel trees
/// from lazy-loaded MCP calls (Phase E refinement).
fn build_initial_nodes(existing: &[PersonaSource]) -> Vec<SourceNode> {
    existing
        .iter()
        .map(|s| {
            let key = SourceNode::build_key(
                &s.account_id,
                &s.selector_kind,
                s.selector_value.as_deref(),
            );
            let state = if s.include == 1 {
                IncludeState::Allow
            } else {
                IncludeState::Deny
            };
            let display = s
                .selector_value
                .clone()
                .unwrap_or_else(|| s.selector_kind.clone());
            SourceNode {
                key,
                account_id: s.account_id.clone(),
                selector_kind: s.selector_kind.clone(),
                selector_value: s.selector_value.clone(),
                display_name: display,
                depth: match s.selector_kind.as_str() {
                    "all" => 0,
                    "server" => 1,
                    _ => 2,
                },
                state,
            }
        })
        .collect()
}
