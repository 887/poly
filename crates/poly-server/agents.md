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

## SurrealDB 3.x API Notes (CRITICAL — Read Before Touching DB Code)

These were all discovered the hard way when porting from the original scaffold to SurrealDB 3.0.1.
Violating any of these will produce compile errors.

### Engine: `SurrealKv` (not `SurrealKV`)
```rust
// CORRECT
use surrealdb::engine::local::SurrealKv;
Surreal::new::<SurrealKv>(&config.db_path).await?

// WRONG (old name, compile error in 3.x)
use surrealdb::engine::local::SurrealKV;
```

### IDs: `String` not `Thing`
`surrealdb::sql::Thing` is **removed** in 3.0.x. All record IDs and FK fields must be `String`.
```rust
// CORRECT — models/mod.rs
pub struct Message {
    pub id: Option<String>,
    pub channel: String,   // FK to channel table
    pub author: String,    // FK to user table
}

// WRONG — Thing is gone
pub struct Message {
    pub id: Option<Thing>,
    pub channel: Thing,
}
```

### Bindings: owned values only — `&String` does NOT implement `SurrealValue`
```rust
// CORRECT
.bind(("uid", auth.user_id.clone()))
.bind(("name", some_string.clone()))
.bind(("opt", option_str.map(str::to_owned)))

// WRONG — &String and &str (non-static) do not implement SurrealValue
.bind(("uid", &auth.user_id))
.bind(("name", &some_string))
```

Static string literals (`&'static str`) do implement `SurrealValue` and are fine for constant values.

### What DOES implement `SurrealValue`
- `String` ✓
- `&'static str` ✓ (static only)
- `i8, i16, i32, i64, u8, u16, u32, u64, f32, f64` ✓
- `bool` ✓
- `chrono::DateTime<Utc>` ✓
- `Option<T>` where `T: SurrealValue` ✓
- `Vec<T>` where `T: SurrealValue` ✓
- `serde_json::Value` ✓
- 2-tuples `(&'static str, OwnedT)` ✓ (works via Array IntoVariables path)

### Taking Results: Use `serde_json::Value` as intermediate
`.take::<MyStruct>()` fails because custom structs don't implement `SurrealValue`.
Always deserialize via `serde_json`:

```rust
// CORRECT pattern for fetching a single optional record
let raw: Option<serde_json::Value> = state.db
    .query("SELECT * FROM user WHERE ...")
    .bind(...)
    .await?
    .take(0)
    .map_err(AppError::Db)?;
let user: Option<User> = raw
    .map(|v| serde_json::from_value::<User>(v).map_err(|e| AppError::Internal(e.to_string())))
    .transpose()?;

// CORRECT pattern for fetching a list
let raw: Vec<serde_json::Value> = state.db
    .query("SELECT * FROM user WHERE ...")
    .await?
    .take(0)
    .map_err(AppError::Db)?;
let users: Vec<User> = from_values(raw)?;  // local helper in each api module

// Helper (copy this into each api module that needs it):
fn from_values<T: serde::de::DeserializeOwned>(vals: Vec<serde_json::Value>) -> crate::error::Result<Vec<T>> {
    vals.into_iter()
        .map(|v| serde_json::from_value::<T>(v).map_err(|e| crate::error::AppError::Internal(e.to_string())))
        .collect()
}
```

### Taking Scalar Fields: `"field_name"` index works directly for `SurrealValue` types
```rust
// Count queries — works correctly for SELECT count() GROUP ALL
let count: Option<i64> = state.db
    .query("SELECT count() FROM message WHERE channel = type::thing($ch) GROUP ALL")
    .bind(("ch", channel_id.clone()))
    .await?
    .take("count")        // extracts the "count" key from [{count: N}]
    .map_err(AppError::Db)?;

// Extract a single field of a String type
let server_id: Option<String> = state.db
    .query("SELECT server FROM type::thing($ch) LIMIT 1")
    .bind(("ch", channel_id.clone()))
    .await?
    .take("server")      // String: SurrealValue ✓
    .map_err(AppError::Db)?;
```

`take("field")` internally calls `(0, "field")` — unwraps the first element of the result array,
then extracts the named field from the object. Confirmed by reading `surrealdb-3.0.1/src/opt/query.rs`.

### `type::thing($var)` in Queries
SurrealDB still needs `type::thing()` when passing a record ID string into a query that
compares against a table field. Without it the string won't match the stored ID:
```sql
SELECT * FROM participant WHERE channel = type::thing($ch)
```

### `IntoVariables` — how bindings work internally
The bind args are processed as: if the value is an Object → becomes Variables directly;
if an Array of chunks(2) → each pair is key/value. Tuple `("key", value)` goes through
the Array path. This is why `(&'static str, OwnedValue)` works for `.bind()`.

---

## Session Notes

### 2026-03-01 (initial scaffold)
- Created full crate structure: config, error, models, db schema, ws, auth, api/* , upload.
- All API handlers implemented; compile check pending.
- Integration tests scaffolded.
- Protocol document created at `docs/poly-server-protocol.md`.

### 2026-03-01 (compile fixes — SurrealDB 3.0.1 compat)
- Fixed 157 compile errors across all source files — see summary above.
- Created `src/lib.rs` so integration tests can `use poly_server::…`.
- `cargo cranky -p poly-server`: 0 errors, 0 warnings.
- All clippy lints in integration tests suppressed with `#![allow(…)]` (expected for test code).
