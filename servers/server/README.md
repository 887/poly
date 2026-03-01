# poly-server

**Poly Self-Hosted Chat Server** — a lean reference implementation of the Poly messaging protocol.

Run this when you want a fully self-hosted alternative to Stoat/Discord for testing Poly clients,
or as a fallback when third-party services block access.

---

## Features

| Feature | Status |
|---|---|
| Username + password auth | ✅ |
| JWT device sessions | ✅ |
| Remote device revocation | ✅ |
| WebSocket real-time events | ✅ |
| Servers (guilds) with channels | ✅ |
| Text channels | ✅ |
| Voice channels (signalling only) | ✅ |
| Direct messages | ✅ |
| Group DMs | ✅ |
| Message reply threads | ✅ |
| Reactions | ✅ |
| File/image uploads | ✅ |
| Friend requests | ✅ |
| Invite codes | ✅ |
| SurrealDB + SurrealKV (embedded) | ✅ |

---

## Quick Start

```bash
# Dev (with logging)
RUST_LOG=debug cargo run -p poly-server

# Production — override defaults via env
POLY_SERVER_BIND=0.0.0.0:7080 \
POLY_SERVER_DB_PATH=/var/lib/poly-server/db \
POLY_SERVER_JWT_SECRET=$(openssl rand -hex 32) \
POLY_SERVER_NAME="My Poly Server" \
cargo run --release -p poly-server
```

The server is now available at `http://localhost:7080`.

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `POLY_SERVER_BIND` | `127.0.0.1:7080` | Listen address |
| `POLY_SERVER_DB_PATH` | `./data/poly.db` | SurrealKV data directory |
| `POLY_SERVER_NAME` | `Poly Server` | Server display name |
| `POLY_SERVER_INVITE_ONLY` | `false` | Restrict registration to invite codes |
| `POLY_SERVER_JWT_SECRET` | *random (dev)* | JWT signing secret (set in production!) |
| `POLY_SERVER_JWT_EXPIRY_SECS` | `2592000` (30 days) | Token lifetime |
| `POLY_SERVER_UPLOADS_DIR` | `./data/uploads` | Attachment storage directory |

---

## API Overview

See [`docs/poly-server-protocol.md`](../../docs/poly-server-protocol.md) for the full protocol
reference including authentication flows, WebSocket event schema, and access-control rules.

### Auth

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `/auth/signup` | No | Register a new account |
| `POST` | `/auth/signin` | No | Sign in — returns JWT |
| `GET` | `/server-info` | No | Server name, version, invite-only flag |
| `POST` | `/auth/signout` | ✅ | Revoke current device session |
| `GET` | `/auth/devices` | ✅ | List all active device sessions |
| `DELETE` | `/auth/devices/:id` | ✅ | Revoke a specific device remotely |

### Servers (guilds)

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/servers` | ✅ | List servers I belong to |
| `POST` | `/servers` | ✅ | Create a server |
| `GET` | `/servers/:id` | ✅ | Get server details |
| `PATCH` | `/servers/:id` | ✅ (owner) | Update server |
| `DELETE` | `/servers/:id` | ✅ (owner) | Delete server |
| `POST` | `/servers/:id/invites` | ✅ | Create invite code |
| `POST` | `/invites/:code/use` | ✅ | Join via invite |
| `DELETE` | `/servers/:id/members/:uid` | ✅ (owner or self) | Kick / leave |

### Channels

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/servers/:id/channels` | ✅ (member) | List server channels |
| `POST` | `/servers/:id/channels` | ✅ (owner) | Create channel |
| `PATCH` | `/channels/:id` | ✅ (owner) | Update channel |
| `DELETE` | `/channels/:id` | ✅ (owner) | Delete channel |
| `GET` | `/channels/@dms` | ✅ | List my DM channels |
| `POST` | `/channels/@dms` | ✅ | Open a DM with a user |
| `POST` | `/channels/@groups` | ✅ | Create a group DM |

### Messages

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/channels/:id/messages` | ✅ (readable) | List messages (paginated) |
| `POST` | `/channels/:id/messages` | ✅ (readable) | Send a message |
| `PATCH` | `/messages/:id` | ✅ (author) | Edit a message |
| `DELETE` | `/messages/:id` | ✅ (author/owner) | Soft-delete a message |
| `POST` | `/messages/:id/reactions/:emoji` | ✅ | Add reaction |
| `DELETE` | `/messages/:id/reactions/:emoji` | ✅ | Remove reaction |

### Attachments

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `/attachments` | ✅ | Upload a file (max 50 MiB, multipart) |
| `GET` | `/attachments/:id` | ✅ (readable) | Download a file |

### Users & Friends

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/users/me` | ✅ | My profile |
| `PATCH` | `/users/me` | ✅ | Update profile |
| `GET` | `/users/:id` | ✅ | Public user profile |
| `GET` | `/users/me/friends` | ✅ | Friend list |
| `POST` | `/users/me/friends` | ✅ | Send friend request |
| `PATCH` | `/users/me/friends/:id` | ✅ (recipient) | Accept / reject |
| `DELETE` | `/users/me/friends/:id` | ✅ | Remove friend |

### WebSocket

Connect to `/ws?token=<JWT>` after signing in. See protocol doc for event schema.

---

## Development

```bash
# Run with debug logging
RUST_LOG=debug,poly_server=trace cargo run -p poly-server

# Run integration tests (starts a real server on a random port)
cargo test -p poly-server

# Lint
cargo cranky -p poly-server
```

---

## Architecture

```
poly-server/
├── src/
│   ├── main.rs         — Axum app assembly, state, TcpListener
│   ├── config.rs       — Config::from_env()
│   ├── error.rs        — AppError → HTTP status mapping
│   ├── db.rs           — SurrealKV init + SCHEMA migration
│   ├── models/         — Rust types for every DB table
│   ├── auth/
│   │   ├── mod.rs      — JWT Claims, auth_middleware
│   │   └── routes.rs   — /auth/* handlers
│   ├── api/
│   │   ├── mod.rs      — Sub-router aggregator
│   │   ├── servers.rs  — /servers/* handlers
│   │   ├── channels.rs — /channels/* + DMs handlers
│   │   ├── messages.rs — /messages/* + reactions handlers
│   │   ├── users.rs    — /users/* + friends handlers
│   │   └── upload.rs   — /attachments upload + serve
│   └── ws/
│       ├── mod.rs      — WebSocket connection handling
│       └── events.rs   — ServerEvent enum (wire format)
└── tests/
    └── integration.rs  — End-to-end test suite
```

All data lives in a SurrealKV embedded database — zero external dependencies beyond the binary itself.
