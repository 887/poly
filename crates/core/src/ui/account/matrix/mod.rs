//! Matrix backend UI overrides.
//!
//! Matrix-specific UI components that differ from the common implementation.
//! Includes context menu items unique to Matrix spaces (room directory,
//! E2EE verification flows, space hierarchy, etc.).
//!
//! ## Architecture
//! This module is part of the per-backend UI layer:
//! ```text
//! ui/account/matrix/   ← YOU ARE HERE — Matrix backend overrides
//! ui/account/common/   ← Shared UI components (used as fallback)
//! ```
//!
//! ## Feature gate
//! This module is only compiled when the `matrix` feature is enabled.
//!
//! ## Status
//! Matrix-specific menu items now ship via the `client-menus` WIT interface
//! (see `docs/plans/plan-client-ui-surface.md`); the old Rust
//! `context_menu` module was removed in WP 2.
