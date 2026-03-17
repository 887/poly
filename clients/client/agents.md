# poly-client — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-06

---

## Purpose

`poly-client` defines the **shared protocol** that all messenger backends implement. It contains:

- The `ClientBackend` trait — the interface every backend (Stoat, Matrix, Discord, Teams, Demo, Poly Server) must implement
- Shared data types (`Server`, `Channel`, `Message`, `User`, etc.)
- Shared event types (`ClientEvent` enum)
- `BackendType` enum for identifying which backend a resource comes from
- `ClientError` shared error type

## WASM Plugin Role — CRITICAL

This crate is the **only Rust dependency** that WASM plugin builds use. All backend client crates (demo, stoat, matrix, discord, teams, server-client) compile as WASM Component Model plugins targeting `wasm32-wasip2`. In that mode, they depend **only** on `poly-client` + `wit-bindgen`.

The types in this crate are the **source of truth** for the WIT (WebAssembly Interface Types) definitions in `wit/messenger-plugin.wit`. Every type in `types.rs`, `events.rs`, and `error.rs` has a corresponding WIT type, and bridge code in `crates/plugin-host/src/bridge.rs` converts between them.

**When modifying types here, you MUST also:**
1. Update `wit/messenger-plugin.wit` to match
2. Update `crates/plugin-host/src/bridge.rs` (host-side conversions)
3. Update every client's `src/guest.rs` (guest-side conversions)
4. Rebuild all WASM plugins: `cargo component build -p <crate> --target wasm32-wasip2`
5. Run E2E tests: `cargo test -p poly-plugin-loader-tests --all-features` (77 tests validate the full interface)

## Boundary Model — IMPORTANT

The Poly client-plugin ABI is a **WIT / Wasm Component Model** boundary, **not** a `wasm-bindgen` Rust↔JS boundary.

That means:
- Types cross as **WIT values** (`record`, `variant`, `enum`, `list`, `option`, `result`), not JS-managed handle objects.
- The boundary is **value-oriented**, so blog advice about passing exported objects by `&reference`, `Wasm*`/`Js*` naming, `wasm_refgen`, or converting Rust errors into `JsValue`/`js_sys::Error` is **not the main rule set here**.
- Errors must stay in the typed WIT channel (`client-error`) and map cleanly to/from `poly-client::ClientError`.
- If a new type cannot be expressed cleanly in WIT, the ABI design is wrong and should be simplified before implementation.

Use `wasm-bindgen`-style patterns only for separate browser/JS interop code, not for the client-plugin contract itself.

## E2E Test Validation

The entire `ClientBackend` trait interface is validated by 77 E2E tests in `crates/plugin-host-tests/`:
- 26 demo tests exercise the full lifecycle (authenticate → all data types → mutate → logout)
- 50 stub tests (10 per client) verify each backend's error/empty behavior
- Tests run through the actual WASM plugin host, not native Rust calls

## Key Design Principles

1. **Backend-agnostic**: `poly-core` depends on this crate and uses the trait interface. It never imports concrete backend types.
2. **Async**: All trait methods are async (using `async_trait`).
3. **Event-driven**: Backends emit events via a stream. The UI subscribes to this event stream.
4. **Flat types**: Data types are simple and flat — backends map their complex internal types to these shared types.
5. **WASM-safe**: Types must be serializable and not depend on platform-specific features (no `dioxus`, `tokio`, etc.).

## Trait Design

The `ClientBackend` trait covers:
- **Auth**: login, logout, session management
- **Servers**: list servers, get server details
- **Channels**: list channels per server, get channel details
- **Messages**: send/receive, paginated history, edit, delete
- **Users**: profiles, friends, presence, channel members
- **Groups**: multi-user DMs/group chats
- **DMs**: direct message channels plus first-class DM open/create and Saved Messages open hooks
- **Notifications**: cross-account notification stream
- **Events**: real-time event stream for all state changes
- **Backend info**: `backend_type()` → `BackendType` enum, `backend_name()` → display string

Recent shared-surface additions (2026-03-17):
- `add_group_member(group_id, user_id)`
- `open_direct_message_channel(user_id)`
- `open_saved_messages_channel()`

These must stay mirrored in `wit/messenger-plugin.wit`, `crates/plugin-host/src/registry.rs`, and every client guest implementation.

## Type Mapping Strategy

| Poly Type | Stoat | Matrix | Discord | Teams |
|---|---|---|---|---|
| `Server` | Server | Space | Guild | Team |
| `Channel` | Channel | Room | Channel | Channel |
| `Category` | Category | Space child | Category | — |
| `User` | User | User | User | User |
| `Group` | Group DM | Multi-user room | Group DM | Group chat |
| `DmChannel` | DM | 1:1 room | DM | 1:1 chat |

## Dependencies

This crate should have MINIMAL dependencies:
- `serde`, `serde_json` — serialization
- `chrono` or `time` — timestamps
- `url` — URLs for icons/avatars
- `futures` — Stream trait for events
- `async-trait` — for trait async methods
- **NO** dioxus, surrealdb, tokio, or UI dependencies here
- This crate compiles for both native and `wasm32-unknown-unknown` (web) targets

## Files

```
src/
├── lib.rs          # Main trait + re-exports
├── traits.rs       # ClientBackend trait definition
├── types.rs        # Server, Channel, Message, User, etc.
├── events.rs       # ClientEvent enum
└── error.rs        # ClientError type
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
