//! Phase C — Persona context builder.
//! Phase F — Heartbeat scheduler.
//!
//! Exposes `PersonaContextRequest`, `PersonaContextBundle`, the
//! `build()` orchestrator, and the `HeartbeatRegistry`.

pub mod context;
pub mod heartbeat;

pub use context::{
    PersonaContextBundle, PersonaContextRequest, PersonaBackendProvider, build,
    PersonaSourceRow, is_chat_included,
};
pub use heartbeat::{HeartbeatRegistry, HeartbeatOutput, summarise, in_quiet_hours};
