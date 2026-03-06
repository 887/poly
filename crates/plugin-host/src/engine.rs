//! Wasmtime engine setup and WIT-generated Component Model bindings.
//!
//! Uses the `wasmtime::component::bindgen!` macro to generate typed Rust
//! wrappers from the WIT interface at `wit/messenger-plugin.wit`.
//!
//! The generated code provides:
//! - `MessengerPlugin` — instantiation + access to guest exports
//! - `Host` / `HostHostApi` traits — implement these for the host imports
//! - All WIT record/enum/variant types as Rust structs/enums

use wasmtime::component::Component;
use wasmtime::{Config, Engine, Result as WasmResult};

// Generate host-side bindings from the WIT world definition.
//
// This produces:
// - `MessengerPlugin` struct with `instantiate_async` and accessor methods
// - Traits for each imported interface (HostApi → `poly::messenger::host_api::Host`)
// - Type definitions for all WIT records, enums, variants
wasmtime::component::bindgen!({
    world: "messenger-plugin",
    path: "../../wit",
    imports: { default: async },
    exports: { default: async },
    require_store_data_send: true,
});

/// Create a configured wasmtime [`Engine`] for plugin execution.
///
/// Enables:
/// - async support (required for async host calls)
/// - component model
/// - fuel metering (optional, for limiting plugin execution time)
pub fn create_engine() -> WasmResult<Engine> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    // Fuel-based metering lets us limit runaway plugins
    config.consume_fuel(true);
    Engine::new(&config)
}

/// Load a WASM component from raw bytes.
///
/// The bytes should be a valid Component Model binary (not a core module).
/// Use `cargo component build` to produce these from guest crates.
pub fn load_component(engine: &Engine, bytes: &[u8]) -> WasmResult<Component> {
    Component::from_binary(engine, bytes)
}

/// Load a WASM component from a file path.
///
/// Convenience wrapper for loading plugin `.wasm` files from disk.
pub fn load_component_from_file(engine: &Engine, path: &std::path::Path) -> WasmResult<Component> {
    Component::from_file(engine, path)
}
