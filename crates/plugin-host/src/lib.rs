//! # poly-plugin-host — WASM Component Model Plugin Host
//!
//! **Dynamically-linked** runtime for loading and executing messenger backend
//! plugins compiled as WASM Component Model binaries (.wasm).
//!
//! ## Why a separate crate?
//!
//! Wasmtime 42.x compilation is expensive (~minutes from scratch). By isolating
//! it in a Rust `dylib` crate, changes to `poly-core` (UI, routing, state)
//! **never** trigger wasmtime recompilation. The final binary links against
//! the shared library (.so/.dll/.dylib) as a fast reference — not a full
//! static link.
//!
//! **DECISION(D22):** Dynamic linking boundary for wasmtime isolation.
//!
//! ## Architecture
//!
//! ```text
//!   poly-core  ──(dylib)──▶  poly-plugin-host.so
//!                                  │
//!                                  ├── engine.rs     (wasmtime Engine + WIT bindgen)
//!                                  ├── host_impl.rs  (host-api: HTTP, WS, storage, log)
//!                                  ├── bridge.rs     (WIT ↔ poly-client type conversion)
//!                                  └── registry.rs   (PluginRegistry + PluginBackend)
//! ```
//!
//! ## Modules
//!
//! - [`engine`] — Wasmtime engine setup and WIT-generated bindings
//! - [`host_impl`] — Host-side implementation of the `host-api` imports
//! - [`bridge`] — Type conversion between WIT types and `poly-client` types
//! - [`registry`] — Plugin registry: loading, management, lifecycle
//!
//! ## Usage
//!
//! ```rust,ignore
//! use poly_plugin_host::PluginRegistry;
//!
//! let mut registry = PluginRegistry::new()?;
//! registry.load_from_file("demo", &Path::new("plugins/poly_demo.wasm"))?;
//! let backend = registry.instantiate("demo").await?;
//!
//! // backend implements ClientBackend — use like any native backend
//! let servers = backend.get_servers().await?;
//! ```

pub mod bridge;
pub mod engine;
pub mod host_impl;
pub mod registry;

pub use registry::{PluginBackend, PluginRegistry, SettingDescriptor, SettingKind};
