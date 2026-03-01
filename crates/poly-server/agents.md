# poly-server — Agent Instructions

> Read this file at the start of every session working on `poly-server`.
> Last Updated: 2026-03-01

---

## Purpose

`poly-server` is a **lean, self-hosted chat server** serving as:
1. A reference implementation of the Poly messaging protocol.
2. A fallback backend for Poly clients when third-party services (Stoat, Discord, etc.) block access.
3. A test harness backend for **integration testing** the Poly client.

It intentionally avoids operational complexity — single binary, embedded SurrealKV, file-based attachment storage.

---

## Architecture Decisions

### DECISION(DX-S01): Pragmatic Auth — argon2 + custom JWT

**Context**: SurrealDB RECORD ACCESS (schema-level permissions) works via SQL-level sessions. In embedded SurrealKV, we cannot cleanly issue multi-user sessions from a single Rust process without reimplementing the session layer.

**Decision**: Auth is fully implemented in Rust:
- `argon2` for password hashing
- `jsonwebtoken` with `Claims { sub, device_id, exp, iat }` for sessions
- All access-control checks in Rust handler functions
- No SurrealDB row-level permissions (to avoid the session isolation problem)

**Trade-off**: Less DB-native security, but the entire permission model is auditable in one place.

---

### DECISION(DX-S02): Soft Delete Messages

**Context**: Show `[deleted]` placeholder for replies/threads rather than broken references.

**Decision**: `DELETE /messages/:id` sets `deleted = true`, replaces `content = '[deleted]'`. Hard delete never happens via the API. `message_to_response()` enforces this on the wire.

---

### DECISION(DX-S03): Attachment Access Control

**Context**: Files should only be readable by users who can read the channel they belong to.

**Decision**:
- Orphan attachments (not yet linked to a message) are readable only by the uploader.
- Once linked to a message, readable by anyone who can read the channel (server member or DM participant).
- Access check is in `GET /attachments/:id` handler via `can_read_channel()`.
- Files stored on disk as `UUID.ext`, DB record holds `storage_name` (safe from path traversal).

---

### DECISION(DX-S04): Per-User Broadcast Channel for WebSocket

**Context**: A user may be logged in on multiple devices simultaneously.

**Decision**: `WsState` holds `HashMap<user_id, broadcast::Sender<ServerEvent>>`. Each connected device receives a `Receiver` clone. Sending to a user reaches all their devices in one step.

---

### DECISION(DX-S05): Auth Middleware Applied in main.rs

**Context**: `Router::route_layer()` requires the state type to already be known, so middleware cannot be applied inside sub-module router functions before `AppState` is constructed.

**Decision**: `auth::routes` exposes `public_router()` and `protected_router()`. The `route_layer(auth_middleware)` is applied to `protected_router()` + `api::router()` in `main.rs` after `AppState` is built.

---

## Module Map

| Module | File | Responsibility |
|---|---|---|
| `main` | `src/main.rs` | App wiring, `AppState`, server startup |
| `config` | `src/config.rs` | Env-var configuration |
| `error` | `src/error.rs` | `AppError` → HTTP status |
| `db` | `src/db.rs` | SurrealKV init + idempotent SCHEMA |
| `models` | `src/models/mod.rs` | All DB record types |
| `auth` | `src/auth/mod.rs` | `Claims`, `AuthUser`, `auth_middleware` |
| `auth::routes` | `src/auth/routes.rs` | `/auth/*` handlers |
| `api` | `src/api/mod.rs` | Sub-router aggregator |
| `api::servers` | `src/api/servers.rs` | `/servers/*` CRUD |
| `api::channels` | `src/api/channels.rs` | `/channels/*`, DMs, groups |
| `api::messages` | `src/api/messages.rs` | `/messages/*`, reactions |
| `api::users` | `src/api/users.rs` | `/users/*`, friends |
| `api::upload` | `src/api/upload.rs` | `/attachments` upload + serve |
| `ws` | `src/ws/mod.rs` | WebSocket handler + `WsState` |
| `ws::events` | `src/ws/events.rs` | `ServerEvent` wire enum |

---

## Database Schema

Schema is idempotent (`DEFINE TABLE OVERWRITE … SCHEMAFULL`) — safe to run on every startup.
See `src/db.rs` for the full `SCHEMA` constant.

Tables: `user`, `device`, `server`, `membership`, `category`, `channel`, `participant`,
`message`, `reaction`, `friend_request`, `voice_session`, `invite`, `attachment`.

---

## Integration Tests

Tests live in `tests/integration.rs`. They:
1. Spin up a real `poly-server` on a random free port.
2. Run through full auth flow (signup → signin → use token → signout).
3. Create servers, channels, send messages, upload files.
4. Connect a WebSocket and verify event delivery.

Run with:
```bash
cargo test -p poly-server
```

---

## What NOT to Do

- **Do not store files in DB** — files go on disk, metadata in DB.
- **Do not use `unwrap()` or `expect()`** — use `AppError` or `?`.
- **Do not add external services** — no Redis, no Postgres, no S3; keep it a single binary.
- **Do not use SurrealDB RECORD ACCESS** — see DECISION(DX-S01).
- **Do not hard-delete messages** — see DECISION(DX-S02).

---

## Session Notes

### 2026-03-01 (initial scaffold)
- Created full crate structure: config, error, models, db schema, ws, auth, api/* , upload.
- All API handlers implemented; compile check pending.
- Integration tests scaffolded.
- Protocol document created at `docs/poly-server-protocol.md`.
