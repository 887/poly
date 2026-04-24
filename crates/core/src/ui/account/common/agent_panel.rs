//! Agent panel — per-chat AI transparency surface.
//!
//! Opened via the 🤖 robot button in the chat header. Shows:
//! - Access toggle: whether Claude can see/act on this chat.
//! - Memory: facts stored for this contact/chat, with forget buttons.
//! - Drafts: pending drafts for this chat (Phase B stub).
//! - Style: ChatStyleEditor mount (Phase E — stubbed until available).
//! - Activity: last N agent actions on this chat (nice-to-have stub).
//!
//! ## Data access
//! KV reads/writes go through `crate::STORAGE` (same DB as the MCP server).
//! The access toggle key is `agent.chat.<account_id>.<chat_id>.mcp_access`.
//! Facts are read from the `contact_facts` / `chat_notes` tables via direct
//! SQL using `crate::STORAGE`.
//!
//! ## 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX + logic.

use crate::i18n::{t, t_args};
use crate::state::{AppState, BatchedSignal, ChatData};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

// ---------------------------------------------------------------------------
// KV key helpers
// ---------------------------------------------------------------------------

fn kv_mcp_access(account_id: &str, chat_id: &str) -> String {
    format!("agent.chat.{account_id}.{chat_id}.mcp_access")
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A stored memory fact for this chat/contact.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentFact {
    pub id: i64,
    pub content: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Section: Access toggle
// ---------------------------------------------------------------------------

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AgentAccessToggle(
    account_id: String,
    chat_id: String,
    enabled: Signal<bool>,
) -> Element {
    rsx! {
        div { class: "agent-panel-section agent-panel-access",
            div { class: "agent-panel-section-header",
                span { class: "agent-panel-section-title", {t("agent-panel-access-label")} }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *enabled.read(),
                        oninput: move |evt| {
                            let val = evt.checked();
                            enabled.set(val);
                            let key = kv_mcp_access(&account_id, &chat_id);
                            spawn(async move {
                                let Some(storage) = crate::STORAGE.get() else { return };
                                if let Err(e) = storage.set(&key, serde_json::json!(val)).await {
                                    tracing::warn!("agent_panel: failed to persist access toggle: {e}");
                                }
                            });
                        },
                    }
                    span { class: "toggle-slider" }
                }
            }
            p { class: "agent-panel-access-desc settings-toggle-desc",
                {t("agent-panel-access-description")}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Section: Memory
// ---------------------------------------------------------------------------

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AgentMemorySection(
    facts: Signal<Vec<AgentFact>>,
    account_id: String,
    chat_id: String,
    access_enabled: bool,
) -> Element {
    rsx! {
        div { class: "agent-panel-section agent-panel-memory",
            h4 { class: "agent-panel-section-title", {t("agent-panel-memory-title")} }
            if !access_enabled {
                div { class: "agent-panel-disabled-state", {t("agent-panel-disabled-state")} }
            } else if facts.read().is_empty() {
                div { class: "agent-panel-empty-state", {t("agent-panel-memory-empty")} }
            } else {
                div { class: "agent-panel-fact-list",
                    for fact in facts.read().clone() {
                        {
                            let fact_id = fact.id;
                            let mut facts_clone = facts;
                            let account_id_c = account_id.clone();
                            let chat_id_c = chat_id.clone();
                            rsx! {
                                div { class: "agent-panel-fact-row", key: "{fact_id}",
                                    span { class: "agent-panel-fact-content", "{fact.content}" }
                                    button {
                                        class: "agent-panel-forget-btn danger-btn",
                                        title: t("agent-panel-memory-forget"),
                                        onclick: move |_| {
                                            let account_id_inner = account_id_c.clone();
                                            let chat_id_inner = chat_id_c.clone();
                                            spawn(async move {
                                                let Some(storage) = crate::STORAGE.get() else { return };
                                                // Attempt to delete from contact_facts table.
                                                // We pass the fact id as a KV delete keyed by a
                                                // sentinel so the host bridge can identify it.
                                                // For now we just clear via KV convention and
                                                // rely on the MCP to clean up on next sync.
                                                let key = format!(
                                                    "agent.fact.{account_id_inner}.{chat_id_inner}.{fact_id}"
                                                );
                                                if let Err(e) = storage.delete(&key).await {
                                                    tracing::warn!(
                                                        "agent_panel: forget fact {fact_id} failed: {e}"
                                                    );
                                                }
                                            });
                                            // Optimistic remove from local list
                                            facts_clone.write().retain(|f| f.id != fact_id);
                                        },
                                        {t("agent-panel-memory-forget")}
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

// ---------------------------------------------------------------------------
// Section: Drafts (Phase B stub)
// ---------------------------------------------------------------------------

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AgentDraftsSection(access_enabled: bool) -> Element {
    rsx! {
        div { class: "agent-panel-section agent-panel-drafts",
            h4 { class: "agent-panel-section-title", {t("agent-panel-drafts-title")} }
            if !access_enabled {
                div { class: "agent-panel-disabled-state", {t("agent-panel-disabled-state")} }
            } else {
                // Phase B: when DraftsSidebar component is available, mount it here.
                // For now show an empty state.
                div { class: "agent-panel-empty-state", {t("agent-panel-drafts-empty")} }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Section: Style — Phase E ChatStyleEditor (stub)
// ---------------------------------------------------------------------------

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AgentStyleSection(access_enabled: bool) -> Element {
    rsx! {
        div { class: "agent-panel-section agent-panel-style",
            h4 { class: "agent-panel-section-title", {t("agent-panel-style-title")} }
            if !access_enabled {
                div { class: "agent-panel-disabled-state", {t("agent-panel-disabled-state")} }
            } else {
                // Phase E: mount ChatStyleEditor here once
                // crates/core/src/ui/agent/chat_style_editor.rs lands.
                // TODO(phase-e): replace stub with: rsx! { ChatStyleEditor { account_id, chat_id } }
                div { class: "agent-panel-empty-state agent-panel-style-stub",
                    "Style editor coming soon (Phase E)."
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main AgentPanel component
// ---------------------------------------------------------------------------

/// Chat-level agent transparency panel.
///
/// Props:
/// - `account_id`: the account currently active (from `nav.active_account_id`).
/// - `chat_id`: the channel / DM / group ID (from `nav.selected_channel`).
/// - `chat_name`: display name shown in the panel header.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AgentPanel(
    account_id: String,
    chat_id: String,
    chat_name: String,
) -> Element {
    let mut app_state: BatchedSignal<AppState> = use_context();

    // Access toggle — load persisted value from KV.
    let mut access_enabled = use_signal(|| false);
    let kv_key_for_load = kv_mcp_access(&account_id, &chat_id);
    use_future(move || {
        let key = kv_key_for_load.clone();
        async move {
            let Some(storage) = crate::STORAGE.get() else { return };
            if let Ok(Some(v)) = storage.get(&key).await {
                if let Some(b) = v.as_bool() {
                    access_enabled.set(b);
                }
            }
        }
    });

    // Facts — load from KV namespace (Phase A's MCP stores facts under
    // `agent.facts.<account>.<chat>.*`). We enumerate a prefix and deserialise.
    // For now the list starts empty; we'll show the empty state.
    let facts: Signal<Vec<AgentFact>> = use_signal(Vec::new);

    // Header dropped — the agent panel now lives inside the utility-rail
    // tab system, which renders its own consistent header. Re-clicking the
    // 🤖 button in the chat header closes the tab. Old `chat_name` arg
    // kept on the prop signature so callers don't need to change.
    let _ = (&chat_name, &mut app_state);

    rsx! {
        aside { class: "user-sidebar agent-panel-sidebar",

            div { class: "agent-panel-body",
                AgentAccessToggle {
                    account_id: account_id.clone(),
                    chat_id: chat_id.clone(),
                    enabled: access_enabled,
                }
                AgentMemorySection {
                    facts,
                    account_id: account_id.clone(),
                    chat_id: chat_id.clone(),
                    access_enabled: *access_enabled.read(),
                }
                AgentDraftsSection {
                    access_enabled: *access_enabled.read(),
                }
                AgentStyleSection {
                    access_enabled: *access_enabled.read(),
                }
            }
        }
    }
}
