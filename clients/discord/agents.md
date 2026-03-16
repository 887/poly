# poly-discord — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-06

---

## Purpose

`poly-discord` implements the `ClientBackend` trait for **Discord**. 

**⚠️ HIGH RISK**: Discord's Terms of Service explicitly prohibit unofficial clients and self-botting. Users should be warned clearly.

## WASM Plugin Architecture (DECISION D21, 2026-03-06)

This crate builds as **both** a native Rust library AND a WASM Component Model plugin.

- **Crate type**: `["cdylib", "rlib"]`
- **Feature gate**: `native` feature (default) enables reqwest, tokio-tungstenite, serde, async-trait, tokio, futures
- **WASM guest**: `src/guest.rs` — currently a **stub** returning errors/empty results. Must be completed when the native implementation is done.
- **cfg pattern**: `#[cfg(feature = "native")]` for native code, `#[cfg(target_os = "wasi")]` for WASI plugin code. **NEVER** use `target_arch = "wasm32"`.

### Building

```sh
# Native (default, part of workspace):
cargo build -p poly-discord

# WASM plugin:
cargo component build -p poly-discord --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_discord.wasm (~4.3MB debug)
```

### Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | Native `DiscordClient` stub, cfg-gated behind `feature = "native"` |
| `src/guest.rs` | WIT guest stub — returns errors for all operations, reports `BackendType::Discord` |
| `Cargo.toml` | Dual crate-type, feature-gated deps, WASI wit-bindgen dep |

### guest.rs Notes

- `#![allow(unsafe_code)]` — required for wit-bindgen FFI
- All methods return `Err(ClientError::Internal("not yet implemented"))` or empty collections
- `get_backend_type()` returns `BackendType::Discord`, `get_backend_name()` returns `"Discord"`
- When implementing the real client, the guest bridge must convert between native types and WIT types
- Because `wit_bindgen::generate!` lives in `src/wit_bindings.rs`, the export must use:
	`export!(DiscordPlugin with_types_in crate::wit_bindings)`
- The `messenger-plugin` world also requires a minimal `plugin_metadata::Guest` implementation even for stub plugins

## Implementation Phase

**Phase 3.3** — Implementation approach to be decided at that time. See [Phase 3 Plan](../../docs/phase-3-plan.md) section 3.3.

## Research Notes (Phase 1)

### Current Landscape (as of 2026-02-28)

**Rust crates available** (all early-stage):
- `discord_client_gateway = "0.2.0"` — "Undetected discord client gateway reimplementation" by UwUDev (~800 downloads)
- `discord_client_rest = "0.1.1"` — REST API companion to the gateway crate (~400 downloads)
- These are pre-alpha, very low adoption, but may work as starting points

**JavaScript ecosystem**:
- `discord.js-selfbot-v13` — ARCHIVED (October 2025). The main JS selfbot library is dead.
- Official `discord.js` is bot-only (requires bot token, not user token)

**Terminal clients** (reference):
- `cordless` (Go) — archived
- `discordo` (Go) — was a thing, status uncertain
- No mature Rust terminal Discord client found

### Discord API Structure
- **Gateway**: WebSocket connection for real-time events (wss://gateway.discord.gg)
- **REST API**: HTTPS endpoints for CRUD operations (https://discord.com/api/v10)
- **Voice**: Separate voice gateway (WebRTC + custom signaling)
- **Auth**: User token (obtained via login) or OAuth2

### TOS Implications
- Discord TOS Section 4: "you agree not to [...] use any unauthorized third-party software that accesses, intercepts, 'mines', or otherwise collects information from or through Discord"
- Using user tokens in unofficial clients is explicitly against TOS
- Risk: Account suspension/termination
- **MUST show clear warning to users before they add a Discord account**

### Possible Implementation Approaches (Decision Deferred)

1. **Direct API Client**: Reverse-engineer gateway/REST, handle anti-bot challenges. Cleanest UX but highest detection risk.
2. **Hidden Webview Bridge**: Run Discord web client in a hidden webview, intercept data via JS injection. Lower detection risk but heavier resource usage.
3. **Matrix Bridge**: Use `mautrix-discord` bridge — route Discord through Matrix SDK. Requires running a bridge server.
4. **Background Official Client**: Run official Discord client in background, communicate via IPC or scraping. Requires Discord installed.
5. **Minimal JS Runtime**: Embed a small JS engine (deno_core, boa) to execute Discord's client-side challenge code.

### Discord → Poly Mapping

| Discord Concept | Poly Type |
|---|---|
| Guild (Server) | `Server` |
| Category | `Category` |
| Text Channel | `Channel` (Text) |
| Voice Channel | `Channel` (Voice) |
| Stage Channel | `Channel` (Voice) |
| DM | `DmChannel` |
| Group DM (up to ~10 users) | `Group` |
| User | `User` |

### Self-Hosted Discord
- "Self-hosted Discord" shouldn't exist per Discord's terms
- But clone APIs exist (e.g., Fosscord/Spacebar)
- Support custom base URL for these instances
- Lower TOS risk for self-hosted clones

## Dependencies

### Native (default feature, TBD based on approach)
- `poly-client` — trait to implement
- Approach-dependent: `reqwest`, `tokio-tungstenite`, `webrtc`, or webview-based deps
- `async-trait`, `tokio`, `futures` — async support

### WASM (target_os = "wasi" only)
- `poly-client` — type definitions only
- `wit-bindgen` — WIT code generation

## Module Structure (Preliminary)

```
src/
├── lib.rs              # DiscordClient struct + ClientBackend impl (native-only)
├── guest.rs            # WIT guest bridge (WASI-only, stub)
├── auth.rs             # Authentication (approach-dependent) (TODO)
├── gateway.rs          # Gateway WebSocket (if direct approach) (TODO)
├── rest.rs             # REST API client (if direct approach) (TODO)
├── types/              # Discord-specific types (TODO)
├── voice.rs            # Voice gateway + WebRTC (TODO)
└── warnings.rs         # TOS warning display logic (TODO)
```

## E2E Test Coverage (2026-03-06)

**10 tests** in `crates/plugin-host-tests/tests/client_e2e/discord.rs` — stub behavior verification through WASM plugin host:

- Backend identity (type=Discord, name="Discord")
- `authenticate()` returns `Err(Internal("not yet implemented"))`
- `is_authenticated()` returns false
- All list methods return empty `Ok(vec![])`
- `get_server()` / `get_channel()` return `Err(NotFound(...))`
- `set_presence()`, `logout()` return `Ok(())`
- Event stream returns valid (empty) stream

```sh
cargo test -p poly-plugin-loader-tests --features test-discord --test client_e2e -- --nocapture
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

**Additional exception for `guest.rs`**: `#![allow(unsafe_code)]` is required for wit-bindgen FFI.

See root `agents.md` § 7a for the full rationale.
