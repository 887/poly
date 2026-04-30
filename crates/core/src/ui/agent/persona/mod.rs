//! Persona UI module — Phase D of `plan-meta-personalities.md`.
//!
//! Directory layout:
//! - `mcp.rs`                   — thin async wrapper for `meta_persona_*` MCP calls
//! - `types.rs`                 — shared data types (PersonaSummary, PersonaDetail, …)
//! - `list_panel.rs`            — PersonaListPanel (mounts inside AgentPanel)
//! - `edit_modal.rs`            — PersonaEditModal (create/edit, collapsible sections)
//! - `sources_editor.rs`        — PersonaSourcesEditor (per-account tree, 3-state pill)
//! - `tool_whitelist_editor.rs` — PersonaToolWhitelistEditor (checkbox grid by category)
//! - `route.rs`                 — PersonaManagementRoute (/agent/personas)

mod edit_modal;
mod list_panel;
mod mcp;
mod route;
mod sources_editor;
mod tool_whitelist_editor;
mod types;

pub use edit_modal::PersonaEditModal;
pub use list_panel::PersonaListPanel;
pub use route::PersonaManagementRoute;
