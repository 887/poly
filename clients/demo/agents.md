# poly-demo — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-06

---

## Purpose

`poly-demo` is a **mock/demo client** implementing the `ClientBackend` trait. It generates fake data for testing the UI without requiring real messenger accounts.

## WASM Plugin Architecture (DECISION D21, 2026-03-06)

This crate builds as **both** a native Rust library AND a WASM Component Model plugin:

- **Crate type**: `["cdylib", "rlib"]` — rlib for native workspace builds, cdylib for WASM
- **Feature gate**: `native` feature (default) enables Dioxus, futures, async-trait, tokio
- **WASM guest**: `src/guest.rs` contains the WIT bridge (only compiled for `target_os = "wasi"`)
- **cfg pattern**: Use `#[cfg(feature = "native")]` for native-only code, `#[cfg(target_os = "wasi")]` for WASI plugin code. **NEVER** use `target_arch = "wasm32"` — that also matches the web frontend target.

### Building

```sh
# Native (default, part of workspace):
cargo build -p poly-demo

# WASM plugin:
cargo component build -p poly-demo --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_demo.wasm (~37MB debug)
```

### Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | Native `DemoClient`/`DemoClient2` impls, cfg-gated behind `feature = "native"` |
| `src/data.rs` | Demo data generators. Avatar assets use `#[cfg(feature = "native")]` for `dioxus::Asset` vs plain `&str` |
| `src/guest.rs` | WIT guest implementation — full bridge with type conversions, thread_local state, `Guest` trait impl |
| `Cargo.toml` | Dual crate-type, feature-gated deps, `[target.'cfg(target_os = "wasi")'.dependencies]` for wit-bindgen |

### guest.rs Architecture

- `#![allow(unsafe_code)]` — **required** because wit-bindgen generates FFI stubs with `#[export_name]` and `unsafe fn`
- `wit_bindgen::generate!({ world: "messenger-plugin", path: "../../wit" })` — generates types at `poly::messenger::types`
- Bridge functions: `to_wit_*` (poly-client → WIT for outputs), `from_wit_*` (WIT → poly-client for inputs)
- `thread_local! { STATE: RefCell<DemoState> }` — authenticated state management (no async runtime in WASM)
- Delegates to `crate::data::*` for actual data generation
- `export!(DemoPlugin)` — macro that wires up the component model exports

### Demo Data (WASM-compatible)

The demo data module (`data.rs`) was modified to work without Dioxus:
- `#[cfg(feature = "native")]` gates `use dioxus::prelude::*` and `Asset` type avatars
- `#[cfg(not(feature = "native"))]` provides `&str` fallback avatar paths
- All data generation functions work in both modes

## What It Provides

- **Demo users**: Hardcoded names, avatars, online/offline status (2 accounts: cat + dog)
- **Demo servers**: Multiple servers with categories and channels (text, voice, video)
- **Demo messages**: Various message types with realistic timestamps
- **Demo friends**: Friend list with status, last message preview
- **Demo groups**: Multi-user group chats
- **Demo notifications**: Friend requests, mentions, DM notifications

## Dependencies

### Native (default feature)
- `poly-client` — the trait to implement
- `dioxus` — Asset type for avatars
- `futures` — Stream for event emission
- `async-trait` — ClientBackend trait
- `tokio` — async runtime

## Electron/Web Note (2026-03-07)

When `poly-demo` is linked into the `wasm32-unknown-unknown` web/Electron app,
the cat demo client's live `event_stream()` must stay wasm-safe.

- `tokio::time::sleep` inside the demo event stream caused a runtime panic in
	Electron/web (`time not implemented on this platform`)
- For `target_arch = "wasm32"`, `DemoClient::event_stream()` now returns an
	empty stream instead of starting the timed fake-event loop
- This keeps demo data loading functional in Electron/web; the preloaded demo
	dataset is enough for UI verification even without live fake events

### WASM (target_os = "wasi" only)
- `poly-client` — type definitions only
- `wit-bindgen` — WIT code generation (workspace dep with `macros` + `realloc` features)

## E2E Test Coverage (2026-03-06)

**26 tests** in `crates/plugin-host-tests/tests/client_e2e/demo.rs` — full lifecycle through WASM plugin host:

- Backend identity (type=Demo, name="Demo")
- Authenticate with token + logout lifecycle
- Session field validation (id, user, token, backend, icon_emoji, instance_id)
- Servers (list, get_by_id, not_found), Channels (list, get_by_id, not_found, type validation)
- Messages (list non-empty, send_message returns new message)
- Users (friends, channel_members, get_user_by_id)
- Groups (list, remove_group_member), DMs (list, messages)
- Notifications, voice participants
- Presence (get returns Online, set to Idle)
- Event stream returns valid stream
- Full lifecycle integration: authenticate → servers → channels → messages → send → DMs → groups → notifications → friends → set_presence → logout

```sh
cargo test -p poly-plugin-loader-tests --features test-demo --test client_e2e -- --nocapture
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

**Additional exception for `guest.rs`**: `#![allow(unsafe_code)]` is required because
wit-bindgen's `generate!` and `export!` macros produce FFI code with `#[export_name]`,
`unsafe fn`, and `unsafe {}` blocks. This is the WASM Component Model ABI and cannot
be avoided. Documented extensively in the file itself.

See root `agents.md` § 7a for the full rationale.
