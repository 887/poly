# poly-backup-server — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-backup-server` is a standalone server application that stores encrypted app settings backups. Users configure this server to keep their Poly settings synchronized across devices. The server knows NOTHING about user data — it stores only encrypted blobs.

## Implementation Phase

**Phase 2** (section 2.8). This is built alongside the core app infrastructure. See [Phase 2 Plan](../../docs/phase-2-plan.md).

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
