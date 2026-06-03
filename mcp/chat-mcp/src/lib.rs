//! Public API for `poly-chat-mcp` — exposed for integration tests.
// The rest of this repo uses mod.rs; renaming all modules would be a
// pointless churn — allow the existing structure.
#![allow(clippy::mod_module_files)]

pub mod memory;
pub mod events;
pub mod persona;
pub mod persona_audit_prune;
pub mod state;
pub mod tools;
pub mod typing_simulation;
