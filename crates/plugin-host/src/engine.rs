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

/// Fuel granted to a guest before every host → guest call.
///
/// Fuel is the only wall-clock bound on a guest: a plugin that spins forever
/// traps once this budget is exhausted instead of wedging the host thread.
/// Every call site must re-arm it — see `registry::refuel!`.
pub const CALL_FUEL: u64 = 1_000_000_000;

/// Apply Poly's WASM sandbox policy to `config`.
///
/// **The sandbox is defined here, not by `Config::new()`'s defaults.** Every
/// proposal is set explicitly — enabled ones because `poly:messenger@0.1.0`
/// guests need them, disabled ones because they do not. Inheriting upstream
/// defaults is what silently widened this boundary across the wasmtime
/// 45 → 47 bump: 46 turned on component-model-async (and WASI 0.3.0), and 47
/// turned on the GC and exception-handling proposals, so unchanged source
/// began accepting three proposals it had previously rejected. Listing every
/// flag means a future default flip is a no-op here rather than a silent
/// widening. `engine::tests::rejects_*` fail if any of this regresses.
///
/// Guests are `cargo component` builds of Rust for `wasm32-wasip2`. That
/// toolchain emits only the WebAssembly 2.0 core feature set (plus the
/// component model itself), which is exactly what stays enabled below.
fn apply_sandbox_policy(config: &mut Config) {
    // ── Required ─────────────────────────────────────────────────────────
    // The component model is the plugin ABI itself.
    config.wasm_component_model(true);
    // Emitted unconditionally by LLVM for `wasm32-*`: `memory.copy`/`fill`,
    // multi-value returns (the canonical ABI relies on them), and `funcref`
    // tables for indirect calls.
    config.wasm_bulk_memory(true);
    config.wasm_multi_value(true);
    config.wasm_reference_types(true);
    // Deterministic, and emitted by any guest built with `+simd128`.
    config.wasm_simd(true);
    // Fuel-based metering lets us bound runaway plugins.
    config.consume_fuel(true);

    // ── Denied core-wasm proposals ───────────────────────────────────────
    // Each of these is on by default in wasmtime 47 (they are part of
    // `WasmFeatures::WASM3`) or reachable via a crate feature, and none is
    // reachable from a stock `cargo component` build.
    //
    // GC + typed function references: `struct`/`array`/`i31` heap types and
    // `call_ref`. Rust emits neither, and the GC heap is a large, young
    // allocator surface (47 also swapped in a copying collector).
    config.wasm_gc(false);
    config.wasm_function_references(false);
    // Exception handling: `tag`/`throw`/`try_table`. Our guests are
    // `panic = "abort"`; unwinding never crosses the guest boundary.
    config.wasm_exceptions(false);
    // Threads: shared memories and `atomic.*`. A plugin store is owned by one
    // tokio task behind a `Mutex`; shared linear memory would defeat that.
    config.wasm_threads(false);
    config.wasm_shared_everything_threads(false);
    // Stack switching: guest-controlled continuations, which would let a guest
    // suspend out of a fuel-metered call.
    config.wasm_stack_switching(false);
    // Relaxed SIMD: results are implementation-defined, so a guest could
    // observe host CPU differences. Plain SIMD above stays on.
    config.wasm_relaxed_simd(false);
    // 64-bit memories, several memories per module, non-4KiB page sizes, and
    // 128-bit arithmetic helpers: none are emitted by the Rust wasm backend.
    config.wasm_memory64(false);
    config.wasm_multi_memory(false);
    config.wasm_custom_page_sizes(false);
    config.wasm_wide_arithmetic(false);
    // Extended constant expressions in initializers — likewise unused.
    config.wasm_extended_const(false);
    // Tail calls: `return_call*`. Not emitted without `+tail-call`.
    config.wasm_tail_call(false);

    // ── Denied component-model sub-proposals ─────────────────────────────
    // `wit/messenger-plugin.wit` declares no `stream`, `future`,
    // `error-context`, `map` or fixed-length-list type, and no `async`
    // lift/lower. The `imports/exports: { default: async }` in the `bindgen!`
    // above is *host*-side async (wasmtime fibers), which is independent of
    // the guest-facing component-model-async proposal — so turning that
    // proposal off does not affect our async host functions.
    config.wasm_component_model_async(false);
    config.wasm_component_model_more_async_builtins(false);
    config.wasm_component_model_async_stackful(false);
    config.wasm_component_model_threading(false);
    config.wasm_component_model_error_context(false);
    config.wasm_component_model_gc(false);
    config.wasm_component_model_map(false);
    config.wasm_component_model_fixed_length_lists(false);
    config.wasm_component_model_implements(false);
}

/// Create a configured wasmtime [`Engine`] for plugin execution.
///
/// The sandbox policy is applied by [`apply_sandbox_policy`]; see that
/// function for what is enabled, what is denied, and why.
///
/// # Errors
///
/// Returns an error if the configured feature set is unsupported by the
/// selected compiler or target.
pub fn create_engine() -> WasmResult<Engine> {
    let mut config = Config::new();
    apply_sandbox_policy(&mut config);
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

// ─── Sandbox-policy tests (plan Phase D.2) ────────────────────────────────
//
// These are the regression net for `apply_sandbox_policy`. If a future
// wasmtime bump flips another proposal on by default, or someone deletes a
// `wasm_*(false)` line, the matching `rejects_*` test goes red instead of the
// widening landing unnoticed.

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::{apply_sandbox_policy, create_engine, CALL_FUEL};
    use wasmtime::component::Component;
    use wasmtime::{Config, Engine, Module, Store};

    /// An engine built from the policy under test.
    fn policy_engine() -> Engine {
        create_engine().expect("sandbox policy must produce a usable engine")
    }

    /// An engine built from wasmtime's stock defaults, for contrast.
    fn default_engine() -> Engine {
        Engine::new(&Config::new()).expect("default config must build")
    }

    /// Assert `wat` is denied by the policy **and** that it is denied *for the
    /// named proposal*.
    ///
    /// The second half matters: a malformed snippet would also be rejected,
    /// which would make the denial test pass vacuously and keep passing after
    /// someone deleted the corresponding `wasm_*(false)` line. So the same
    /// bytes are re-checked against a config that is the policy plus exactly
    /// this one proposal re-enabled — if that does not load, the snippet was
    /// testing nothing.
    fn assert_gated(proposal: &str, wat: &str, reenable: impl Fn(&mut Config)) {
        let engine = policy_engine();
        assert!(
            Module::new(&engine, wat).is_err(),
            "sandbox regression: the `{proposal}` proposal was accepted. Either \
             a `wasm_*(false)` line was removed from apply_sandbox_policy, or a \
             wasmtime bump re-enabled it by default and the explicit denial no \
             longer covers it."
        );

        let mut config = Config::new();
        apply_sandbox_policy(&mut config);
        reenable(&mut config);
        let widened = Engine::new(&config)
            .unwrap_or_else(|e| panic!("re-enabling `{proposal}` must still build: {e}"));
        if let Err(e) = Module::new(&widened, wat) {
            panic!(
                "this test is vacuous: the `{proposal}` snippet fails even with \
                 `{proposal}` enabled, so the denial above proved nothing. Fix \
                 the snippet: {e}"
            );
        }
    }

    /// Assert the policy engine still accepts `wat` (no over-narrowing).
    fn assert_accepted(what: &str, wat: &str) {
        let engine = policy_engine();
        if let Err(e) = Module::new(&engine, wat) {
            panic!("sandbox is too narrow: `{what}` must stay loadable, got: {e}");
        }
    }

    #[test]
    fn engine_builds_from_policy() {
        let _engine = policy_engine();
    }

    /// Fuel metering must stay on: `Store::set_fuel`/`get_fuel` both error
    /// when `consume_fuel(false)`, so this fails loudly if metering is
    /// dropped from the policy.
    #[test]
    fn fuel_metering_is_enabled() {
        let engine = policy_engine();
        let mut store = Store::new(&engine, ());
        store
            .set_fuel(CALL_FUEL)
            .expect("consume_fuel(true) must stay in the sandbox policy");
        assert_eq!(
            store.get_fuel().expect("fuel must be readable"),
            CALL_FUEL,
            "the store did not accept the configured call budget"
        );
    }

    // ── Denied proposals ─────────────────────────────────────────────────

    #[test]
    fn rejects_gc_proposal() {
        assert_gated("gc", "(module (type (struct (field i32))))", |c| {
            // The GC proposal is layered on typed function references.
            c.wasm_function_references(true);
            c.wasm_gc(true);
        });
    }

    #[test]
    fn rejects_typed_function_references() {
        assert_gated(
            "function-references",
            "(module (type $t (func)) (func (param (ref $t))))",
            |c| {
                c.wasm_function_references(true);
            },
        );
    }

    #[test]
    fn rejects_exception_handling() {
        assert_gated("exceptions", "(module (tag $e))", |c| {
            c.wasm_exceptions(true);
        });
    }

    #[test]
    fn rejects_shared_memory_threads() {
        assert_gated("threads", "(module (memory 1 1 shared))", |c| {
            c.wasm_threads(true);
        });
    }

    #[test]
    fn rejects_memory64() {
        assert_gated("memory64", "(module (memory i64 1))", |c| {
            c.wasm_memory64(true);
        });
    }

    #[test]
    fn rejects_multi_memory() {
        assert_gated("multi-memory", "(module (memory 1) (memory 1))", |c| {
            c.wasm_multi_memory(true);
        });
    }

    #[test]
    fn rejects_tail_calls() {
        assert_gated("tail-call", "(module (func $f (return_call $f)))", |c| {
            c.wasm_tail_call(true);
        });
    }

    #[test]
    fn rejects_extended_const() {
        assert_gated(
            "extended-const",
            "(module (global i32 (i32.add (i32.const 1) (i32.const 2))))",
            |c| {
                c.wasm_extended_const(true);
            },
        );
    }

    #[test]
    fn rejects_custom_page_sizes() {
        assert_gated(
            "custom-page-sizes",
            "(module (memory 1 (pagesize 1)))",
            |c| {
                c.wasm_custom_page_sizes(true);
            },
        );
    }

    // ── Proposals that must stay available ───────────────────────────────

    #[test]
    fn accepts_baseline_core_module() {
        assert_accepted(
            "wasm-mvp arithmetic",
            "(module (func (export \"add\") (param i32 i32) (result i32) \
             local.get 0 local.get 1 i32.add))",
        );
    }

    #[test]
    fn accepts_bulk_memory() {
        assert_accepted(
            "bulk-memory",
            "(module (memory 1) (func (memory.fill (i32.const 0) (i32.const 0) (i32.const 0))))",
        );
    }

    #[test]
    fn accepts_reference_types() {
        assert_accepted(
            "reference-types",
            "(module (table 1 funcref) (func (result externref) ref.null extern))",
        );
    }

    #[test]
    fn accepts_multi_value() {
        assert_accepted(
            "multi-value",
            "(module (func (result i32 i32) i32.const 1 i32.const 2))",
        );
    }

    #[test]
    fn accepts_simd() {
        assert_accepted(
            "simd",
            "(module (func (result v128) v128.const i32x4 0 0 0 0))",
        );
    }

    #[test]
    fn accepts_components() {
        let engine = policy_engine();
        Component::new(&engine, "(component)")
            .expect("the component model is the plugin ABI and must stay enabled");
    }

    // ── Contrast with upstream defaults ──────────────────────────────────

    /// Documents *why* `apply_sandbox_policy` exists: wasmtime 47's stock
    /// `Config::new()` accepts GC and exception-handling modules that our
    /// guests never produce. If this test ever fails, upstream narrowed its
    /// own defaults — good news, but re-read the policy rationale before
    /// deleting anything from it.
    #[test]
    fn upstream_defaults_are_wider_than_our_policy() {
        let engine = default_engine();
        assert!(
            Module::new(&engine, "(module (type (struct (field i32))))").is_ok(),
            "wasmtime default config no longer accepts the gc proposal"
        );
        assert!(
            Module::new(&engine, "(module (tag $e))").is_ok(),
            "wasmtime default config no longer accepts the exceptions proposal"
        );
    }

    /// The policy must be idempotent and independent of prior config state —
    /// applying it twice, or over an already-widened config, still denies.
    #[test]
    fn policy_overrides_a_pre_widened_config() {
        let mut config = Config::new();
        config.wasm_gc(true);
        config.wasm_exceptions(true);
        apply_sandbox_policy(&mut config);
        apply_sandbox_policy(&mut config);
        let engine = Engine::new(&config).expect("policy must build over a widened config");
        assert!(
            Module::new(&engine, "(module (type (struct (field i32))))").is_err(),
            "apply_sandbox_policy must win over an earlier wasm_gc(true)"
        );
    }
}
