//! End-to-end client interface tests through the WASM plugin host.
//!
//! Each client has a feature-gated test module. Enable individual clients:
//!
//! ```sh
//! # Test only Demo:
//! cargo test -p poly-plugin-loader-tests --features test-demo -- client_e2e
//!
//! # Test all clients:
//! cargo test -p poly-plugin-loader-tests --all-features -- client_e2e
//! ```
//!
//! ## Prerequisites
//!
//! Build the WASM plugin binaries before running:
//! ```sh
//! cargo component build -p poly-demo -p poly-stoat -p poly-matrix \
//!     -p poly-discord -p poly-teams -p poly-server-client \
//!     --target wasm32-wasip2
//! ```

// Test binary — allow expect/unwrap for test assertions and setup
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(any(
    feature = "test-demo",
    feature = "test-stoat",
    feature = "test-matrix",
    feature = "test-discord",
    feature = "test-teams",
    feature = "test-server"
))]
mod harness;

#[cfg(feature = "test-demo")]
mod demo;

#[cfg(feature = "test-stoat")]
mod stoat;

#[cfg(feature = "test-matrix")]
mod matrix;

#[cfg(feature = "test-discord")]
mod discord;

#[cfg(feature = "test-teams")]
mod teams;

#[cfg(feature = "test-server")]
mod server;
