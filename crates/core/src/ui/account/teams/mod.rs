//! Microsoft Teams backend UI overrides.
//!
//! Teams-specific UI components that differ from the common implementation.
//! Includes context menu items unique to Teams (meeting scheduling,
//! file sharing, app/connector management, etc.).
//!
//! ## Architecture
//! This module is part of the per-backend UI layer:
//! ```text
//! ui/account/teams/    ← YOU ARE HERE — Teams backend overrides
//! ui/account/common/   ← Shared UI components (used as fallback)
//! ```
//!
//! ## Feature gate
//! This module is only compiled when the `teams` feature is enabled.

pub mod context_menu;
