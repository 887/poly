//! Discord backend UI overrides.
//!
//! Discord-specific UI components that differ from the common implementation.
//! Includes context menu items unique to Discord servers (Server Boost,
//! Sticker management, integration settings, etc.).
//!
//! ## Architecture
//! This module is part of the per-backend UI layer:
//! ```text
//! ui/account/discord/  ← YOU ARE HERE — Discord backend overrides
//! ui/account/common/   ← Shared UI components (used as fallback)
//! ```
//!
//! ## Feature gate
//! This module is only compiled when the `discord` feature is enabled.
//!
//! ## Status
//! Discord-specific menu items now ship via the `client-menus` WIT interface
//! (see `docs/plans/plan-client-ui-surface.md`); the old Rust
//! `context_menu` module was removed in WP 2.
