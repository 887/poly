//! Test support: re-exports from [`crate::mock_backend`] for use in integration
//! tests and doc examples.
//!
//! Mirrors the pattern in `crates/audio-backend/src/fake_backend.rs`:
//! a single public module callers import to get the full mock API without
//! needing to know the internal module layout.

pub use crate::mock_backend::{
    generate_gradient_frame, mock_stream, MockVideoBackend, MockVideoInputStream, MockVideoState,
};
