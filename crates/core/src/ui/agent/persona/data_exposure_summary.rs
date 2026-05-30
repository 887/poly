//! DataExposureSummary — H.5
//!
//! "Data exposure summary" widget shown in PersonaListPanel below each persona
//! row.  Displays "can read X channels across Y accounts" derived from the
//! persona's `persona_sources` rows.
//!
//! Loading is done once per panel render via `use_future` — same pattern as
//! PersonaListPanel itself. The widget is intentionally lightweight (no
//! pagination, no filter) because it is a summary, not a management UI.
//!
//! ## Reactive hygiene
//! Local `Signal<T>` only; no cross-component subscribers.

use super::mcp::call_persona_mcp;
use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

// ─── SourceSummary ────────────────────────────────────────────────────────────

/// Computed from `meta_persona_list_sources` result.
#[derive(Debug, Clone, Default, PartialEq)]
struct SourceSummary {
    channel_count: usize,
    account_count: usize,
}

fn compute_summary(json: &serde_json::Value) -> SourceSummary {
    let Some(arr) = json.as_array() else {
        return SourceSummary::default();
    };

    // Count INCLUDE rows (include == 1); collect unique account_ids.
    let mut accounts = std::collections::HashSet::new();
    let mut channels = 0usize;

    for row in arr {
        let include = row.get("include").and_then(serde_json::Value::as_i64).unwrap_or(0);
        if include != 1 {
            continue;
        }
        if let Some(acc) = row.get("account_id").and_then(|v| v.as_str()) {
            accounts.insert(acc.to_string());
        }
        channels = channels.saturating_add(1);
    }

    SourceSummary {
        channel_count: channels,
        account_count: accounts.len(),
    }
}

// ─── DataExposureSummary ─────────────────────────────────────────────────────

/// Compact one-line badge shown under each persona row in PersonaListPanel.
///
/// Renders a short human-readable summary of how much data this persona can
/// observe.  Intentionally non-interactive — the full source management is in
/// PersonaEditModal > Sources section.
#[derive(Props, Clone, PartialEq, Eq)]
pub struct DataExposureSummaryProps {
    pub persona_slug: String,
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn DataExposureSummary(props: DataExposureSummaryProps) -> Element {
    let persona_slug = props.persona_slug.clone();
    let mut summary: Signal<Option<SourceSummary>> = use_signal(|| None);
    let mut loading = use_signal(|| true);

    // One-shot load on mount (slug is static for this instance).
    use_future(move || {
        let slug = persona_slug.clone();
        async move {
            match call_persona_mcp(
                "meta_persona_list_sources",
                serde_json::json!({ "slug": slug }),
            )
            .await
            {
                Ok(json) => {
                    let s = compute_summary(&json);
                    summary.set(Some(s));
                }
                Err(e) => {
                    tracing::warn!("DataExposureSummary: list_sources failed: {e}");
                    summary.set(Some(SourceSummary::default()));
                }
            }
            loading.set(false);
        }
    });

    rsx! {
        span { class: "persona-data-exposure-summary text-xs text-muted",
            if *loading.read() {
                "…"
            } else if let Some(s) = summary.read().clone() {
                if s.channel_count == 0 {
                    {t("persona-exposure-no-sources")}
                } else {
                    {
                        format!(
                            "{}: {} {} / {} {}",
                            t("persona-exposure-label"),
                            s.channel_count,
                            t("persona-exposure-channels"),
                            s.account_count,
                            t("persona-exposure-accounts"),
                        )
                    }
                }
            }
        }
    }
}
