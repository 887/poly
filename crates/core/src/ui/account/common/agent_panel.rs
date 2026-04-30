//! Agent panel — per-chat AI transparency surface.
//!
//! Opened via the 🤖 robot button in the chat header. Shows:
//! - Access toggle: whether Claude can see/act on this chat.
//! - Memory: facts stored for this contact/chat, with forget buttons.
//! - Drafts: pending drafts for this chat (Phase B stub).
//! - Personas: PersonaListPanel — list of personas with "Talk to" buttons (Phase E).
//! - Style: ChatStyleEditor mount (stub).
//!
//! ## Phase E wiring
//!
//! `talk_session: Signal<Option<TalkSession>>` is mounted here (natural scope:
//! AgentPanel is the root of the persona UI subtree). When the user clicks "Talk
//! to" on a persona row, `PersonaListPanel.on_talk` fires → we generate a UUID
//! session_id → set `talk_session` → `PersonaTalkToOverlay` mounts.
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
use crate::ui::agent::persona::talk_to_overlay::{PersonaTalkToOverlay, TalkSession};
use crate::ui::agent::persona::{PersonaListPanel, PersonaSummary};
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
// Session ID generation
// ---------------------------------------------------------------------------

/// Generate a short UUID-like session ID without external crate dependencies.
/// Uses the platform timestamp + a simple random fallback for WASM.
fn new_session_id() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        // On WASM use Math.random via js_sys for randomness.
        let r1 = (js_sys::Math::random() * u32::MAX as f64) as u32;
        let r2 = (js_sys::Math::random() * u32::MAX as f64) as u32;
        // js_sys::Date::now() returns ms-since-epoch as f64 — works without
        // enabling the web-sys "Performance" feature.
        let ts = js_sys::Date::now() as u64;
        format!("{ts:016x}-{r1:08x}-{r2:08x}")
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        // Mix in the thread ID for pseudo-uniqueness.
        let tid = format!("{:?}", std::thread::current().id());
        let hash: u64 = tid.bytes().fold(ts as u64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(b as u64)
        });
        format!("{ts:032x}-{hash:016x}")
    }
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
                div { class: "agent-panel-empty-state", {t("agent-panel-drafts-empty")} }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Section: Style (stub)
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
                div { class: "agent-panel-empty-state agent-panel-style-stub",
                    "Style editor coming soon."
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

    // Facts — starts empty; shown as empty state.
    let facts: Signal<Vec<AgentFact>> = use_signal(Vec::new);

    // Phase E — TalkSession signal.  `None` = overlay closed; `Some(s)` = overlay open.
    let mut talk_session: Signal<Option<TalkSession>> = use_signal(|| None);

    // Snapshot for overlay render — peek avoids subscribing AgentPanel to talk_session
    // on every render (hang class #7 countermeasure).
    let current_talk = talk_session.peek().clone();

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

                // Phase E.4: PersonaListPanel with on_talk wired to open overlay.
                PersonaListPanel {
                    on_talk: move |summary: PersonaSummary| {
                        let session = TalkSession {
                            persona_slug: summary.slug.clone(),
                            persona_name: summary.name.clone(),
                            persona_avatar: summary.avatar_emoji.clone(),
                            session_id: new_session_id(),
                        };
                        talk_session.set(Some(session));
                    },
                }

                AgentStyleSection {
                    access_enabled: *access_enabled.read(),
                }
            }

            // Phase E.1: PersonaTalkToOverlay — mounted over the panel when active.
            if let Some(session) = current_talk {
                PersonaTalkToOverlay {
                    session,
                    on_close: move |_| talk_session.set(None),
                }
            }
        }
    }
}
