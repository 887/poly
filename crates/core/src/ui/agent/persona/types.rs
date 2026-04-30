//! Shared data types for persona UI components.

use serde::{Deserialize, Serialize};

/// Summary row returned by `meta_persona_list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonaSummary {
    pub slug: String,
    pub name: String,
    pub avatar_emoji: String,
    pub enabled: bool,
    pub proactivity: String,
    pub last_run_at: Option<String>,
}

/// Full persona row returned by `meta_persona_get`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonaDetail {
    pub slug: String,
    pub name: String,
    pub avatar_emoji: String,
    pub system_prompt: String,
    pub style_notes: Option<String>,
    pub heartbeat_interval_secs: Option<i64>,
    pub proactivity: String,
    pub rate_limit_per_hour: i64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub sources: Vec<PersonaSource>,
    #[serde(default)]
    pub tool_whitelist: Vec<String>,
    #[serde(default)]
    pub pinned_facts: Vec<PersonaFact>,
    #[serde(default)]
    pub recent_audit: Vec<AuditRow>,
    /// G.6 — when true, the 22:00-08:00 outbound quiet-hours block is lifted
    /// for this persona.
    #[serde(default)]
    pub quiet_hours_disabled: bool,
}

/// A source binding row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonaSource {
    pub id: i64,
    pub account_id: String,
    pub selector_kind: String,
    pub selector_value: Option<String>,
    pub include: i64,
}

/// A persona fact row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonaFact {
    pub id: i64,
    pub category: Option<String>,
    pub fact_text: String,
    pub pinned: bool,
    pub created_at: String,
}

/// An audit log row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditRow {
    pub id: i64,
    pub occurred_at: String,
    pub actor: String,
    pub action: String,
    pub target_account: Option<String>,
    pub target_chat: Option<String>,
    pub result: String,
    pub error_msg: Option<String>,
    /// Phase H.2 — JSON payload of the audit row (raw stringified JSON).
    /// Renders as a collapsible inline viewer in `PersonaAuditPanel`.
    #[serde(default)]
    pub payload_json: Option<String>,
}

/// Source include state for the per-account editor tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeState {
    /// Explicit allow (include=1).
    Allow,
    /// No row — inherits from parent.
    Inherit,
    /// Explicit deny (include=0).
    Deny,
}

impl IncludeState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Allow => "Allow",
            Self::Inherit => "Inherit",
            Self::Deny => "Deny",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::Allow => Self::Inherit,
            Self::Inherit => Self::Deny,
            Self::Deny => Self::Allow,
        }
    }
}

/// Parse a persona summary list from the MCP result JSON.
pub fn parse_persona_list(json: &serde_json::Value) -> Vec<PersonaSummary> {
    // `meta_persona_list` returns a bare JSON array of persona rows. Older
    // tool versions wrapped them as `{"personas": [...]}` — accept both.
    let arr_opt = json
        .as_array()
        .or_else(|| json.get("personas").and_then(|v| v.as_array()));
    arr_opt
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a full persona detail from the MCP result JSON.
pub fn parse_persona_detail(json: &serde_json::Value) -> Option<PersonaDetail> {
    serde_json::from_value(json.clone()).ok()
}
