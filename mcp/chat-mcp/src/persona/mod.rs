//! Phase C — Persona context builder.
//!
//! Exposes `PersonaContextRequest`, `PersonaContextBundle`, and the
//! `build()` orchestrator.  Tests live in `context.rs`.

pub mod context;

pub use context::{PersonaContextBundle, PersonaContextRequest, PersonaBackendProvider, build};
