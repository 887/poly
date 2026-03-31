# poly-backup-server — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-01 (Session 2)


---

## Priority 2 — Use Jujutsu (jj) Instead of Git

- **Always use `jj` commands** for version control, never raw `git`
- `jj status`, `jj diff`, `jj log`, `jj show` for inspection
- `jj new`, `jj describe`, `jj commit` for creating changes
- `jj git push` to push to remote
- Only fall back to `git` if `jj` cannot accomplish the task

---

---

## Purpose

`poly-backup-server` is a standalone Axum server that stores **encrypted settings blobs**
for Poly clients. The server is zero-knowledge — it never sees plaintext.

See [README.md](README.md) for the full feature status checklist and run instructions.

## Implementation Status

Core implementation complete including E2E protocol tests (10/10 pass). See README.md feature
table for exact status.

## Docker Build & Deployment

**Dockerfile location:** `servers/backup-server/Dockerfile`

**Keep it in sync:** Whenever `servers/backup-server/Cargo.toml` or workspace dependencies change,
the Dockerfile may need review. Specifically:
- If new workspace crates are added as dependencies → update `COPY` steps if they live outside `servers/`
- If dependency versions are updated → rebuild to get new versions
- If binary name or src structure changes → update `RUN mkdir` / `touch` paths

**Build:** `docker build -t poly-backup-server servers/backup-server/`  
**Run:** `docker run -e POLY_BACKUP_PASSPHRASE=... -v /data:/data poly-backup-server`

## CRITICAL: SurrealDB 3.0 datetime → `serde_json::Value` incompatibility

**DECISION(DX-SURREAL-DATETIME-1):** All timestamp fields in this server use `TYPE string`
(not `TYPE datetime`) and store RFC3339 strings set from Rust-side `Utc::now().to_rfc3339()`.

**WHY:** SurrealDB 3.0.x with `kv-surrealkv` CANNOT serialize `TYPE datetime` fields into
`serde_json::Value` when using `.take::<serde_json::Value>(0)`. The error is:
`"Expected any, got datetime"`. This applies to all datetime-typed columns in any `SELECT *` result.

**RULES (NON-NEGOTIABLE):**
- NEVER use `TYPE datetime` for any schema field (exception: fields never read back in Rust)
- NEVER use `time::now()` inside SurrealQL string literals
- ALWAYS bind timestamps as `$now` and set from Rust: `.bind(("now", Utc::now().to_rfc3339()))`
- ALWAYS do expiry checks in Rust with `chrono::DateTime::parse_from_rfc3339()` not SurrealQL `time::now()`
- ALWAYS use explicit SELECT column lists (not `SELECT *`) to avoid accidentally retrieving
  datetime-typed columns that may be added in the future

**Example — correct pattern:**
```rust
state.db
    .query("CREATE table CONTENT { value: $val, created_at: $now }")
    .bind(("val", value))
    .bind(("now", Utc::now().to_rfc3339()))
    .await?.check().map_err(AppError::from)?;
```

**Example — expiry check in Rust (NOT in SurrealQL):**
```rust
let expires_at_str = record
    .get("expires_at")
    .and_then(serde_json::Value::as_str)
    .ok_or(AppError::Unauthorized)?;
if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires_at_str) {
    if exp <= Utc::now() { return Err(AppError::Unauthorized); }
} else {
    return Err(AppError::Unauthorized);
}
```

## Making Changes

## Implementation Phase

**Phase 2** (section 2.8). See [Phase 2 Plan](../../docs/phase-2-plan.md) § 2.8 and
[Phase 2.3 Plan](../../docs/phase-2.3-plan.md) for the detailed sub-plan.

## Architecture — Implemented

### File Map

```
crates/poly-backup-server/
├── src/
│   ├── main.rs         # tokio::main, DB init, graceful shutdown
│   ├── lib.rs          # AppState, create_app(), utoipa ApiDoc, health check
│   ├── config.rs       # Config::from_env() — all POLY_* env vars
│   ├── error.rs        # AppError enum, IntoResponse, Result<T> alias
│   ├── db.rs           # Db type alias, init(), SCHEMA const, record structs
│   ├── auth/mod.rs     # Challenge/Auth handlers, AuthUser extractor, verify_pow(), hash_token()
│   ├── sync/mod.rs     # push, pull, status handlers + request/response types
│   └── web/mod.rs      # admin_router(), AdminState, HTML embedded SPA (ADMIN_HTML const)
├── cranky.toml         # Lint config (deny: unwrap, expect, panic, indexing_slicing)
├── agents.md           # This file
└── README.md           # Feature status, run instructions, API overview
```

### SurrealDB Query Patterns

All queries follow the pattern established in `poly-server`. Always look there when
implementing new queries.

```rust
// Single optional record (use explicit column list, not SELECT *):
let rec: Option<serde_json::Value> = state.db
    .query("SELECT nonce, public_key, difficulty, expires_at FROM table WHERE field = $val LIMIT 1")
    .bind(("val", value))
    .await?
    .take(0)
    .map_err(AppError::from)?;

// Multiple records (explicit columns):
let recs: Vec<serde_json::Value> = state.db
    .query("SELECT sequence, encrypted_blob, pushed_at FROM table WHERE public_key = $pk ORDER BY seq ASC")
    .bind(("pk", pk))
    .await?
    .take(0)
    .map_err(AppError::from)?;

// Aggregation:
let agg: Option<serde_json::Value> = state.db
    .query("SELECT count() AS n, math::max(sequence) AS max_seq FROM table WHERE public_key = $pk GROUP ALL")
    .bind(("pk", pk))
    .await?
    .take(0)
    .map_err(AppError::from)?;
let n = agg.as_ref().and_then(|v| v.get("n")).and_then(serde_json::Value::as_i64).unwrap_or(0);

// Create — ALWAYS bind $now, NEVER use time::now() in SurrealQL:
let now_str = Utc::now().to_rfc3339();
state.db
    .query("CREATE table CONTENT { field: $val, created_at: $now }")
    .bind(("val", value))
    .bind(("now", now_str))
    .await?
    .check()
    .map_err(AppError::from)?;

// Update:
state.db
    .query("UPDATE table SET field = $val WHERE id = $id")
    .bind(("val", value))
    .bind(("id", id))
    .await?
    .check()
    .map_err(AppError::from)?;

// Delete:
state.db
    .query("DELETE table WHERE condition = $val")
    .bind(("val", value))
    .await?
    .check()
    .map_err(AppError::from)?;
```

### utoipa — KEEP DESCRIPTIONS CURRENT

**Rules for every handler:**
1. Add `#[utoipa::path(post/get/delete, path = "/api/...", ...)]` attribute
2. Add request body type to `request_body = Type` (if any)
3. Add all response variants to `responses(...)`
4. Add the handler to `paths(...)` in the `ApiDoc` derive in `lib.rs`
5. Add any new types to `components(schemas(...))` in `ApiDoc`
6. For authenticated routes: add `security(("BearerAuth" = []))`
7. Run `cargo doc -p poly-backup-server` and check the generated spec at `/swagger-ui`

**When to update utoipa:**
- New field added to a request/response struct → update the struct's doc comment
- Endpoint behaviour changed → update `responses(...)` in the `#[utoipa::path]`
- New endpoint added → follow all 7 steps above
- Description is wrong/stale → fix the doc comment on the struct or handler

### Admin UI (ADMIN_HTML const in web/mod.rs)

The entire admin UI is a `const &str` in `src/web/mod.rs`. It uses:
- **Tailwind CSS** (CDN, `cdn.tailwindcss.com`) — utility classes
- **Alpine.js** (CDN, `unpkg.com/alpinejs`) — reactive state + `x-data`/`x-bind`
- **Web Crypto API** — `crypto.subtle.digest("SHA-256", ...)` for PoW mining in JS
- No build step, no bundler — all inline

When editing the HTML:
- The login page PoW flow: `GET /admin/challenge` → mine in JS → `POST /admin/login`
- All dashboard API calls use `credentials: 'include'` to send the session cookie
- The `app()` Alpine function is the single source of truth for all UI state
- `fmtDate(iso)` + `fmtRel(iso)` are the date formatting helpers in the JS
- CSS custom properties are defined in `<style>`:root{...}` — match the app's neutral-dark theme

### Admin Session Security

- Sessions stored in `AdminState.sessions: DashMap<String, Instant>`
  where the key is `SHA-256(raw_token)` and value is expiry `Instant`
- Raw token is only in the browser cookie (`poly_admin=<token>`)
- `check_session()` in `web/mod.rs` validates every admin API request
- All admin auth uses constant-time string comparison (`ct_str_eq`)
- Rate limit: `AdminState.rate: Mutex<AdminLoginTracker>` — 10 attempts/minute global

### Token System (API)

- Tokens: 128-char alphanumeric (a-z,A-Z,0-9) — ~760 bits entropy
- Storage: `SHA-256(raw_token)` in the `token` table — raw never stored server-side
- Expiry: rolling — every API call that passes auth resets `expires_at` to `now + token_expiry_days`
- `AuthUser` extractor in `auth/mod.rs` handles all the DB lookup + expiry rolling

### PoW Protocol

- `POST /api/challenge` → server generates random 64-char nonce, stores in DB for 60s
- Client mines: find `counter` such that SHA-256(nonce + counter.to_string()) has ≥ N leading zero bits
- `POST /api/auth` → server verifies PoW, verifies passphrase, issues token
- `verify_pow(nonce, counter, difficulty)` in `auth/mod.rs` is the canonical verifier
- Admin login uses same primitive but with lower difficulty (16 vs 20 bits)
  and challenges stored in memory (`AdminState.challenges`) not DB

## Architecture

### Stack
- **Axum 0.8** (via Dioxus fullstack) — HTTP server
- **Dioxus fullstack** — Admin web UI
- **SurrealDB 3.0** (SurrealKV) — server-side storage
- **TailwindCSS** — Admin UI styling

### Auth Flow (see overall-plan.md Section 5.2)

```
1. Client → Server: Request challenge
2. Server → Client: PoW challenge (SHA-256 based, configurable difficulty)
3. Client → Server: PoW solution + server passphrase
4. Server validates: PoW correct? Passphrase correct? Account limit not reached? Public key known or new slot available?
5. Server → Client: Long session token (128+ chars)
6. Client stores token locally, uses for all subsequent requests
```

### Token System
- Tokens are long random strings — impractical to brute-force
- Each token tracks:
  - `public_key_user_id` — the user's Ed25519 public key
  - `device_name` — user-provided device name
  - `client_info` — browser/client string
  - `created_at` — when the token was issued
  - `last_seen_at` — last API call with this token
- Token expiry: configurable days of inactivity (e.g., 365 days)
- Tokens can be revoked (remote logout from admin UI or client)

### Rate Limiting
- Exponential backoff on failed passphrase attempts (per IP)
- PoW difficulty can be increased under load
- Global rate limit on auth endpoints

### Storage Model
- **Users table**: `public_key` (primary key), `created_at`, `last_sync_at`
- **Tokens table**: `token_hash`, `public_key` (FK), `device_name`, `client_info`, `created_at`, `last_seen_at`, `active`
- **Settings table**: `public_key` (FK), `sequence_num`, `encrypted_blob`, `created_at`
  - Each settings change gets a monotonic sequence number per user
  - Client pulls changes since their last-known sequence number
  - Blobs are opaque — server cannot read them

### REST API

| Method | Endpoint | Auth | Description |
|---|---|---|---|
| POST | `/api/challenge` | None | Request PoW challenge |
| POST | `/api/auth` | PoW solution | Verify PoW + passphrase, issue token |
| GET | `/api/sync/pull?since={seq}` | Token | Pull settings changes since sequence |
| POST | `/api/sync/push` | Token | Push new encrypted settings blob |
| GET | `/api/tokens` | Token | List active tokens for this user |
| DELETE | `/api/tokens/{id}` | Token | Revoke a specific token |
| GET | `/api/status` | None | Server health + capacity (accounts used/max) |

### Admin Web UI
- View registered accounts (by public key hash, NOT data)
- View active sessions per account
- Server configuration (passphrase, max accounts, token expiry, PoW difficulty)
- Server health dashboard

### Configuration (Environment Variables)

| Variable | Default | Description |
|---|---|---|
| `POLY_PASSPHRASE` | (required) | Server-wide access passphrase |
| `POLY_MAX_ACCOUNTS` | `0` (unlimited) | Maximum user accounts |
| `POLY_TOKEN_EXPIRY_DAYS` | `365` | Days of inactivity before token expires |
| `POLY_POW_DIFFICULTY` | `20` | PoW difficulty (number of leading zero bits) |
| `POLY_BIND_ADDRESS` | `0.0.0.0:3000` | Server listen address |
| `POLY_DB_PATH` | `./data/poly-backup.db` | SurrealKV database path |
| `POLY_ADMIN_TOKEN` | (optional) | Token for admin endpoints |

## Dependencies

- `dioxus = "0.7.3"` (fullstack feature)
- `axum = "0.8"` (via dioxus)
- `surrealdb = "3.0.1"` (feature: kv-surrealkv)
- `tokio` — async runtime
- `sha2` — PoW hash computation
- `rand` — token generation
- `serde`, `serde_json` — API (de)serialization
- `tower-http` — CORS, rate limiting

## Module Structure

```
src/
├── main.rs             # Axum server entry + Dioxus fullstack mount
├── config.rs           # Environment variable configuration
├── auth/
│   ├── mod.rs
│   ├── pow.rs          # Proof-of-work challenge generation + verification
│   ├── passphrase.rs   # Passphrase verification with rate limiting
│   └── tokens.rs       # Token generation, validation, expiry, revocation
├── sync/
│   ├── mod.rs
│   ├── push.rs         # Accept encrypted settings blob
│   ├── pull.rs         # Return settings changes since sequence
│   └── storage.rs      # SurrealDB operations for settings blobs
├── web/
│   ├── mod.rs
│   ├── admin.rs        # Admin dashboard components
│   └── status.rs       # Public status page
├── db.rs              # SurrealDB initialization + schema
└── middleware.rs       # Auth middleware, rate limiting
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
