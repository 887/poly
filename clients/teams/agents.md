# poly-teams — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-06

---

## Purpose

`poly-teams` implements the `ClientBackend` trait for **Microsoft Teams** using the **Microsoft Graph API**.

## WASM Plugin Architecture (DECISION D21, 2026-03-06)

This crate builds as **both** a native Rust library AND a WASM Component Model plugin.

- **Crate type**: `["cdylib", "rlib"]`
- **Feature gate**: `native` feature (default) enables reqwest, oauth2, serde, async-trait, tokio, futures
- **WASM guest**: `src/guest.rs` — currently a **stub** returning errors/empty results. Must be completed when the native implementation is done.
- **cfg pattern**: `#[cfg(feature = "native")]` for native code, `#[cfg(target_os = "wasi")]` for WASI plugin code. **NEVER** use `target_arch = "wasm32"`.

### Building

```sh
# Native (default, part of workspace):
cargo build -p poly-teams

# WASM plugin:
cargo component build -p poly-teams --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_teams.wasm (~4.3MB debug)
```

### Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | Native `TeamsClient` stub, cfg-gated behind `feature = "native"` |
| `src/guest.rs` | WIT guest stub — returns errors for all operations, reports `BackendType::Teams` |
| `Cargo.toml` | Dual crate-type, feature-gated deps, WASI wit-bindgen dep |

### guest.rs Notes

- `#![allow(unsafe_code)]` — required for wit-bindgen FFI
- All methods return `Err(ClientError::Internal("not yet implemented"))` or empty collections
- `get_backend_type()` returns `BackendType::Teams`, `get_backend_name()` returns `"Teams"`
- When implementing the real client, the guest bridge must convert between native types and WIT types
- Because `wit_bindgen::generate!` lives in `src/wit_bindings.rs`, the export must use:
  `export!(TeamsPlugin with_types_in crate::wit_bindings)`
- The `messenger-plugin` world also requires a minimal `plugin_metadata::Guest` implementation even for stub plugins

## Implementation Phase

**Phase 3.4** — Last backend to implement. See [Phase 3 Plan](../../docs/phase-3-plan.md) section 3.4.

## Technology

- **API**: Microsoft Graph REST API (https://graph.microsoft.com/v1.0/)
- **Auth**: OAuth2 with Azure AD
  - Device Code Flow (for headless/terminal)
  - Authorization Code Flow with PKCE (for browser-based)
- **Reference Implementation**: `ttyms` crate — terminal Microsoft Teams client in Rust

## Research Notes (Phase 1)

### ttyms Reference
- Crate: `ttyms = "0.1.4"` (released ~2026-02-27, very new)
- Architecture: Microsoft Graph API over HTTPS
- Auth: OAuth2 Device Code Flow or PKCE browser flow
- Ships with a **default Azure AD client ID** (works out of the box)
- Features:
  - 1:1 and group chat (send/receive/edit/delete)
  - Teams & Channels browsing
  - Message reactions
  - Presence/status
  - Vim-style navigation (TUI)
- Token storage: OS credential manager (`keyring` crate)
- Sensitive data zeroized in memory
- Scopes: Minimal permissions, delegated (user context only)

### Microsoft Graph API Endpoints

**Teams & Channels** (Team = Poly Server):
- `GET /me/joinedTeams` — list teams
- `GET /teams/{team-id}/channels` — list channels in team
- `GET /teams/{team-id}/channels/{channel-id}/messages` — channel messages
- `POST /teams/{team-id}/channels/{channel-id}/messages` — send message

**Chat** (1:1 and Group):
- `GET /me/chats` — list all chats
- `GET /chats/{chat-id}/messages` — chat messages
- `POST /chats/{chat-id}/messages` — send message
- Chat types: `oneOnOne`, `group`, `meeting`

**Users & Presence**:
- `GET /me` — current user profile
- `GET /users/{id}` — user profile
- `GET /me/presence` — current presence
- `GET /communications/presences` — batch presence

**Subscriptions** (real-time-ish):
- `POST /subscriptions` — webhook subscriptions for change notifications
- Alternative: polling at intervals

### Teams → Poly Mapping

| Teams Concept | Poly Type |
|---|---|
| Team | `Server` |
| Channel (in Team) | `Channel` |
| 1:1 Chat | `DmChannel` |
| Group Chat | `Group` (displayed under DMs with Teams icon) |
| Meeting | Not mapped (stub only) |
| User | `User` |

### Auth Flow
1. Open browser → Azure AD login page
2. User authenticates with Microsoft account
3. Redirect back with auth code
4. Exchange for access token + refresh token
5. Store tokens securely (local SurrealKV, encrypted for backup)

### Rate Limiting
- Microsoft Graph has per-app and per-user throttling
- 429 responses with Retry-After header
- Need exponential backoff logic

## Dependencies

### Native (default feature)
- `poly-client` — trait to implement
- `reqwest` — HTTP client for Graph API
- `oauth2` — OAuth2 flow handling
- `serde`, `serde_json` — API response parsing
- `tokio` — async runtime
- `url` — URL construction
- `async-trait`, `futures` — async support

### WASM (target_os = "wasi" only)
- `poly-client` — type definitions only
- `wit-bindgen` — WIT code generation

## Module Structure

```
src/
├── lib.rs              # TeamsClient struct + ClientBackend impl (native-only)
├── guest.rs            # WIT guest bridge (WASI-only, stub)
├── auth.rs             # OAuth2 (Device Code + PKCE) (TODO)
├── graph/              # Microsoft Graph API client (TODO)
│   ├── mod.rs
│   ├── teams.rs        # Teams + Channels
│   ├── chats.rs        # 1:1 and group chats
│   ├── messages.rs     # Message operations
│   ├── users.rs        # User profiles, presence
│   └── subscriptions.rs # Change notification subscriptions
├── types/              # Teams-specific type definitions (TODO)
│   ├── mod.rs
│   └── ...             # Matching Graph API schemas
└── rate_limit.rs       # Rate limiting + retry logic (TODO)
```

## E2E Test Coverage (2026-03-06)

**10 tests** in `crates/plugin-host-tests/tests/client_e2e/teams.rs` — stub behavior verification through WASM plugin host:

- Backend identity (type=Teams, name="Teams")
- `authenticate()` returns `Err(Internal("not yet implemented"))`
- `is_authenticated()` returns false
- All list methods return empty `Ok(vec![])`
- `get_server()` / `get_channel()` return `Err(NotFound(...))`
- `set_presence()`, `logout()` return `Ok(())`
- Event stream returns valid (empty) stream

```sh
cargo test -p poly-plugin-loader-tests --features test-teams --test client_e2e -- --nocapture
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

**Additional exception for `guest.rs`**: `#![allow(unsafe_code)]` is required for wit-bindgen FFI.

See root `agents.md` § 7a for the full rationale.
