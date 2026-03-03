//! Demo backend UI overrides.
//!
//! The demo backend is used for UI testing with mock data. Its UI overrides
//! are minimal — mostly using common components with demo-specific context
//! menu items.
//!
//! ## Architecture
//! This module is part of the per-backend UI layer:
//! ```text
//! ui/account/demo/     ← YOU ARE HERE — Demo backend overrides
//! ui/account/common/   ← Shared UI components (used as fallback)
//! ```
//!
//! ## Feature gate
//! This module is only compiled when the `demo` feature is enabled.

pub mod context_menu;
