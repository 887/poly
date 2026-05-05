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
use crate::state::{AppState, BatchedSignal, ChatViewState};
use crate::ui::account::common::chat_view::catch_up_clipboard_text;
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
        // lint-allow-unused: f64-to-u32/u64 casts on bounded random/timestamp
        // values (clamped 0..=u32::MAX / 0..=u64::MAX before cast).
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let r1 = (js_sys::Math::random() * f64::from(u32::MAX)) as u32;
        // lint-allow-unused: same as above (random u32).
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let r2 = (js_sys::Math::random() * f64::from(u32::MAX)) as u32;
        // js_sys::Date::now() returns ms-since-epoch as f64 — works without
        // enabling the web-sys "Performance" feature.
        // lint-allow-unused: ms-since-epoch fits in u64 for next ~580M years.
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions, clippy::cast_sign_loss)]
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
        // lint-allow-unused: u128→u64 truncation is intentional — only need
        // a low-bits seed for a deterministic per-thread pseudo-id hash.
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let ts_seed = ts as u64;
        let hash: u64 = tid.bytes().fold(ts_seed, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(u64::from(b))
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
// Section: Catch me up — clipboard helper for external LLMs
// ---------------------------------------------------------------------------

/// "Catch me up" — copies the last 20 messages of the current chat as a
/// plain-text summary prompt to the clipboard, so the user can paste them
/// into Claude Desktop / ChatGPT / any external LLM and ask for a recap.
/// No network call from the host. Doesn't depend on the in-app agent
/// being authorised; useful even when the access toggle is off.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AgentCatchUpSection(channel_name: String) -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let mut copy_status: Signal<Option<String>> = use_signal(|| None);
    rsx! {
        div { class: "agent-panel-section agent-panel-catch-up",
            h4 { class: "agent-panel-section-title", {t("agent-panel-catch-up-title")} }
            p { class: "agent-panel-section-desc",
                {t("agent-panel-catch-up-desc")}
            }
            button {
                class: "agent-panel-action-btn",
                onclick: {
                    let chan = channel_name.clone();
                    move |_| {
                        let snapshot = chat_view_state.peek().clone();
                        let payload = catch_up_clipboard_text(&snapshot, &chan, 20);
                        let escaped = payload.replace('\\', r"\\").replace('`', r"\`");
                        let _ = document::eval(&format!(
                            "navigator.clipboard.writeText(`{escaped}`)"
                        ));
                        copy_status.set(Some(t("agent-panel-catch-up-copied")));
                    }
                },
                span { class: "agent-panel-action-icon", "📋" }
                span { class: "agent-panel-action-label",
                    {t("agent-panel-catch-up-button")}
                }
            }
            if let Some(msg) = copy_status.read().clone() {
                p { class: "agent-panel-action-status", "{msg}" }
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
            if let Ok(Some(v)) = storage.get(&key).await
                && let Some(b) = v.as_bool() {
                    access_enabled.set(b);
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

    let _ = &mut app_state;

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

                AgentCatchUpSection {
                    channel_name: chat_name.clone(),
                }

                // PersonaListPanel removed — personas don't make sense in
                // the per-conversation Agent panel (they're a global
                // concept, managed at /agent/personas). The TalkToOverlay
                // is also no longer reachable from here; if we ever
                // want a "talk to persona about this chat" affordance,
                // it belongs as a button in the chat composer or a
                // global keyboard shortcut, not in this panel.

                AgentStyleSection {
                    access_enabled: *access_enabled.read(),
                }
            }

            // PersonaTalkToOverlay kept for compatibility — only mounts
            // if some other code path opens a TalkSession. Currently
            // unreachable from this panel after removing PersonaListPanel.
            if let Some(session) = current_talk {
                PersonaTalkToOverlay {
                    session,
                    on_close: move |_| talk_session.set(None),
                }
            }
        }
    }
}
