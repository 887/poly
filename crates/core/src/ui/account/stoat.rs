//! Stoat (formerly Revolt) backend UI overrides.
//!
//! Stoat-specific UI components that differ from the common implementation.
//! Includes context menu items unique to Stoat servers (bot management,
//! webhook configuration, etc.).
//!
//! ## Architecture
//! This module is part of the per-backend UI layer:
//! ```text
//! ui/account/stoat/    ← YOU ARE HERE — Stoat backend overrides
//! ui/account/common/   ← Shared UI components (used as fallback)
//! ```
//!
//! ## Feature gate
//! This module is only compiled when the `stoat` feature is enabled.
//!
//! ## Status
//! Stoat-specific menu items now ship via the `client-menus` WIT interface
//! (see `docs/plans/plan-client-ui-surface.md`); the old Rust
//! `context_menu` module was removed in WP 2.
