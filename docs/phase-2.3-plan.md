# Phase 2.3 Plan — Backup Server (poly-backup-server)

> **Status:** 🔲 Not Started  
> **Parent:** [Phase 2 Plan](phase-2-plan.md) — Section 2.8  
> **Overall Context:** [Overall Plan §5](overall-plan.md#5-backup-server-architecture)  
> **Crate:** `servers/backup-server/`

---

## Overview

`poly-backup-server` is a standalone Axum HTTP server that stores **encrypted blobs** for Poly
clients. It knows nothing about the content it stores — all encryption happens on the client before
any data leaves the device.

**Stack:**
- **Axum 0.8** — HTTP server
- **SurrealDB 3.0 + SurrealKV** — embedded storage (consistent with rest of project)
- **Tailwind CSS + Alpine.js** — admin web SPA served at `/` (embedded HTML const, no build step)
- **utoipa 5** — OpenAPI 3.1 spec; Swagger UI served at `/swagger-ui` via CDN HTML
- **tokio** — async runtime

---

## 2.3.1 Project Structure

```
servers/backup-server/
├── src/
│   ├── main.rs             # Entry: tokio::main, startup, graceful shutdown
│   ├── lib.rs              # AppState, create_app(), utoipa ApiDoc
│   ├── config.rs           # Config::from_env() — all POLY_* env vars
│   ├── error.rs            # AppError enum, IntoResponse, Result<T> alias
│   ├── db.rs               # SurrealKV init, SCHEMA const, record structs
│   ├── auth/
│   │   └── mod.rs          # Challenge/Auth handlers, AuthUser extractor, hash_token, verify_pow
│   ├── sync/
│   │   └── mod.rs          # push, pull, status handlers + request/response types
│   └── web/
│       └── mod.rs          # admin_router(), AdminState, ADMIN_HTML embedded SPA
├── cranky.toml
├── Cargo.toml
├── agents.md
└── README.md
```

---

## 2.3.2 Configuration

All configuration via environment variables. No config file required.

| Variable | Default | Description |
|---|---|---|
| `POLY_PASSPHRASE` | *(required)* | Server-wide access passphrase (shared out-of-band) |
| `POLY_MAX_ACCOUNTS` | `0` (unlimited) | Maximum number of registered public keys |
| `POLY_TOKEN_EXPIRY_DAYS` | `365` | Days of inactivity before a token expires (rolling) |
| `POLY_POW_DIFFICULTY` | `20` | PoW difficulty in leading zero bits |
| `POLY_BIND` | `0.0.0.0:8080` | Address + port to listen on |
| `POLY_DATA_DIR` | `./data` | Directory for SurrealKV database files |
| `POLY_RATE_LIMIT_MAX` | `5` | Max failed auth attempts per IP before backoff |
| `POLY_RATE_LIMIT_WINDOW_SECS` | `3600` | Rate-limit window in seconds |

```rust
// src/config.rs
#[derive(Debug, Clone)]
pub struct Config {
    pub passphrase: String,
    pub max_accounts: usize,
    pub token_expiry_days: u64,
    pub pow_difficulty: u32,
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub rate_limit_max: u32,
    pub rate_limit_window_secs: u64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> { ... }
}
```

---

## 2.3.3 Database Schema (SurrealDB)

```sql
-- Registered accounts
DEFINE TABLE accounts SCHEMAFULL;
DEFINE FIELD public_key     ON accounts TYPE string;  -- hex Ed25519 pubkey
DEFINE FIELD registered_at  ON accounts TYPE datetime;
DEFINE FIELD last_seen_at   ON accounts TYPE datetime;
DEFINE INDEX accounts_pk ON accounts FIELDS public_key UNIQUE;

-- Session tokens
DEFINE TABLE tokens SCHEMAFULL;
DEFINE FIELD token_hash     ON tokens TYPE string;    -- SHA-256(raw_token)
DEFINE FIELD public_key     ON tokens TYPE string;    -- FK → accounts
DEFINE FIELD device_name    ON tokens TYPE string;    -- Client user-agent label
DEFINE FIELD created_at     ON tokens TYPE datetime;
DEFINE FIELD last_seen_at   ON tokens TYPE datetime;
DEFINE FIELD expires_at     ON tokens TYPE datetime;  -- rolling expiry
DEFINE INDEX tokens_hash ON tokens FIELDS token_hash UNIQUE;

-- Encrypted sync blobs (append-only log)
DEFINE TABLE sync_blobs SCHEMAFULL;
DEFINE FIELD public_key     ON sync_blobs TYPE string;
DEFINE FIELD sequence       ON sync_blobs TYPE int;
DEFINE FIELD encrypted_blob ON sync_blobs TYPE string;  -- base64-encoded ciphertext
DEFINE FIELD pushed_at      ON sync_blobs TYPE datetime;
DEFINE INDEX sync_pk_seq ON sync_blobs FIELDS public_key, sequence UNIQUE;

-- PoW challenges (short-lived)
DEFINE TABLE challenges SCHEMAFULL;
DEFINE FIELD nonce          ON challenges TYPE string;
DEFINE FIELD public_key     ON challenges TYPE string;
DEFINE FIELD difficulty     ON challenges TYPE int;
DEFINE FIELD created_at     ON challenges TYPE datetime;
DEFINE FIELD expires_at     ON challenges TYPE datetime;  -- TTL: 60s
DEFINE INDEX challenge_nonce ON challenges FIELDS nonce UNIQUE;

-- Rate-limit counters
DEFINE TABLE rate_limits SCHEMAFULL;
DEFINE FIELD ip             ON rate_limits TYPE string;
DEFINE FIELD failures       ON rate_limits TYPE int;
DEFINE FIELD window_start   ON rate_limits TYPE datetime;
```

Tasks:
- [ ] **2.3.3.1** SurrealKV init + schema migration runner in `db.rs`
- [ ] **2.3.3.2** Typed query helpers for each table (accounts, tokens, sync_blobs, challenges, rate_limits)
- [ ] **2.3.3.3** Auto-expire challenges (query on use; delete where `expires_at < now()`)
- [ ] **2.3.3.4** Background task: prune expired tokens + old challenges every 5 minutes

---

## 2.3.4 REST API

All JSON. All API routes under `/api`. Errors return `{ "error": "..." }`.

### Authentication middleware

Applied to all `/api/sync/*` and `/api/admin/*` routes:
1. Extract `Authorization: Bearer <token>` header → 401 if missing
2. SHA-256 hash the token, look up in DB → 401 if not found
3. Check `expires_at` → 401 if expired
4. Update `last_seen_at` + roll `expires_at` forward
5. Attach `public_key` to request extensions

---

### POST `/api/challenge`

**Purpose:** Issue a PoW nonce for a given public key.

**Request:**
```json
{ "public_key": "<hex ed25519 pubkey>" }
```

**Response `200`:**
```json
{
  "nonce": "<random 32-byte hex string>",
  "difficulty": 20,
  "expires_at": "2026-03-01T12:01:00Z"
}
```

**Logic:**
1. Validate `public_key` is valid hex (32 bytes = 64 hex chars)
2. Check rate limit for caller IP → 429 if exceeded
3. Delete any existing pending challenge for this `public_key`
4. Generate `nonce = random_hex(32)`
5. Store challenge with `expires_at = now() + 60s`
6. Return nonce + difficulty

**utoipa annotation:**
```rust
#[utoipa::path(
    post,
    path = "/api/challenge",
    request_body = ChallengeRequest,
    responses(
        (status = 200, description = "Challenge issued", body = ChallengeResponse),
        (status = 400, description = "Invalid public key"),
        (status = 429, description = "Too many failed attempts"),
    )
)]
```

Tasks:
- [ ] **2.3.4.1** `ChallengeRequest` + `ChallengeResponse` types with `utoipa::ToSchema`
- [ ] **2.3.4.2** Challenge handler + nonce generation (`rand::thread_rng()`)
- [ ] **2.3.4.3** Challenge storage + TTL enforcement

---

### POST `/api/auth`

**Purpose:** Verify PoW solution + passphrase, issue session token.

**Request:**
```json
{
  "public_key": "<hex>",
  "nonce": "<nonce from /api/challenge>",
  "counter": 12345678,
  "passphrase": "<server passphrase>",
  "device_name": "Linux Desktop"
}
```

**PoW verification:**
- Compute `SHA-256(nonce + counter.to_string())`
- Check that result has at least `difficulty` leading zero bits
- Challenge must not be expired

**Response `200`:**
```json
{
  "token": "<128-char base62>",
  "expires_at": "2027-03-01T00:00:00Z"
}
```

**Response `401`:**
```json
{ "error": "Invalid passphrase or PoW solution" }
```

> **Note:** Always return the same error string for wrong passphrase OR invalid PoW — do not distinguish, to prevent enumeration.

**Logic:**
1. Look up pending challenge by nonce + validate it matches `public_key`
2. Verify PoW: SHA-256(`nonce` + `counter`) has ≥ `difficulty` leading zero bits
3. Check rate limit for caller IP → 429 if exceeded; increment counter on attempt
4. Verify passphrase using `subtle::ConstantTimeEq` → if wrong, increment failure counter
5. Check `POLY_MAX_ACCOUNTS` → if at limit and `public_key` is new → 403
6. Upsert account record for `public_key`
7. Generate token: `rand::Alphanumeric.sample_iter(...).take(128)`
8. Store `token_hash = SHA-256(token)` + device_name + timestamps
9. Delete consumed challenge
10. Reset rate-limit counter on success
11. Return raw `token` (only time it's ever sent in plaintext)

Tasks:
- [ ] **2.3.4.4** `AuthRequest` + `AuthResponse` types with `utoipa::ToSchema`
- [ ] **2.3.4.5** PoW verifier: `verify_pow(nonce: &str, counter: u64, difficulty: u32) -> bool`
- [ ] **2.3.4.6** Passphrase constant-time comparison (`subtle` crate)
- [ ] **2.3.4.7** Token generator + SHA-256 hasher
- [ ] **2.3.4.8** Full auth handler wiring
- [ ] **2.3.4.9** Rate-limit middleware (per-IP `DashMap<IpAddr, (u32, Instant)>` in-process + persisted to DB)

---

### POST `/api/sync/push`

**Purpose:** Store an encrypted settings blob. Requires auth token.

**Request:**
```json
{
  "encrypted_blob": "<base64-encoded ciphertext>",
  "sequence_hint": 42
}
```

> `sequence_hint` is the client's last known sequence. Server ignores it but logs for diagnostics.

**Response `200`:**
```json
{ "sequence": 43 }
```

**Logic:**
1. Auth middleware validates token, provides `public_key`
2. Compute `sequence = MAX(sequence FOR public_key) + 1`
3. Insert new `sync_blobs` row
4. Return new sequence number

Tasks:
- [ ] **2.3.4.10** `PushRequest` + `PushResponse` types
- [ ] **2.3.4.11** Push handler with sequence auto-increment

---

### GET `/api/sync/pull?since={sequence}`

**Purpose:** Fetch all encrypted settings blobs since a sequence number. Requires auth token.

**Response `200`:**
```json
{
  "blobs": [
    { "sequence": 43, "encrypted_blob": "...", "pushed_at": "2026-03-01T..." }
  ],
  "latest_sequence": 43
}
```

**Logic:**
1. Auth middleware validates token, provides `public_key`
2. Query `sync_blobs WHERE public_key = $pk AND sequence > $since ORDER BY sequence`
3. Return array (empty if nothing new)

Tasks:
- [ ] **2.3.4.12** `PullResponse` + `BlobEntry` types
- [ ] **2.3.4.13** Pull handler

---

### GET `/api/sync/status`

**Purpose:** Return account info + token metadata for the authenticated client.

**Response `200`:**
```json
{
  "public_key": "<hex>",
  "registered_at": "2026-02-01T...",
  "latest_sequence": 43,
  "token": {
    "device_name": "Linux Desktop",
    "created_at": "2026-02-01T...",
    "last_seen_at": "2026-03-01T...",
    "expires_at": "2027-03-01T..."
  }
}
```

Tasks:
- [ ] **2.3.4.14** `SyncStatusResponse` type
- [ ] **2.3.4.15** Status handler

---

### Admin Endpoints (no auth token required — protected by passphrase in request body)

#### GET `/api/admin/accounts`

Returns all registered accounts + their token count + last seen.

**Request header:** `X-Admin-Passphrase: <passphrase>`

**Response `200`:**
```json
{
  "accounts": [
    {
      "public_key": "<hex>",
      "registered_at": "...",
      "last_seen_at": "...",
      "token_count": 2,
      "blob_count": 15
    }
  ],
  "total_accounts": 1,
  "max_accounts": 10
}
```

#### GET `/api/admin/tokens?public_key={hex}`

List all active tokens for a public key (for remote session management).

#### DELETE `/api/admin/tokens/{token_id}`

Revoke a specific token by ID. The ID is an opaque DB record ID (not the raw token).

Tasks:
- [ ] **2.3.4.16** Admin passphrase middleware (`X-Admin-Passphrase` header, constant-time check)
- [ ] **2.3.4.17** `GET /api/admin/accounts` handler
- [ ] **2.3.4.18** `GET /api/admin/tokens` handler
- [ ] **2.3.4.19** `DELETE /api/admin/tokens/{id}` handler

---

## 2.3.5 utoipa / Swagger Documentation

- [ ] **2.3.5.1** Add `utoipa` + `utoipa-swagger-ui` + `utoipa-axum` to `Cargo.toml`
- [ ] **2.3.5.2** Derive `utoipa::ToSchema` on all request/response types
- [ ] **2.3.5.3** Annotate all handlers with `#[utoipa::path(...)]`
- [ ] **2.3.5.4** Assemble `ApiDoc` via `#[derive(OpenApi)]` macro including all paths + schemas
- [ ] **2.3.5.5** Mount `SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())` in Axum router
- [ ] **2.3.5.6** Add `securitySchemes`: `BearerAuth` (HTTP bearer) + `AdminPassphrase` (API key header)
- [ ] **2.3.5.7** Verify Swagger UI renders correctly at `/swagger-ui` and all examples are accurate
- [ ] **2.3.5.8** Add OpenAPI `info` block: title, version, description, contact, license

---

## 2.3.6 Admin Web UI (Tailwind CSS + Alpine.js SPA)

> **DECISION:** Replaced planned Dioxus fullstack admin UI with a lightweight,
> single-file HTML SPA embedded as a Rust `const &str` in `src/web/mod.rs`.
> No build step, no Dioxus dependency in this crate. Fully functional, simpler
> to maintain, and avoids axum/Dioxus SSR integration complexity.

Dark enterprise-themed SPA served at `/`. Tailwind CSS via CDN + Alpine.js 3.14 via CDN.

### Layout & Pages

#### Login Screen (pre-auth)
- Centered card with Poly brand + server-passphrase field + username/password
- On submit: mines SHA-256 PoW challenge (16-bit difficulty) via Web Crypto API
- PoW mining is async — shows spinner while mining
- `POST /admin/login` → sets `poly_admin` session cookie (HttpOnly, SameSite=Strict)
- Global rate limit: 10 attempts/minute enforced server-side

#### Users Page (sidebar → "Users")
- Table: Public key (truncated + hover full), registered date, last seen, # sessions, # blobs
- Click row → expands inline to show active tokens (device name, last seen, expires, Revoke button)
- Revoke calls `DELETE /admin/api/tokens/:id`

#### Settings Page (sidebar → "Settings")
- Max accounts input (0 = unlimited) → `POST /admin/api/settings`
- Server info panel: version, pow_difficulty, token_expiry_days, bind address

### Auth Security
- Admin PoW challenge: nonce stored in `AdminState.challenges: DashMap<_, _>`
- Session stored as `SHA-256(cookie_value)` in `AdminState.sessions: DashMap<String, Instant>`
- Rate limit: `AdminState.rate: Mutex<AdminLoginTracker>` — 10 attempts/minute global window
- All admin API routes call `check_session()` helper which validates cookie

### Implementation
- `src/web/mod.rs` — all admin backend logic + `ADMIN_HTML: &'static str` const
- No build step: HTML served directly from the constant
- CSS theme vars: `--bg: #070b14`, `--surface: #0d1526`, `--accent: #6366f1`

Tasks:
- [x] **2.3.6.1** Admin router with session auth (cookie-based)
- [x] **2.3.6.2** PoW challenge endpoint (`GET /admin/challenge`)
- [x] **2.3.6.3** Login endpoint with PoW verification + rate limiting (`POST /admin/login`)
- [x] **2.3.6.4** Logout endpoint (`POST /admin/logout`)
- [x] **2.3.6.5** Stats API (`GET /admin/api/stats`)
- [x] **2.3.6.6** Accounts API (`GET /admin/api/accounts`)
- [x] **2.3.6.7** Tokens-per-account API + Revoke token (`GET/DELETE /admin/api/accounts/:pk/tokens`, `DELETE /admin/api/tokens/:id`)
- [x] **2.3.6.8** Settings API (`POST /admin/api/settings`)
- [x] **2.3.6.9** Embedded Tailwind + Alpine.js HTML SPA (login + users + settings)
- [x] **2.3.6.10** Dark enterprise theme matching Poly neutral-dark CSS vars

---

## 2.3.7 Docker / Deployment

- [ ] **2.3.7.1** `Dockerfile` — multi-stage: `cargo build --release` → minimal runtime image
- [ ] **2.3.7.2** `docker-compose.yml` — single-service with volume mount for data dir + env var template
- [ ] **2.3.7.3** `.env.example` documenting all `POLY_*` variables
- [ ] **2.3.7.4** Health check endpoint `GET /health` → `{ "status": "ok", "version": "..." }`
- [ ] **2.3.7.5** Graceful shutdown: drain in-flight requests, close DB cleanly

---

## 2.3.8 Testing

- [ ] **2.3.8.1** Unit tests for PoW verifier (`verify_pow`)
- [ ] **2.3.8.2** Unit tests for token hashing + validation
- [ ] **2.3.8.3** Integration test: full auth round-trip (challenge → auth → token)
- [ ] **2.3.8.4** Integration test: push → pull round-trip (encrypted blob stored and retrieved correctly)
- [ ] **2.3.8.5** Integration test: token expiry enforcement
- [ ] **2.3.8.6** Integration test: rate limiting (N+1 request gets 429)
- [ ] **2.3.8.7** Integration test: max_accounts enforcement (N+1 new pubkey gets 403)

---

## Completion Criteria

- [ ] `cargo test --package poly-backup-server` passes all tests
- [ ] Swagger UI at `/swagger-ui` fully documents all endpoints with accurate schemas
- [ ] Admin UI renders at `/` showing accounts, sessions, stats
- [ ] Full auth round-trip works: challenge → PoW solve → auth → push → pull
- [ ] Docker image builds and runs: `docker compose up`
- [ ] Rate limiting blocks brute force attempts
- [ ] Token revocation prevents further API access immediately
