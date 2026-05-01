//! `PersonaTalkToOverlay` — slide-in chat overlay for talking to a persona.
//!
//! Phase E of `plan-meta-personalities.md`.
//!
//! ## Architecture
//!
//! - `TalkSession`     — shared type: slug + session_id, mounted in `AgentPanel`.
//! - `TalkLine`        — one transcript message (User | Assistant).
//! - `PersonaTalkToOverlay` — the overlay component (E.1 + E.2 + E.3 + E.6).
//! - `TalkTranscript`  — transcript scroller (sub-component, Single Responsibility).
//! - `TalkComposer`    — input + send button (sub-component).
//!
//! ## Reactive hygiene (CLAUDE.md non-negotiables)
//!
//! - No raw `Signal::write()` — all mutations via `.set()` (single-subscriber local).
//! - No `use_effect` with non-Signal captures — `use_reactive_effect` for keyed loads.
//! - KV reads/writes via `crate::STORAGE.get()`.
//! - `.peek()` for hook keys; subscription not needed for session_id.
//!
//! ## Session pruning policy
//!
//! At overlay open we load stored sessions for the persona and prune to the 5 most
//! recent by `started_at` (ISO-8601 string sort, lexicographic ≡ chronological).
//! When there are exactly 5 sessions the oldest (smallest `started_at`) is dropped
//! to make room for the new one, preserving a strict cap of 5.

use super::mcp::call_persona_mcp;
use crate::i18n::t;
use crate::state::use_reactive_effect;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};
use serde::{Deserialize, Serialize};

// ─── Public shared types ──────────────────────────────────────────────────────

/// Uniquely identifies an open or resumed talk session.
///
/// Mounted as `Signal<Option<TalkSession>>` in `AgentPanel` and passed down
/// to `PersonaListPanel` (to open) and `PersonaTalkToOverlay` (to render).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalkSession {
    pub persona_slug: String,
    pub persona_name: String,
    pub persona_avatar: String,
    pub session_id: String,
}

// ─── Transcript types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TalkLineKind {
    User,
    Assistant,
}

/// One line in the conversation transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TalkLine {
    pub kind: TalkLineKind,
    pub content: String,
    pub timestamp_ms: u64,
}

// ─── KV helpers ───────────────────────────────────────────────────────────────

fn kv_dev_mode_key() -> &'static str {
    "persona.talk.dev_mode"
}

fn kv_session_key(slug: &str, session_id: &str) -> String {
    format!("persona.talk.{slug}.{session_id}")
}

fn kv_sessions_index_key(slug: &str) -> String {
    format!("persona.talk.{slug}.__index__")
}

/// Maximum sessions kept per persona (older ones are pruned at session-start).
const MAX_SESSIONS: usize = 5;

// ─── Transcript sub-component ─────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn TalkTranscript(
    lines: Signal<Vec<TalkLine>>,
    dev_mode: bool,
) -> Element {
    rsx! {
        div { class: "talk-transcript",
            if lines.read().is_empty() {
                div { class: "talk-transcript-empty",
                    {t("persona-talk-transcript-empty")}
                }
            } else {
                for line in lines.read().clone() {
                    {
                        let (row_class, label) = match line.kind {
                            TalkLineKind::User => ("talk-line talk-line-user", "You"),
                            TalkLineKind::Assistant => ("talk-line talk-line-assistant", "Persona"),
                        };
                        rsx! {
                            div { class: row_class, key: "{line.timestamp_ms}",
                                span { class: "talk-line-label", "{label}" }
                                // Dev mode: render raw bundle JSON in <pre> for user messages
                                // that contain a bundle marker; regular content otherwise.
                                if dev_mode && line.kind == TalkLineKind::Assistant && line.content.starts_with("__bundle__:") {
                                    div { class: "talk-bundle-dev",
                                        pre { class: "talk-bundle-json",
                                            {line.content.trim_start_matches("__bundle__:")}
                                        }
                                    }
                                } else if !dev_mode && line.kind == TalkLineKind::Assistant && line.content.starts_with("__bundle__:") {
                                    // Normal mode: summarise the bundle rather than dump raw JSON
                                    {render_bundle_summary(line.content.trim_start_matches("__bundle__:"))}
                                } else {
                                    p { class: "talk-line-content", "{line.content}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a compact summary of the PersonaContextBundle in normal mode.
fn render_bundle_summary(json_str: &str) -> Element {
    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return rsx! { p { class: "talk-line-content", "Context bundle (parse error)." } },
    };

    let system_excerpt = parsed
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .chars()
        .take(120)
        .collect::<String>();

    let pinned_count = parsed
        .get("pinned_facts")
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);

    let chat_count = parsed
        .get("chats")
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);

    let total_messages: usize = parsed
        .get("chats")
        .and_then(|v| v.as_array())
        .map_or(0, |chats| {
            chats
                .iter()
                .map(|c| {
                    c.get("recent_messages")
                        .and_then(|m| m.as_array())
                        .map_or(0, Vec::len)
                })
                .sum()
        });

    rsx! {
        div { class: "talk-bundle-summary",
            p { class: "talk-bundle-system-excerpt",
                em { "{system_excerpt}…" }
            }
            ul { class: "talk-bundle-stats",
                li { "Pinned facts: {pinned_count}" }
                li { "Sources: {chat_count} chats, {total_messages} messages" }
            }
        }
    }
}

// ─── Composer sub-component ───────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn TalkComposer(
    draft: Signal<String>,
    loading: bool,
    on_send: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "talk-composer",
            textarea {
                class: "talk-composer-input",
                placeholder: t("persona-talk-placeholder"),
                disabled: loading,
                value: draft.read().clone(),
                oninput: move |evt| draft.set(evt.value()),
                onkeydown: move |evt| {
                    // Ctrl+Enter or Meta+Enter to send
                    if evt.key() == Key::Enter
                        && (evt.modifiers().ctrl() || evt.modifiers().meta())
                        && !loading
                    {
                        on_send.call(());
                    }
                },
            }
            button {
                class: "btn btn-primary talk-send-btn",
                disabled: loading || draft.read().trim().is_empty(),
                onclick: move |_| on_send.call(()),
                if loading {
                    span { class: "spinner spinner-sm" }
                    " {t(\"persona-talk-sending\")}"
                } else {
                    {t("persona-talk-send")}
                }
            }
        }
    }
}

// ─── Session picker sub-component ────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn SessionPicker(
    sessions: Vec<String>,
    on_resume: EventHandler<String>,
    on_new: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "talk-session-picker",
            h5 { class: "talk-session-picker-title", {t("persona-talk-pick-session")} }
            for session_id in sessions {
                {
                    let sid = session_id.clone();
                    rsx! {
                        button {
                            class: "btn btn-sm btn-secondary talk-session-resume-btn",
                            key: "{session_id}",
                            onclick: move |_| on_resume.call(sid.clone()),
                            "Resume {&session_id[..session_id.len().min(8)]}…"
                        }
                    }
                }
            }
            button {
                class: "btn btn-sm btn-primary talk-session-new-btn",
                onclick: move |_| on_new.call(()),
                {t("persona-talk-new-session")}
            }
        }
    }
}

// ─── Main overlay component ───────────────────────────────────────────────────

/// State for the overlay's async loading / pick-session flow.
#[derive(Debug, Clone, PartialEq, Eq)]
enum OverlayPhase {
    /// Loading stored sessions from KV.
    LoadingSessions,
    /// Sessions loaded; show picker unless there are none (then auto-start).
    PickSession(Vec<String>),
    /// Active conversation in this session.
    Chatting,
}

/// Slide-in overlay for talking to a persona (Phase E).
///
/// Mounted by `AgentPanel` when `talk_session` signal is `Some(...)`.
/// The caller is responsible for setting `talk_session` to `None` to close.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonaTalkToOverlay(
    session: TalkSession,
    on_close: EventHandler<()>,
) -> Element {
    // ── Component-local signals (single-subscriber, no hang risk) ──────────

    // Overlay phase: loading → pick-session → chatting
    let mut phase: Signal<OverlayPhase> = use_signal(|| OverlayPhase::LoadingSessions);

    // Transcript for the active session
    let mut lines: Signal<Vec<TalkLine>> = use_signal(Vec::new);

    // Composer draft
    let mut draft: Signal<String> = use_signal(String::new);

    // Inflight send
    let loading: Signal<bool> = use_signal(|| false);

    // Error string (toast)
    let mut error: Signal<Option<String>> = use_signal(|| None);

    // Dev mode toggle (persisted via KV)
    let mut dev_mode: Signal<bool> = use_signal(|| true);

    // ── Dev mode — load persisted preference ──────────────────────────────
    use_future(move || async move {
        if let Some(storage) = crate::STORAGE.get()
            && let Ok(Some(v)) = storage.get(kv_dev_mode_key()).await
                && let Some(b) = v.as_bool() {
                    dev_mode.set(b);
                }
    });

    // ── Load existing sessions on (slug, session_id) change ───────────────
    //
    // use_reactive_effect so re-opening the overlay for a DIFFERENT persona
    // re-fires the load (hang class #6 countermeasure).
    let slug_dep = session.persona_slug.clone();
    use_reactive_effect(slug_dep, move |slug| {
        spawn(async move {
            let stored = load_sessions_for_persona(&slug).await;
            if stored.is_empty() {
                // No prior sessions — go straight to a new conversation
                phase.set(OverlayPhase::Chatting);
            } else {
                phase.set(OverlayPhase::PickSession(stored));
            }
        });
    });

    // ── Snapshots for rendering (peek — no subscription needed) ───────────
    let current_phase = phase.read().clone();
    let is_loading = *loading.read();
    let current_error = error.read().clone();
    let is_dev_mode = *dev_mode.read();

    rsx! {
        div { class: "persona-talk-overlay",
            // ── Header ─────────────────────────────────────────────────────
            div { class: "persona-talk-header",
                span { class: "persona-talk-avatar", "{session.persona_avatar}" }
                span { class: "persona-talk-name", "{session.persona_name}" }

                // Dev mode toggle
                button {
                    class: if is_dev_mode { "btn btn-xs btn-active talk-dev-toggle" } else { "btn btn-xs talk-dev-toggle" },
                    title: t("persona-talk-dev-mode-toggle"),
                    onclick: move |_| {
                        let next = !*dev_mode.read();
                        dev_mode.set(next);
                        spawn(async move {
                            if let Some(storage) = crate::STORAGE.get() {
                                drop(storage
                                    .set(kv_dev_mode_key(), serde_json::json!(next))
                                    .await);
                            }
                        });
                    },
                    if is_dev_mode { "DEV" } else { "NORMAL" }
                }

                button {
                    class: "btn btn-icon talk-close-btn",
                    title: t("persona-talk-close"),
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            // ── Body ───────────────────────────────────────────────────────
            match current_phase {
                OverlayPhase::LoadingSessions => rsx! {
                    div { class: "talk-loading-sessions",
                        span { class: "spinner" }
                        " {t(\"persona-talk-loading-sessions\")}"
                    }
                },

                OverlayPhase::PickSession(sessions) => {
                    let slug_resume = session.persona_slug.clone();
                    let slug_new    = session.persona_slug.clone();
                    rsx! {
                        SessionPicker {
                            sessions,
                            on_resume: move |sid: String| {
                                let slug = slug_resume.clone();
                                let sid2 = sid.clone();
                                spawn(async move {
                                    let loaded = load_transcript(&slug, &sid2).await;
                                    lines.set(loaded);
                                    phase.set(OverlayPhase::Chatting);
                                });
                            },
                            on_new: move |_| {
                                let slug = slug_new.clone();
                                let new_sid = session.session_id.clone();
                                spawn(async move {
                                    prune_old_sessions(&slug, &new_sid).await;
                                    lines.set(Vec::new());
                                    phase.set(OverlayPhase::Chatting);
                                });
                            },
                        }
                    }
                },

                OverlayPhase::Chatting => {
                    // Pre-clone for each closure that needs slug/sid so no
                    // "moved after use" errors (each closure captures its own copy).
                    let slug_retry  = session.persona_slug.clone();
                    let sid_retry   = session.session_id.clone();
                    let slug_send   = session.persona_slug.clone();
                    let sid_send    = session.session_id.clone();

                    rsx! {
                        // Error toast
                        if let Some(ref err_msg) = current_error {
                            div { class: "talk-error-toast",
                                span { "{err_msg}" }
                                button {
                                    class: "btn btn-xs btn-secondary talk-retry-btn",
                                    onclick: move |_| {
                                        // Retry: re-send the last user message
                                        let last_user = lines
                                            .peek()
                                            .iter()
                                            .rev()
                                            .find(|l| l.kind == TalkLineKind::User)
                                            .map(|l| l.content.clone());
                                        if let Some(msg) = last_user {
                                            error.set(None);
                                            let slug = slug_retry.clone();
                                            let sid  = sid_retry.clone();
                                            spawn(async move {
                                                invoke_and_append(&slug, &sid, &msg, is_dev_mode, lines, loading, error).await;
                                            });
                                        }
                                    },
                                    {t("persona-talk-retry")}
                                }
                                button {
                                    class: "btn btn-xs btn-icon",
                                    onclick: move |_| error.set(None),
                                    "✕"
                                }
                            }
                        }

                        TalkTranscript { lines, dev_mode: is_dev_mode }

                        TalkComposer {
                            draft,
                            loading: is_loading,
                            on_send: move |_| {
                                let msg = draft.peek().trim().to_string();
                                if msg.is_empty() || *loading.peek() {
                                    return;
                                }
                                draft.set(String::new());
                                let slug = slug_send.clone();
                                let sid  = sid_send.clone();
                                spawn(async move {
                                    invoke_and_append(&slug, &sid, &msg, is_dev_mode, lines, loading, error).await;
                                });
                            },
                        }
                    }
                },
            }
        }
    }
}

// ─── Async helpers ────────────────────────────────────────────────────────────

/// Append a user message, call `meta_persona_invoke`, append the result.
///
/// Manages loading + error signals.  Persists transcript after each turn.
// lint-allow-unused: 7 args is fewer than the alternative struct-of-signals plumbing
#[allow(clippy::too_many_arguments)]
async fn invoke_and_append(
    slug: &str,
    session_id: &str,
    user_msg: &str,
    _dev_mode: bool,
    mut lines: Signal<Vec<TalkLine>>,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    loading.set(true);

    let now_ms = current_timestamp_ms();

    // Append user message immediately (optimistic).
    {
        let mut current = lines.peek().clone();
        current.push(TalkLine {
            kind: TalkLineKind::User,
            content: user_msg.to_string(),
            timestamp_ms: now_ms,
        });
        lines.set(current);
    }

    // Call the MCP tool.
    let result = call_persona_mcp(
        "meta_persona_invoke",
        serde_json::json!({
            "slug": slug,
            "user_prompt": user_msg,
            "include_summaries": true,
        }),
    )
    .await;

    loading.set(false);

    match result {
        Ok(bundle_json) => {
            // Serialise the bundle back to a string for storage.
            let bundle_str = serde_json::to_string_pretty(&bundle_json)
                .unwrap_or_else(|_| bundle_json.to_string());

            // Prefix with `__bundle__:` so the transcript renderer knows to
            // display it in dev or normal mode specially.
            // dev/normal currently render the same prefix; renderer branches downstream.
            let content = format!("__bundle__:{bundle_str}");

            let mut current = lines.peek().clone();
            current.push(TalkLine {
                kind: TalkLineKind::Assistant,
                content,
                timestamp_ms: current_timestamp_ms(),
            });
            lines.set(current);

            // Persist transcript.
            persist_transcript(slug, session_id, &lines.peek()).await;
        }
        Err(e) => {
            // Leave the user message visible; show the error toast.
            tracing::warn!("PersonaTalkToOverlay: invoke error for '{slug}': {e}");
            error.set(Some(e));
        }
    }
}

/// Persist the current transcript to KV at `persona.talk.<slug>.<session_id>`.
async fn persist_transcript(slug: &str, session_id: &str, lines: &[TalkLine]) {
    let Some(storage) = crate::STORAGE.get() else { return };
    let key = kv_session_key(slug, session_id);
    let json = match serde_json::to_value(lines) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("PersonaTalkToOverlay: failed to serialise transcript: {e}");
            return;
        }
    };
    if let Err(e) = storage.set(&key, json).await {
        tracing::warn!("PersonaTalkToOverlay: KV persist failed: {e}");
    }

    // Update the session index for this persona.
    update_session_index(slug, session_id, storage).await;
}

/// Add `session_id` to the sessions index for `slug`.
async fn update_session_index(slug: &str, session_id: &str, storage: &crate::storage::Storage) {
    let idx_key = kv_sessions_index_key(slug);
    let mut sessions: Vec<String> = match storage.get(&idx_key).await {
        Ok(Some(v)) => serde_json::from_value(v).unwrap_or_default(),
        _ => vec![],
    };
    if !sessions.contains(&session_id.to_string()) {
        sessions.push(session_id.to_string());
    }
    drop(storage.set(&idx_key, serde_json::to_value(&sessions).unwrap_or_default()).await);
}

/// Load stored session IDs for a persona (ordered oldest-first by insertion).
async fn load_sessions_for_persona(slug: &str) -> Vec<String> {
    let Some(storage) = crate::STORAGE.get() else { return vec![] };
    let idx_key = kv_sessions_index_key(slug);
    match storage.get(&idx_key).await {
        Ok(Some(v)) => serde_json::from_value(v).unwrap_or_default(),
        _ => vec![],
    }
}

/// Load transcript lines for a specific session from KV.
async fn load_transcript(slug: &str, session_id: &str) -> Vec<TalkLine> {
    let Some(storage) = crate::STORAGE.get() else { return vec![] };
    let key = kv_session_key(slug, session_id);
    match storage.get(&key).await {
        Ok(Some(v)) => serde_json::from_value(v).unwrap_or_default(),
        _ => vec![],
    }
}

/// Prune old sessions, keeping at most MAX_SESSIONS - 1 (making room for new_sid).
///
/// Pruning policy: drop the oldest sessions (by insertion order in the index,
/// which is chronological since we push on first persist). When there are exactly
/// MAX_SESSIONS, the oldest is dropped.  KV records for pruned sessions are also
/// deleted.
async fn prune_old_sessions(slug: &str, new_sid: &str) {
    let Some(storage) = crate::STORAGE.get() else { return };
    let idx_key = kv_sessions_index_key(slug);
    let mut sessions: Vec<String> = match storage.get(&idx_key).await {
        Ok(Some(v)) => serde_json::from_value(v).unwrap_or_default(),
        _ => vec![],
    };

    // Drop sessions over the cap (oldest first — index is insertion-ordered).
    while sessions.len() >= MAX_SESSIONS {
        let oldest = sessions.remove(0);
        let data_key = kv_session_key(slug, &oldest);
        drop(storage.delete(&data_key).await);
    }

    // Add the new session id to the index.
    if !sessions.contains(&new_sid.to_string()) {
        sessions.push(new_sid.to_string());
    }

    drop(storage
        .set(&idx_key, serde_json::to_value(&sessions).unwrap_or_default())
        .await);
}

/// Current Unix timestamp in milliseconds.
fn current_timestamp_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        // js_sys::Date::now() returns ms-since-epoch as f64 — works without
        // enabling the web-sys "Performance" feature.
        // lint-allow-unused: JS epoch ms → u64; bounded < 2^53
        #[allow(clippy::cast_possible_truncation, clippy::as_conversions, clippy::cast_sign_loss, clippy::cast_precision_loss)]
        let v = js_sys::Date::now() as u64;
        v
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
