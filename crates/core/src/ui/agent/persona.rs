//! Persona UI module — Phases D+E+G+H of `plan-meta-personalities.md`.
//!
//! Directory layout:
//! - `mcp.rs`                       — thin async wrapper for `meta_persona_*` MCP calls
//! - `types.rs`                     — shared data types (PersonaSummary, PersonaDetail, …)
//! - `list_panel.rs`                — PersonaListPanel (mounts inside AgentPanel)
//! - `edit_modal.rs`                — PersonaEditModal (create/edit, collapsible sections)
//! - `sources_editor.rs`            — PersonaSourcesEditor (per-account tree, 3-state pill)
//! - `tool_whitelist_editor.rs`     — PersonaToolWhitelistEditor (checkbox grid by category)
//! - `route.rs`                     — PersonasSection (inline persona panel rendered inside AgentPage)
//! - `talk_to_overlay.rs`           — PersonaTalkToOverlay + TalkSession (Phase E)
//! - `outbound_allowlist_editor.rs` — G.1+G.2+G.4+G.6 outbound UI
//! - `audit_panel.rs`               — H.1+H.2+H.4 audit log panel
//! - `confirm_modals.rs`            — G.5+H.6 typed-confirm flows
//! - `data_exposure_summary.rs`     — H.5 per-persona exposure widget

mod audit_panel;
mod confirm_modals;
mod data_exposure_summary;
mod edit_modal;
mod list_panel;
pub(crate) mod mcp;
mod outbound_allowlist_editor;
mod route;
mod sources_editor;
mod tool_whitelist_editor;
pub mod talk_to_overlay;
mod types;

pub use edit_modal::PersonaEditModal;
pub use list_panel::PersonaListPanel;
pub use route::PersonasSection;
pub use talk_to_overlay::{PersonaTalkToOverlay, TalkSession};
pub use types::PersonaSummary;
