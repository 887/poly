# poly-client — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-client` defines the **shared protocol** that all messenger backends implement. It contains:

- The `ClientBackend` trait — the interface every backend (Stoat, Matrix, Discord, Teams, Demo) must implement
- Shared data types (`Server`, `Channel`, `Message`, `User`, etc.)
- Shared event types (`ClientEvent` enum)
- `BackendType` enum for identifying which backend a resource comes from

## Key Design Principles

1. **Backend-agnostic**: `poly-core` depends on this crate and uses the trait interface. It never imports concrete backend types.
2. **Async**: All trait methods are async (using `async_trait` or Rust's native async in traits).
3. **Event-driven**: Backends emit events via a stream. The UI subscribes to this event stream.
4. **Flat types**: Data types are simple and flat — backends map their complex internal types to these shared types.

## Trait Design

The `ClientBackend` trait covers:
- **Auth**: login, logout, session management
- **Servers**: list servers, get server details
- **Channels**: list channels per server, get channel details
- **Messages**: send/receive, paginated history, edit, delete
- **Users**: profiles, friends, presence, channel members
- **Groups**: multi-user DMs/group chats
- **DMs**: direct message channels
- **Notifications**: cross-account notification stream
- **Events**: real-time event stream for all state changes
- **Backend info**: type enum, display name, icon

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
- `async-trait` — if needed for trait async methods
- **NO** dioxus, surrealdb, or UI dependencies here

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
