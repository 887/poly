# poly-backup-server

Encrypted settings backup/sync server for **Poly** (PolyGlot Messenger).

## Purpose

A standalone Axum HTTP server that stores **encrypted blobs** for Poly clients.
The server **knows nothing** about the content it stores — all encryption happens
on the client before any data leaves the device.

## Features

| Feature | Status |
|---|---|
| Zero-knowledge blob storage | ✅ Implemented |
| PoW challenge (SHA-256, Anubis-style) | ✅ Implemented |
| Passphrase auth (constant-time, SHA-256) | ✅ Implemented |
| 128-char alphanumeric session tokens | ✅ Implemented |
| SHA-256 token hashing (never stored raw) | ✅ Implemented |
| Rolling token expiry (per API call) | ✅ Implemented |
| Device name tracking per session | ✅ Implemented |
| Per-account monotonic sequence numbers | ✅ Implemented |
| Push/pull encrypted blob delta sync | ✅ Implemented |
| Account limit enforcement | ✅ Implemented |
| SurrealKV embedded database | ✅ Implemented |
| OpenAPI 3.1 spec + Swagger UI | ✅ Implemented |
| Admin web UI (Tailwind CSS + Alpine.js) | ✅ Implemented |
| Admin PoW login (anti-bot) | ✅ Implemented |
| Admin rate limiting (10/min global) | ✅ Implemented |
| Admin session cookies (HttpOnly) | ✅ Implemented |
| Graceful shutdown (Ctrl-C + SIGTERM) | ✅ Implemented |
| Docker image | 🔲 TODO (phase-2.3.7) |
| Per-IP API rate limiting | 🔲 TODO (phase-2.3) |
| Persistent max_accounts setting | 🔲 TODO (phase-2.3.6) |

## Running

```bash
POLY_PASSPHRASE="your-secret" \
POLY_ADMIN_USER="admin" \
POLY_ADMIN_PASSWORD="strong-password" \
cargo run -p poly-backup-server
```

**URLs:**
- Admin UI: `http://localhost:8080/`
- Swagger UI: `http://localhost:8080/swagger-ui`
- Health: `http://localhost:8080/api/health`

## Configuration

| Variable | Default | Description |
|---|---|---|
| `POLY_PASSPHRASE` | `changeme` | Server-wide API access passphrase |
| `POLY_MAX_ACCOUNTS` | `0` | Max registered accounts (0 = unlimited) |
| `POLY_TOKEN_EXPIRY_DAYS` | `365` | Session token inactivity expiry in days |
| `POLY_POW_DIFFICULTY` | `20` | API PoW difficulty in leading zero bits |
| `POLY_ADMIN_POW_DIFFICULTY` | `16` | Admin UI PoW difficulty (lower = faster for humans) |
| `POLY_BIND` | `0.0.0.0:8080` | Socket address to listen on |
| `POLY_DATA_DIR` | `./data` | Directory for SurrealKV database files |
| `POLY_ADMIN_USER` | `admin` | Admin UI username |
| `POLY_ADMIN_PASSWORD` | `changeme` | Admin UI password |
| `POLY_ADMIN_SESSION_HOURS` | `4` | Admin session cookie lifetime in hours |
| `POLY_ADMIN_RATE_LIMIT` | `10` | Max admin login attempts per minute (global) |
| `POLY_RATE_LIMIT_MAX` | `5` | Max failed API auth attempts per IP |
| `POLY_RATE_LIMIT_WINDOW_SECS` | `3600` | IP rate-limit window in seconds |

## API Overview

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `/api/challenge` | None | Issue PoW nonce for a public key |
| `POST` | `/api/auth` | PoW + passphrase | Verify and issue 128-char session token |
| `POST` | `/api/sync/push` | Bearer token | Store encrypted blob, return sequence |
| `GET` | `/api/sync/pull?since=N` | Bearer token | Fetch blobs since sequence N |
| `GET` | `/api/sync/status` | Bearer token | Account info + token metadata |
| `GET` | `/api/health` | None | Health check |
| `GET` | `/swagger-ui` | None | Interactive API documentation |

## Admin UI

A dark-themed enterprise-style SPA at `/` — no framework, just Tailwind CSS +
Alpine.js + Web Crypto API.

**Login flow:**
1. Page loads → JS fetches PoW challenge from `/admin/challenge`
2. User enters username + password
3. On submit: browser mines SHA-256 PoW in background (< 1 second at 16-bit difficulty)
4. Solved counter posted to `/admin/login` — server verifies PoW + credentials
5. Session cookie set (HttpOnly, SameSite=Strict)

**Dashboard pages:**
- **Users** — table of all accounts with public key, timestamps, device count, blob count;
  click any row to expand and see active sessions with per-session Revoke button
- **Settings** — update max accounts, view server runtime info

## Database Schema

SurrealKV embedded database, 5 tables:

| Table | Purpose |
|---|---|
| `account` | Registered public keys + timestamps |
| `token` | Session tokens (SHA-256 hash only) + device info |
| `sync_blob` | Encrypted settings blobs (per-account sequence) |
| `challenge` | Short-lived PoW challenges (60s TTL) |
| `rate_limit` | Per-IP failure counters |

## Security Notes

- Server passphrase is compared via **constant-time SHA-256** to prevent timing attacks
- Token values are **never stored** — only `SHA-256(token)` is in the database
- Admin credentials use **constant-time comparison** with SHA-256 hashing
- Admin UI is protected by **PoW + rate limiting** to prevent brute force
- All admin sessions use **HttpOnly, SameSite=Strict** cookies

## License

MIT / Apache-2.0
