//! PersonaOutboundAllowlistEditor — G.1 + G.2 + G.4 + G.6
//!
//! Visible inside PersonaEditModal's "Outbound" section only when
//! `proactivity == "outbound-allowlisted"`.
//!
//! Responsibilities:
//! - G.1  Render per-(account, chat) allow/deny rows loaded via
//!   `meta_persona_list_outbound_allows`.
//! - G.2  Per-row `max_messages_per_day` stepper (1–20, default 1).
//! - G.4  Dry-run posture banner when `rate_limit_per_hour == 0`.
//! - G.6  Quiet-hours per-persona toggle stored in a Signal; persisted on
//!   save by the parent modal via `meta_persona_update`.
//!   (The DB column `quiet_hours_disabled` is added as an ALTER TABLE
//!   guard-or-ignore migration in memory.rs Phase G.6 extension.)
//!
//! ## Reactive hygiene
//! - All signals are single-component-scoped local `Signal<T>`.
//! - No raw `Signal::write()` — uses `.set()` which is safe for local signals.
//! - `use_reactive_effect` re-fires when `persona_slug` changes.

use super::mcp::call_persona_mcp;
use crate::i18n::t;
use crate::state::use_reactive_effect;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

// ─── OutboundAllowRow ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundAllowEntry {
    pub account_id: String,
    pub chat_id: String,
    pub max_messages_per_day: i64,
}

impl OutboundAllowEntry {
    fn from_json(v: &serde_json::Value) -> Option<Self> {
        Some(OutboundAllowEntry {
            account_id: v.get("account_id")?.as_str()?.to_string(),
            chat_id: v.get("chat_id")?.as_str()?.to_string(),
            max_messages_per_day: v
                .get("max_messages_per_day")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(1),
        })
    }
}

// ─── DryRunBanner (G.4) ──────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn DryRunBanner() -> Element {
    rsx! {
        div { class: "persona-outbound-dry-run-banner",
            span { class: "persona-banner-icon", "⚠" }
            span { class: "persona-banner-text", {t("persona-outbound-dry-run-banner")} }
        }
    }
}

// ─── AllowRow ────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AllowRow(
    entry: OutboundAllowEntry,
    on_cap_change: EventHandler<(String, String, i64)>,
    on_remove: EventHandler<(String, String)>,
) -> Element {
    let cap = entry.max_messages_per_day.clamp(1, 20);
    let acct = entry.account_id.clone();
    let chat = entry.chat_id.clone();
    let acct_rm = acct.clone();
    let chat_rm = chat.clone();

    rsx! {
        div { class: "persona-allowlist-row",
            span { class: "persona-allowlist-account",
                title: "{acct}",
                {acct.chars().take(16).collect::<String>()}
            }
            span { class: "persona-allowlist-sep", "/" }
            span { class: "persona-allowlist-chat",
                title: "{chat}",
                {chat.chars().take(20).collect::<String>()}
            }
            div { class: "persona-allowlist-stepper",
                button {
                    class: "btn btn-icon stepper-btn",
                    disabled: cap <= 1,
                    onclick: {
                        let a = acct.clone(); let c = chat.clone();
                        let on_cap_change = on_cap_change;
                        move |_| on_cap_change.call((a.clone(), c.clone(), (cap - 1).max(1)))
                    },
                    "−"
                }
                span { class: "stepper-value", "{cap}" }
                button {
                    class: "btn btn-icon stepper-btn",
                    disabled: cap >= 20,
                    onclick: {
                        let a = acct.clone(); let c = chat.clone();
                        let on_cap_change = on_cap_change;
                        move |_| on_cap_change.call((a.clone(), c.clone(), (cap + 1).min(20)))
                    },
                    "+"
                }
                span { class: "stepper-label", {t("persona-outbound-msgs-per-day")} }
            }
            button {
                class: "btn btn-icon btn-danger-icon persona-allowlist-remove",
                title: t("persona-outbound-remove-entry"),
                onclick: move |_| on_remove.call((acct_rm.clone(), chat_rm.clone())),
                "×"
            }
        }
    }
}

// ─── PersonaOutboundAllowlistEditor ──────────────────────────────────────────

/// Props:
/// - `persona_slug`        — slug of the persona being edited.
/// - `rate_limit_per_hour` — used to decide whether to show the dry-run banner.
/// - `quiet_hours_disabled`— initial value from the persona row; updates fire
///   `on_quiet_hours_changed` so the parent can persist.
/// - `on_quiet_hours_changed` — called when the user toggles quiet-hours.
#[derive(Props, Clone, PartialEq)]
pub struct PersonaOutboundAllowlistEditorProps {
    pub persona_slug: String,
    pub rate_limit_per_hour: i64,
    pub quiet_hours_disabled: bool,
    pub on_quiet_hours_changed: EventHandler<bool>,
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonaOutboundAllowlistEditor(
    props: PersonaOutboundAllowlistEditorProps,
) -> Element {
    let persona_slug = props.persona_slug.clone();
    let rate_limit = props.rate_limit_per_hour;
    let mut entries: Signal<Vec<OutboundAllowEntry>> = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut op_error: Signal<Option<String>> = use_signal(|| None);
    let mut quiet_hours_disabled = use_signal(|| props.quiet_hours_disabled);

    // New-row add form state.
    let mut new_account = use_signal(String::new);
    let mut new_chat = use_signal(String::new);

    // G.1 + G.2 — load existing entries on mount or slug change.
    let slug_dep = persona_slug.clone();
    use_reactive_effect(slug_dep, move |slug_load| {
        spawn(async move {
            match call_persona_mcp(
                "meta_persona_list_outbound_allows",
                serde_json::json!({ "slug": slug_load }),
            )
            .await
            {
                Ok(json) => {
                    let rows: Vec<OutboundAllowEntry> = json
                        .as_array()
                        .map(|arr| arr.iter().filter_map(OutboundAllowEntry::from_json).collect())
                        .unwrap_or_default();
                    entries.set(rows);
                }
                Err(e) => {
                    tracing::warn!("list_outbound_allows failed: {e}");
                    op_error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    // Handler: change daily cap for one entry.
    let slug_cap = persona_slug.clone();
    let on_cap_change = move |(acct, chat, new_cap): (String, String, i64)| {
        let slug_inner = slug_cap.clone();
        spawn(async move {
            let res = call_persona_mcp(
                "meta_persona_set_outbound_allow",
                serde_json::json!({
                    "slug": slug_inner,
                    "account_id": acct,
                    "chat_id": chat,
                    "max_messages_per_day": new_cap,
                }),
            )
            .await;
            if let Err(e) = res {
                tracing::warn!("set_outbound_allow failed: {e}");
                op_error.set(Some(e));
                return;
            }
            // Update local state optimistically.
            let mut cur = entries.read().clone();
            for row in &mut cur {
                if row.account_id == acct && row.chat_id == chat {
                    row.max_messages_per_day = new_cap;
                }
            }
            entries.set(cur);
        });
    };

    // Handler: remove one entry.
    let slug_rm = persona_slug.clone();
    let on_remove = move |(acct, chat): (String, String)| {
        let slug_inner = slug_rm.clone();
        spawn(async move {
            let res = call_persona_mcp(
                "meta_persona_remove_outbound_allow",
                serde_json::json!({
                    "slug": slug_inner,
                    "account_id": acct,
                    "chat_id": chat,
                }),
            )
            .await;
            if let Err(e) = res {
                tracing::warn!("remove_outbound_allow failed: {e}");
                op_error.set(Some(e));
                return;
            }
            let cur: Vec<_> = entries
                .read()
                .iter()
                .filter(|r| !(r.account_id == acct && r.chat_id == chat))
                .cloned()
                .collect();
            entries.set(cur);
        });
    };

    // Handler: add new entry.
    let slug_add = persona_slug.clone();
    let on_add = move |_| {
        let acct = new_account.read().trim().to_string();
        let chat = new_chat.read().trim().to_string();
        if acct.is_empty() || chat.is_empty() {
            return;
        }
        let slug_inner = slug_add.clone();
        spawn(async move {
            let res = call_persona_mcp(
                "meta_persona_set_outbound_allow",
                serde_json::json!({
                    "slug": slug_inner,
                    "account_id": acct,
                    "chat_id": chat,
                    "max_messages_per_day": 1_i64,
                }),
            )
            .await;
            match res {
                Ok(_) => {
                    let mut cur = entries.read().clone();
                    // Upsert local state.
                    if !cur.iter().any(|r| r.account_id == acct && r.chat_id == chat) {
                        cur.push(OutboundAllowEntry {
                            account_id: acct,
                            chat_id: chat,
                            max_messages_per_day: 1,
                        });
                        entries.set(cur);
                    }
                    new_account.set(String::new());
                    new_chat.set(String::new());
                }
                Err(e) => {
                    tracing::warn!("set_outbound_allow (add) failed: {e}");
                    op_error.set(Some(e));
                }
            }
        });
    };

    // G.6 — quiet-hours toggle handler.
    let on_quiet_hours_changed = props.on_quiet_hours_changed;
    let on_quiet_toggle = move |_| {
        let next = !*quiet_hours_disabled.read();
        quiet_hours_disabled.set(next);
        on_quiet_hours_changed.call(next);
    };

    rsx! {
        div { class: "persona-modal-section persona-outbound-section",

            // G.4 — Dry-run banner (rate_limit_per_hour == 0).
            if rate_limit == 0 {
                DryRunBanner {}
            }

            // G.6 — Quiet-hours toggle.
            div { class: "settings-toggle-row",
                label { class: "settings-toggle-label", {t("persona-outbound-quiet-hours-label")} }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        // Disabled = normal quiet hours; enabled = quiet-hours OFF.
                        checked: *quiet_hours_disabled.read(),
                        onchange: on_quiet_toggle,
                    }
                    span { class: "toggle-slider" }
                }
                span { class: "settings-toggle-hint", {t("persona-outbound-quiet-hours-hint")} }
            }

            // G.1 — Allowlist heading.
            h5 { class: "persona-outbound-allowlist-title", {t("persona-outbound-allowlist-title")} }

            if *loading.read() {
                div { class: "agent-panel-empty-state", {t("persona-loading")} }
            } else {
                div { class: "persona-allowlist-list",
                    if entries.read().is_empty() {
                        div { class: "agent-panel-empty-state", {t("persona-outbound-allowlist-empty")} }
                    } else {
                        for entry in entries.read().clone() {
                            AllowRow {
                                key: "{entry.account_id}/{entry.chat_id}",
                                entry: entry.clone(),
                                on_cap_change: on_cap_change.clone(),
                                on_remove: on_remove.clone(),
                            }
                        }
                    }
                }

                // Add-entry form.
                div { class: "persona-allowlist-add-row",
                    input {
                        r#type: "text",
                        class: "settings-input settings-input-sm",
                        placeholder: t("persona-outbound-account-id-placeholder"),
                        value: "{new_account.read()}",
                        oninput: move |e| new_account.set(e.value()),
                    }
                    input {
                        r#type: "text",
                        class: "settings-input settings-input-sm",
                        placeholder: t("persona-outbound-chat-id-placeholder"),
                        value: "{new_chat.read()}",
                        oninput: move |e| new_chat.set(e.value()),
                    }
                    button {
                        class: "btn btn-sm btn-secondary",
                        onclick: on_add,
                        "+ {t(\"persona-outbound-add-entry\")}"
                    }
                }
            }

            if let Some(err) = op_error.read().clone() {
                div { class: "persona-save-error", "{err}" }
            }
        }
    }
}
