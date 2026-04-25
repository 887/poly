//! DraftBanner — renders above the composer when pending agent drafts exist
//! for the current chat channel.
//!
//! The banner reads drafts from the shared `storage.sqlite3` via
//! `crate::storage::drafts::DraftStore` (direct SQLite on native; no-op stub
//! on WASM). It polls every 2 seconds via `use_future` so auto-send countdowns
//! update without a full page reload.
//!
//! # B.4 spec
//! - Shows "✨ {suggested_by} suggests:" label
//! - Preview text truncated to 3 lines (via CSS `line-clamp`)
//! - Countdown "Auto-sending in Ns" when `auto_send_at` is set
//! - Buttons: [Send] [Edit] [Discard] and conditionally [Cancel auto-send]
//!
//! # B.5 DraftsSidebar
//! `DraftsSidebar` is also in this file — it's the cross-chat panel showing all
//! pending drafts for the active account. Opening a row navigates to that chat.

use crate::i18n::{t, t_args};
use crate::storage::drafts::{Draft, DraftStore};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Number of seconds until `auto_send_at` ISO-8601 timestamp, clamped to 0.
fn secs_until(ts: &str) -> Option<i64> {
    // Parse YYYY-MM-DDTHH:MM:SSZ manually — no chrono dep.
    if ts.len() < 19 { return None; }
    let bytes = ts.as_bytes();
    let parse = |slice: &[u8]| -> Option<u64> {
        std::str::from_utf8(slice).ok()?.parse().ok()
    };
    let y = parse(&bytes[0..4])?;
    let mo = parse(&bytes[5..7])?;
    let d = parse(&bytes[8..10])?;
    let h = parse(&bytes[11..13])?;
    let m = parse(&bytes[14..16])?;
    let s = parse(&bytes[17..19])?;

    // Convert to epoch seconds (Julian day arithmetic).
    let a = (14u64.wrapping_sub(mo)) / 12;
    let yr = y + 4800 - a;
    let mon = mo + 12 * a - 3;
    let jdn = d + (153 * mon + 2) / 5 + 365 * yr + yr / 4 - yr / 100 + yr / 400 - 32045;
    let unix_epoch_jdn: u64 = 2440588;
    let epoch_days = jdn.saturating_sub(unix_epoch_jdn);
    let target_secs = epoch_days * 86400 + h * 3600 + m * 60 + s;

    // SystemTime::now() panics on wasm32-unknown-unknown — use Date.now()
    // there. On native, std SystemTime works fine.
    let now_secs: u64 = {
        #[cfg(target_arch = "wasm32")]
        {
            (js_sys::Date::now() / 1000.0) as u64
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        }
    };

    Some((target_secs as i64).saturating_sub(now_secs as i64))
}

/// Open a loopback HTTP request to `poly-chat-mcp` to call a draft tool.
///
/// We read the MCP port from the KV store key `agent.mcp.port` (set by the
/// `/agent` page). Falls back to port 3010 (the default).
async fn call_draft_mcp(tool: &str, args: serde_json::Value) -> bool {
    let port = {
        // Try to read the port from STORAGE. If unavailable, use default.
        #[cfg(not(target_arch = "wasm32"))]
        {
            // On native we can use `std::env` or default.
            std::env::var("POLY_CHAT_MCP_PORT")
                .ok()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(3010)
        }
        #[cfg(target_arch = "wasm32")]
        {
            3010u16
        }
    };

    let url = format!("http://127.0.0.1:{port}/mcp");
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": tool,
            "arguments": args
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    {
        let result = reqwest::Client::new()
            .post(&url)
            .json(&body)
            .send()
            .await;
        match result {
            Ok(resp) => {
                let json: serde_json::Value = resp.json().await.unwrap_or_default();
                !json
                    .get("result")
                    .and_then(|r| r.get("isError"))
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false)
            }
            Err(e) => {
                tracing::warn!("draft MCP call failed: {e}");
                false
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        // On WASM use fetch.
        use wasm_bindgen_futures::JsFuture;
        use wasm_bindgen::JsValue;
        let _ = (url, body); // suppress unused warnings
        false
    }
}

// ─── DraftBanner ──────────────────────────────────────────────────────────────

/// Props for `DraftBanner`.
#[derive(Props, Clone, PartialEq)]
pub struct DraftBannerProps {
    /// The current account ID.
    pub account_id: String,
    /// The current channel / chat ID.
    pub chat_id: String,
}

/// Renders a banner above the composer when pending agent drafts exist for the
/// current channel. Shows the draft text, auto-send countdown, and action buttons.
#[ui_action(None)]
#[context_menu(None)]
#[component]
pub fn DraftBanner(props: DraftBannerProps) -> Element {
    let account_id = props.account_id.clone();
    let chat_id    = props.chat_id.clone();

    // Drafts signal — refreshed every 2 s.
    let mut drafts: Signal<Vec<Draft>> = use_signal(Vec::new);
    let mut tick: Signal<u64> = use_signal(|| 0u64);

    // Poll drafts from local SQLite every 2 s.
    {
        let account_id_poll = account_id.clone();
        let chat_id_poll    = chat_id.clone();
        use_future(move || {
            let account_id_f = account_id_poll.clone();
            let chat_id_f    = chat_id_poll.clone();
            async move {
                loop {
                    // Small sleep before each read so we don't hammer SQLite.
                    // tokio::time::sleep uses Instant::now() which panics on
                    // wasm32-unknown-unknown; use document::eval setTimeout on
                    // web, tokio::time::sleep on native.
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = dioxus::document::eval("setTimeout(() => dioxus.send(true), 2000);").recv::<bool>().await;
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                    let loaded = if let Some(store) = DraftStore::try_open() {
                        store.pending_for_chat(&account_id_f, &chat_id_f)
                    } else {
                        Vec::new()
                    };
                    drafts.set(loaded);
                    let new_tick = tick.read().wrapping_add(1);
                    tick.set(new_tick);
                }
            }
        });
    }

    // Initial load on mount.
    {
        let account_id_init = account_id.clone();
        let chat_id_init    = chat_id.clone();
        use_effect(move || { // poly-lint: allow stale-effect-capture — component keyed by (account_id, chat_id) and remounted on change; draft load is one-shot per mount; Signal::set() requires FnMut so use_reactive_effect cannot be used here
            let loaded = if let Some(store) = DraftStore::try_open() {
                store.pending_for_chat(&account_id_init, &chat_id_init)
            } else {
                Vec::new()
            };
            drafts.set(loaded);
        });
    }

    let current_drafts = drafts.read().clone();
    if current_drafts.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "draft-banner",
            for draft in current_drafts {
                {
                    let account_id_row = account_id.clone();
                    let chat_id_row    = chat_id.clone();
                    rsx! {
                        DraftBannerRow {
                            key: "{draft.id}",
                            draft: draft.clone(),
                            on_refresh: move |_| {
                                // Re-read from SQLite.
                                let store = DraftStore::try_open();
                                let loaded = store
                                    .as_ref()
                                    .map(|s| s.pending_for_chat(&account_id_row, &chat_id_row))
                                    .unwrap_or_default();
                                drafts.set(loaded);
                            },
                        }
                    }
                }
            }
        }
    }
}

// ─── DraftBannerRow ───────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct DraftBannerRowProps {
    draft: Draft,
    on_refresh: EventHandler<()>,
}

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn DraftBannerRow(props: DraftBannerRowProps) -> Element {
    let draft = props.draft.clone();
    let draft_id = draft.id;

    let countdown_secs = draft
        .auto_send_at
        .as_deref()
        .and_then(secs_until)
        .filter(|&s| s > 0);

    let label = t_args("agent-draft-claude-suggests", &[("suggested_by", &draft.suggested_by)]);

    rsx! {
        div { class: "draft-banner-row",
            div { class: "draft-banner-header",
                span { class: "draft-banner-label", "{label}" }
                if let Some(secs) = countdown_secs {
                    span { class: "draft-banner-countdown",
                        {t_args("agent-draft-autosend-in", &[("secs", &secs.to_string())])}
                    }
                }
            }
            div { class: "draft-banner-body",
                // CSS applies line-clamp: 3 on this class.
                "{draft.body}"
            }
            div { class: "draft-banner-actions",
                button {
                    class: "btn btn-primary draft-btn-send",
                    title: t("agent-draft-send"),
                    onclick: {
                        let on_refresh = props.on_refresh.clone();
                        move |_| {
                            let on_refresh = on_refresh.clone();
                            spawn(async move {
                                let ok = call_draft_mcp(
                                    "draft_approve",
                                    serde_json::json!({ "draft_id": draft_id }),
                                ).await;
                                if ok { on_refresh.call(()); }
                            });
                        }
                    },
                    {t("agent-draft-send")}
                }
                button {
                    class: "btn draft-btn-edit",
                    title: t("agent-draft-edit"),
                    // Edit opens the draft body in the composer for manual editing.
                    // For MVP: discard the draft so user can retype it.
                    onclick: {
                        let body = draft.body.clone();
                        let on_refresh = props.on_refresh.clone();
                        move |_| {
                            // Copy draft body to clipboard / composer is handled by
                            // parent; for now we just discard and let user retype.
                            let on_refresh = on_refresh.clone();
                            let body_clone = body.clone();
                            spawn(async move {
                                // Discard so banner clears.
                                let ok = call_draft_mcp(
                                    "draft_discard",
                                    serde_json::json!({ "draft_id": draft_id }),
                                ).await;
                                if ok {
                                    // Inject into composer via JS eval.
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        let js = format!(
                                            "const el = document.getElementById('poly-message-composer'); \
                                             if (el) {{ el.value = {}; el.dispatchEvent(new Event('input')); }}",
                                            serde_json::to_string(&body_clone).unwrap_or_default()
                                        );
                                        let _ = document::eval(&js);
                                    }
                                    on_refresh.call(());
                                }
                            });
                        }
                    },
                    {t("agent-draft-edit")}
                }
                button {
                    class: "btn draft-btn-discard",
                    title: t("agent-draft-discard"),
                    onclick: {
                        let on_refresh = props.on_refresh.clone();
                        move |_| {
                            let on_refresh = on_refresh.clone();
                            spawn(async move {
                                let ok = call_draft_mcp(
                                    "draft_discard",
                                    serde_json::json!({ "draft_id": draft_id }),
                                ).await;
                                if ok { on_refresh.call(()); }
                            });
                        }
                    },
                    {t("agent-draft-discard")}
                }
                if countdown_secs.is_some() {
                    button {
                        class: "btn draft-btn-cancel-autosend",
                        title: t("agent-draft-cancel-autosend"),
                        onclick: {
                            let on_refresh = props.on_refresh.clone();
                            move |_| {
                                let on_refresh = on_refresh.clone();
                                spawn(async move {
                                    let ok = call_draft_mcp(
                                        "draft_cancel_autosend",
                                        serde_json::json!({ "draft_id": draft_id }),
                                    ).await;
                                    if ok { on_refresh.call(()); }
                                });
                            }
                        },
                        {t("agent-draft-cancel-autosend")}
                    }
                }
            }
        }
    }
}

// ─── DraftsSidebar ────────────────────────────────────────────────────────────

/// Cross-chat panel showing all pending drafts for the active account.
/// Used as a `ChatUtilityPanel::Drafts` variant (B.5).
#[derive(Props, Clone, PartialEq)]
pub struct DraftsSidebarProps {
    /// The currently active account ID.
    pub account_id: String,
    /// Callback when the user clicks a draft row — navigates to that chat.
    pub on_open_chat: EventHandler<(String, String)>, // (account_id, chat_id)
}

#[ui_action(None)]
#[context_menu(None)]
#[component]
pub fn DraftsSidebar(props: DraftsSidebarProps) -> Element {
    let account_id = props.account_id.clone();

    let mut drafts: Signal<Vec<Draft>> = use_signal(Vec::new);

    // Initial load + poll every 2 s.
    {
        let account_id_poll = account_id.clone();
        use_future(move || {
            let account_id_f = account_id_poll.clone();
            async move {
                loop {
                    let loaded = if let Some(store) = DraftStore::try_open() {
                        store.pending_for_account(&account_id_f)
                    } else {
                        Vec::new()
                    };
                    drafts.set(loaded);
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = dioxus::document::eval("setTimeout(() => dioxus.send(true), 2000);").recv::<bool>().await;
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }
        });
    }

    let current_drafts = drafts.read().clone();

    rsx! {
        div { class: "drafts-sidebar",
            div { class: "drafts-sidebar-header",
                span { class: "drafts-sidebar-title", {t("agent-drafts-sidebar-title")} }
            }
            if current_drafts.is_empty() {
                div { class: "drafts-sidebar-empty",
                    {t("agent-drafts-sidebar-empty")}
                }
            } else {
                div { class: "drafts-sidebar-list",
                    for draft in current_drafts {
                        DraftsSidebarRow {
                            key: "{draft.id}",
                            draft: draft.clone(),
                            on_open: {
                                let on_open_chat = props.on_open_chat.clone();
                                move |_| on_open_chat.call((draft.account_id.clone(), draft.chat_id.clone()))
                            },
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DraftsSidebarRowProps {
    draft: Draft,
    on_open: EventHandler<()>,
}

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn DraftsSidebarRow(props: DraftsSidebarRowProps) -> Element {
    let draft = props.draft.clone();
    let preview: String = draft.body.chars().take(120).collect();
    let has_autosend = draft.auto_send_at.is_some();

    rsx! {
        button {
            class: "drafts-sidebar-row",
            onclick: move |_| props.on_open.call(()),
            div { class: "drafts-sidebar-row-meta",
                span { class: "drafts-sidebar-row-chat", "{draft.chat_id}" }
                span { class: "drafts-sidebar-row-agent", "{draft.suggested_by}" }
                if has_autosend {
                    span { class: "drafts-sidebar-row-autosend-badge", "⏱" }
                }
            }
            div { class: "drafts-sidebar-row-preview", "{preview}" }
        }
    }
}
