//! Poly native server backend UI overrides.
//!
//! UI components specific to Poly's own server protocol. Includes context
//! menu items for Poly-native servers (server administration, federation
//! settings, etc.).
//!
//! ## Architecture
//! This module is part of the per-backend UI layer:
//! ```text
//! ui/account/poly_native/  ← YOU ARE HERE — Poly native server overrides
//! ui/account/common/       ← Shared UI components (used as fallback)
//! ```

pub mod context_menu;
