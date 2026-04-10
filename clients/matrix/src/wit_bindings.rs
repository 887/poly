//! WIT bindings for the Matrix messenger plugin.
//!
//! This module isolates the `wit-bindgen` code generation, which produces unsafe FFI stubs.
//! The `#![allow(unsafe_code)]` is confined to this module only.

#![allow(unsafe_code)]

// EXCEPTION: unsafe_code is required here because wit-bindgen's generate!()
// macro produces FFI stubs with #[export_name], unsafe fn, and unsafe blocks.
// This is generated code for the WASM Component Model ABI — there is no safe
// alternative. This module is only compiled on wasm32-wasip2 targets
// (cfg-gated in lib.rs).

wit_bindgen::generate!({
    world: "messenger-plugin",
    path: "../../wit",
});

pub use exports::poly::messenger::messenger_client::Guest;
pub use exports::poly::messenger::plugin_metadata::Guest as PluginMetadataGuest;
pub use exports::poly::messenger::plugin_metadata::PluginManifest;
pub use exports::poly::messenger::plugin_metadata::SettingDescriptor;
pub use poly::messenger::types as wit;
