//! Wry sandbox wiring for poly-desktop.
//!
//! Phase A of `docs/plans/plan-host-sandbox-impl.md`.
//!
//! Re-exports `advertised_host_caps` from `poly_host_sandbox`. The
//! `wry-sandbox` feature on `poly-host-sandbox` both enables the real
//! `WrySandbox` implementation and makes `advertised_host_caps()` return
//! `[SandboxBrowser]`.
//!
//! This module is compiled only for native targets (never wasm32).

pub use poly_host_sandbox::advertised_host_caps;
