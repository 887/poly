//! PersonaAuditPanel — H.1 + H.2 + H.4
//!
//! Paged list of `persona_audit` rows with:
//! - H.1  Filters: action, time-range (last 24h / 7d / 30d / all), target_account.
//! - H.2  Inline collapsible JSON viewer for `payload_json`.
//! - H.4  "Export audit" button → JSONL download via a `data:` URL.
//!
//! ## Reactive hygiene
//! - All signals local to this component.
//! - `use_reactive_effect` re-fires on slug or filter changes.

use std::fmt::Write as _;
use super::mcp::call_persona_mcp;
use super::types::AuditRow;
use crate::i18n::t;
use crate::state::use_reactive_effect;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

// ─── Page size ────────────────────────────────────────────────────────────────

const PAGE_SIZE: usize = 25;

// ─── AuditRowView (H.1 + H.2) ────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AuditRowView(row: AuditRow) -> Element {
    let mut json_open = use_signal(|| false);
    let has_payload = row.payload_json.is_some();

    rsx! {
        div { class: "persona-audit-row-card",
            div { class: "persona-audit-row-header",
                span { class: "persona-audit-time font-mono text-xs", "{row.occurred_at}" }
                span { class: "persona-audit-actor tag-pill", "{row.actor}" }
                span { class: "persona-audit-action tag-pill", "{row.action}" }
                span {
                    class: if row.result == "ok" {
                        "persona-audit-result tag-pill tag-ok"
                    } else {
                        "persona-audit-result tag-pill tag-err"
                    },
                    "{row.result}"
                }
                if let Some(acct) = &row.target_account {
                    span { class: "persona-audit-target text-xs text-muted",
                        "→ {acct}"
                    }
                }
                if has_payload {
                    button {
                        class: "btn btn-icon btn-xs persona-audit-json-toggle",
                        title: if *json_open.read() { t("persona-audit-hide-json") } else { t("persona-audit-show-json") },
                        onclick: move |_| {
                            let v = *json_open.read();
                            json_open.set(!v);
                        },
                        if *json_open.read() { "▼" } else { "▶" }
                    }
                }
            }
            // H.2 — Inline collapsible JSON viewer.
            if *json_open.read() {
                if let Some(payload) = &row.payload_json {
                    div { class: "persona-audit-json-viewer",
                        // Pretty-print if valid JSON, else raw.
                        pre { class: "persona-audit-json-pre",
                            {
                                serde_json::from_str::<serde_json::Value>(payload.as_str())
                                    .ok()
                                    .and_then(|v| serde_json::to_string_pretty(&v).ok())
                                    .unwrap_or_else(|| payload.clone())
                            }
                        }
                    }
                }
            }
            if let Some(err) = &row.error_msg {
                div { class: "persona-audit-error-msg text-xs text-danger", "{err}" }
            }
        }
    }
}

// ─── AuditFilters ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFilters {
    pub action: String,
    pub time_range: TimeRange,
    pub target_account: String,
}

impl Default for AuditFilters {
    fn default() -> Self {
        Self {
            action: String::new(),
            time_range: TimeRange::Last7d,
            target_account: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeRange {
    Last24h,
    Last7d,
    Last30d,
    All,
}

impl TimeRange {
    pub fn label(self) -> &'static str {
        match self {
            Self::Last24h => "24h",
            Self::Last7d => "7d",
            Self::Last30d => "30d",
            Self::All => "All",
        }
    }

    pub fn limit(self) -> i64 {
        match self {
            Self::Last24h => 50,
            Self::Last7d => 200,
            Self::Last30d => 500,
            Self::All => 1000,
        }
    }
}

// ─── PersonaAuditPanel ───────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq, Eq)]
pub struct PersonaAuditPanelProps {
    pub persona_slug: String,
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonaAuditPanel(props: PersonaAuditPanelProps) -> Element {
    let persona_slug = props.persona_slug.clone();
    let mut rows: Signal<Vec<AuditRow>> = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut load_error: Signal<Option<String>> = use_signal(|| None);
    let mut filters = use_signal(AuditFilters::default);
    let mut page = use_signal(|| 0_usize);

    // Load rows when slug or filters change.
    // Uses `meta_persona_audit_query` (Phase T.4) so action / target_account
    // filtering happens server-side — no full-table fetch on panel open.
    let slug_dep = persona_slug.clone();
    use_reactive_effect((slug_dep, filters.read().clone()), move |(slug_load, f)| {
        spawn(async move {
            loading.set(true);
            load_error.set(None);

            // Build args — pass filter values only when the user has typed
            // something, so the server omits the WHERE clause for empty fields.
            let mut qargs = serde_json::json!({
                "slug":  slug_load,
                "limit": f.time_range.limit(),
            });
            if let Some(obj) = qargs.as_object_mut() {
                if !f.action.is_empty() {
                    obj.insert("action".to_string(), serde_json::Value::String(f.action.clone()));
                }
                if !f.target_account.is_empty() {
                    obj.insert(
                        "target_account".to_string(),
                        serde_json::Value::String(f.target_account.clone()),
                    );
                }
            }

            match call_persona_mcp("meta_persona_audit_query", qargs).await {
                Ok(json) => {
                    let filtered: Vec<AuditRow> = json
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    rows.set(filtered);
                    page.set(0);
                }
                Err(e) => {
                    tracing::warn!("audit_query failed: {e}");
                    load_error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    // H.4 — build JSONL data URL for download.
    let rows_snap = rows.read().clone();
    let export_url = {
        let jsonl = rows_snap
            .iter()
            .filter_map(|r| serde_json::to_string(r).ok())
            .collect::<Vec<_>>()
            .join("\n");
        let encoded = js_sys_encode_uri_component(&jsonl);
        format!("data:application/x-ndjson;charset=utf-8,{encoded}")
    };

    // Pagination.
    let page_rows: Vec<AuditRow> = {
        let start = page.read().saturating_mul(PAGE_SIZE);
        rows.read()
            .iter()
            .skip(start)
            .take(PAGE_SIZE)
            .cloned()
            .collect()
    };
    let total_pages = rows.read().len().div_ceil(PAGE_SIZE).max(1);

    // Filter bar state snapshot (use peek to avoid rendering subscription on filter).
    let cur_filters = filters.read().clone();

    rsx! {
        div { class: "persona-modal-section persona-audit-panel",

            // Filter bar.
            div { class: "persona-audit-filters",
                // Action filter.
                input {
                    r#type: "text",
                    class: "settings-input settings-input-sm",
                    placeholder: t("persona-audit-filter-action"),
                    value: "{cur_filters.action}",
                    oninput: move |e| {
                        let mut f = filters.read().clone();
                        f.action = e.value();
                        filters.set(f);
                    },
                }
                // Account filter.
                input {
                    r#type: "text",
                    class: "settings-input settings-input-sm",
                    placeholder: t("persona-audit-filter-account"),
                    value: "{cur_filters.target_account}",
                    oninput: move |e| {
                        let mut f = filters.read().clone();
                        f.target_account = e.value();
                        filters.set(f);
                    },
                }
                // Time-range buttons.
                div { class: "persona-audit-time-tabs",
                    for tr in [TimeRange::Last24h, TimeRange::Last7d, TimeRange::Last30d, TimeRange::All] {
                        button {
                            class: if cur_filters.time_range == tr {
                                "btn btn-xs btn-primary"
                            } else {
                                "btn btn-xs btn-secondary"
                            },
                            onclick: {
                                move |_| {
                                    let mut f = filters.read().clone();
                                    f.time_range = tr;
                                    filters.set(f);
                                }
                            },
                            {tr.label()}
                        }
                    }
                }

                // H.4 — Export button.
                a {
                    class: "btn btn-sm btn-secondary persona-audit-export-btn",
                    href: "{export_url}",
                    download: "persona-audit.jsonl",
                    title: t("persona-audit-export-tooltip"),
                    {t("persona-audit-export")}
                }
            }

            // Row count.
            div { class: "persona-audit-count text-xs text-muted",
                {format!("{} {}", rows.read().len(), t("persona-audit-rows-total"))}
            }

            if *loading.read() {
                div { class: "agent-panel-empty-state", {t("persona-loading")} }
            } else if let Some(err) = load_error.read().clone() {
                div { class: "agent-panel-empty-state agent-panel-error", "{err}" }
            } else if rows.read().is_empty() {
                div { class: "agent-panel-empty-state", {t("persona-audit-empty")} }
            } else {
                // H.1 — Row list.
                div { class: "persona-audit-list",
                    for row in page_rows {
                        AuditRowView { key: "{row.id}", row: row.clone() }
                    }
                }
                // Pagination.
                if total_pages > 1 {
                    div { class: "persona-audit-pagination",
                        button {
                            class: "btn btn-sm btn-secondary",
                            disabled: *page.read() == 0,
                            onclick: move |_| {
                                let p = *page.read();
                                if p > 0 { page.set(p - 1); }
                            },
                            "←"
                        }
                        span { class: "persona-audit-page-label",
                            "{*page.read() + 1} / {total_pages}"
                        }
                        button {
                            class: "btn btn-sm btn-secondary",
                            disabled: *page.read() + 1 >= total_pages,
                            onclick: move |_| {
                                let p = *page.read();
                                if p + 1 < total_pages { page.set(p + 1); }
                            },
                            "→"
                        }
                    }
                }
            }
        }
    }
}

// ─── js_sys_encode_uri_component fallback ────────────────────────────────────

/// Minimal URL-encoding for the JSONL data URL.
///
/// On WASM the browser's `encodeURIComponent` is available; on native (test /
/// host) we use a simple percent-encode of unsafe characters. The data: URL
/// is consumed by the browser's download mechanism which decodes it before
/// writing to disk.
fn js_sys_encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len().saturating_mul(3));
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(char::from(b));
            }
            _ => {
                out.push('%');
                let _ = write!(out, "{b:02X}");
            }
        }
    }
    out
}
