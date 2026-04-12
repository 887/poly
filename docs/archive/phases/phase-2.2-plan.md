# Phase 2.2 Plan — Poly Server (Self-Hosted Chat Backend)

> **Status:** 🔄 In Progress — Scaffold + Compile-Clean  
> **Crate:** `servers/server`  
> **Parent:** [Phase 2 Plan](phase-2-plan.md)  
> **Last Updated:** 2026-02-28

Poly Server is a lean, self-hosted reference chat backend. It exists so Poly the
client has something useful to connect to when commercial platforms block
third-party clients. It is **not** a feature-complete Discord replacement — it is
a clean, hackable reference implementation.

---

## Design Principles

- **Lean** — no feature creep; every endpoint serves the client UX
- **SurrealDB-native auth** — users, sessions, and permissions all live in the DB; no
  separate JWT signing key, no hand-rolled session store
- **WebSocket-first real-time** — one persistent WS connection per client for all push
  events (messages, presence, typing, device revocation)
- **Axum** — same web framework as `poly-backup-server`; matches workspace conventions
- **No media server** — voice/video signalling only (WebRTC SDP relay); actual audio/
  video flows peer-to-peer

---

## 2.2.1 Database Schema & SurrealDB Auth

- [ ] **2.2.1.1** Initialize SurrealDB with SurrealKV backend, `DEFINE NAMESPACE poly; DEFINE DATABASE server;`
- [ ] **2.2.1.2** Define `DEFINE ACCESS user ON DATABASE TYPE RECORD` with:
  - `SIGNUP`: hash password via `crypto::argon2::generate`, create `user` record
  - `SIGNIN`: verify argon2 hash, return session token
  - `DURATION FOR SESSION 30d, FOR TOKEN 1h`
- [ ] **2.2.1.3** Define `user` table
  - Fields: `username` (unique index), `display_name`, `avatar_url`, `created_at`, `password` (hashed)
  - Permissions: `FOR select FULL; FOR update, delete WHERE id = $auth`
- [ ] **2.2.1.4** Define `device` table (for device management / forced logout)
  - Fields: `owner` (→ user), `name`, `user_agent`, `ip`, `created_at`, `last_seen`, `revoked`
  - Created on every signin; token carries `device_id`
  - Permissions: server-side only (root access)
- [ ] **2.2.1.5** Define `server` table (guilds/communities)
  - Fields: `name`, `icon_url`, `owner` (→ user), `created_at`
  - Permissions: `FOR select WHERE $auth IN members; FOR create WHERE $auth != NONE; FOR update, delete WHERE owner = $auth`
- [ ] **2.2.1.6** Define `membership` table (user ↔ server many-to-many)
  - Fields: `user` (→ user), `server` (→ server), `roles`, `joined_at`
  - Unique index on `(user, server)`
- [ ] **2.2.1.7** Define `category` table (channel groups within a server)
  - Fields: `server` (→ server), `name`, `position`
- [ ] **2.2.1.8** Define `channel` table
  - Fields: `server` (→ server, nullable for DMs/groups), `category` (→ category, nullable), `name`, `kind` (`text` | `voice`), `position`
  - Permissions: `FOR select WHERE server IN $auth->membership->server OR $auth IN participants`
- [ ] **2.2.1.9** Define `participant` table (user ↔ DM/group channel)
  - Fields: `user` (→ user), `channel` (→ channel), `added_at`
- [ ] **2.2.1.10** Define `message` table
  - Fields: `channel` (→ channel), `author` (→ user), `content`, `reply_to` (→ message, nullable), `edited_at` (nullable), `deleted` (bool, soft-delete), `created_at`
  - Permissions: `FOR select WHERE channel IN readable_channels; FOR create WHERE $auth != NONE; FOR update, delete WHERE author = $auth`
- [ ] **2.2.1.11** Define `reaction` table (message reactions)
  - Fields: `message` (→ message), `user` (→ user), `emoji`
  - Unique index on `(message, user, emoji)`
- [ ] **2.2.1.12** Define `friend_request` table
  - Fields: `from` (→ user), `to` (→ user), `status` (`pending` | `accepted` | `rejected`), `created_at`
- [ ] **2.2.1.13** Define `voice_session` table (for who is in a voice channel)
  - Fields: `user` (→ user), `channel` (→ channel), `joined_at`
  - Ephemeral; cleared on server restart / on disconnect

---

## 2.2.2 HTTP API — Auth

- [ ] **2.2.2.1** `POST /auth/signup` — `{ username, password, display_name }` → `{ token, user_id, device_id }`
- [ ] **2.2.2.2** `POST /auth/signin` — `{ username, password, device_name }` → `{ token, user_id, device_id }`
- [ ] **2.2.2.3** `POST /auth/signout` — bearer token → revoke current device, invalidate session
- [ ] **2.2.2.4** `GET /auth/devices` — list all devices for current user (name, ip, last_seen, revoked)
- [ ] **2.2.2.5** `DELETE /auth/devices/:device_id` — revoke a specific device (also triggers WS push to that device)

---

## 2.2.3 HTTP API — Users

- [ ] **2.2.3.1** `GET /users/me` — current user profile
- [ ] **2.2.3.2** `PATCH /users/me` — update display_name, avatar_url
- [ ] **2.2.3.3** `GET /users/:id` — public profile (display_name, avatar_url, status)
- [ ] **2.2.3.4** `GET /users/me/friends` — list accepted friends
- [ ] **2.2.3.5** `POST /users/me/friends` — send friend request `{ username }`
- [ ] **2.2.3.6** `PATCH /users/me/friends/:request_id` — accept / reject friend request
- [ ] **2.2.3.7** `DELETE /users/me/friends/:user_id` — remove friend

---

## 2.2.4 HTTP API — Servers (Guilds)

- [ ] **2.2.4.1** `GET /servers` — list servers current user is member of
- [ ] **2.2.4.2** `POST /servers` — create server `{ name, icon_url? }`
- [ ] **2.2.4.3** `GET /servers/:id` — server detail + members + channels + categories
- [ ] **2.2.4.4** `PATCH /servers/:id` — update server (owner only)
- [ ] **2.2.4.5** `DELETE /servers/:id` — delete server (owner only)
- [ ] **2.2.4.6** `POST /servers/:id/invite` — generate invite code (stored in DB, expires)
- [ ] **2.2.4.7** `POST /servers/join/:invite_code` — join server via invite
- [ ] **2.2.4.8** `DELETE /servers/:id/members/me` — leave server
- [ ] **2.2.4.9** `DELETE /servers/:id/members/:user_id` — kick member (owner only)

---

## 2.2.5 HTTP API — Channels

- [ ] **2.2.5.1** `GET /servers/:server_id/channels` — list all channels + categories
- [ ] **2.2.5.2** `POST /servers/:server_id/channels` — create channel `{ name, kind, category_id? }`
- [ ] **2.2.5.3** `PATCH /channels/:id` — update channel (owner/admin only)
- [ ] **2.2.5.4** `DELETE /channels/:id` — delete channel
- [ ] **2.2.5.5** `POST /servers/:server_id/categories` — create category
- [ ] **2.2.5.6** `PATCH /categories/:id` — rename / reorder category
- [ ] **2.2.5.7** `GET /channels/@dms` — list all DM + group channels for current user
- [ ] **2.2.5.8** `POST /channels/@dms` — open DM with a user `{ user_id }`
- [ ] **2.2.5.9** `POST /channels/@groups` — create group DM `{ user_ids[], name }`

---

## 2.2.6 HTTP API — Messages

- [ ] **2.2.6.1** `GET /channels/:id/messages?before=&limit=` — paginated message history (newest first)
- [ ] **2.2.6.2** `POST /channels/:id/messages` — send message `{ content, reply_to? }`
- [ ] **2.2.6.3** `PATCH /messages/:id` — edit message (author only)
- [ ] **2.2.6.4** `DELETE /messages/:id` — soft-delete message (author only)
- [ ] **2.2.6.5** `POST /messages/:id/reactions` — add reaction `{ emoji }`
- [ ] **2.2.6.6** `DELETE /messages/:id/reactions/:emoji` — remove own reaction

---

## 2.2.7 WebSocket — Real-Time Events

- [ ] **2.2.7.1** `GET /ws` — upgrade endpoint; auth via `?token=` query param or `Authorization` header
- [ ] **2.2.7.2** Connection manager: `Arc<RwLock<HashMap<UserId, Vec<WsSender>>>>` (one user can have multiple devices connected)
- [ ] **2.2.7.3** Server → client events (JSON envelope `{ "event": "...", "data": {} }`):
  - `MessageCreated` — new message in any subscribed channel
  - `MessageEdited` — message content changed
  - `MessageDeleted` — message soft-deleted
  - `ReactionAdded` / `ReactionRemoved`
  - `TypingStart` — user started typing
  - `PresenceUpdate` — user online/offline
  - `DeviceRevoked` — this device's session was revoked → client should sign out
  - `VoiceStateUpdate` — user joined/left voice channel
  - `FriendRequestReceived`
  - `FriendRequestAccepted`
  - `ServerMemberJoined` / `ServerMemberLeft`
- [ ] **2.2.7.4** Client → server messages:
  - `TypingStart { channel_id }` — broadcast typing indicator to channel members (3s TTL)
  - `Heartbeat` / `Pong` — keepalive (server pings every 30s, kicks dead connections)
  - `VoiceJoin { channel_id }` / `VoiceLeave { channel_id }`
  - `VoiceSignal { target_user_id, sdp }` — WebRTC SDP/ICE relay
- [ ] **2.2.7.5** Subscription: on connect, subscribe user to all their server channels + DMs automatically
- [ ] **2.2.7.6** Broadcast helpers: `broadcast_to_channel(channel_id, event)`, `broadcast_to_server(server_id, event)`, `send_to_user(user_id, event)`

---

## 2.2.8 Voice Channel (WebRTC Signalling)

- [ ] **2.2.8.1** `POST /channels/:id/voice/join` — register in `voice_session`, notify channel via WS
- [ ] **2.2.8.2** `POST /channels/:id/voice/leave` — remove `voice_session`, notify channel via WS
- [ ] **2.2.8.3** `GET /channels/:id/voice/members` — who is currently in the voice channel
- [ ] **2.2.8.4** WS relay: `VoiceSignal` events routed peer-to-peer through server (SDP offer/answer, ICE candidates)

---

## 2.2.9 Server Configuration

- [ ] **2.2.9.1** Config via env vars / CLI flags: `BIND_ADDR`, `DB_PATH`, `MAX_ACCOUNTS`, `INVITE_ONLY`
- [ ] **2.2.9.2** `INVITE_ONLY` mode — signup requires a valid invite code
- [ ] **2.2.9.3** `GET /server-info` — public endpoint: server name, version, invite-only flag, user count
- [ ] **2.2.9.4** Admin endpoints (authenticated as a configured admin user):
  - `GET /admin/users` — list all users
  - `DELETE /admin/users/:id` — delete user and all their content
  - `POST /admin/invites` — generate global invite code (for invite-only mode)

---

## 2.2.10 Poly Server Client (poly-core integration)

- [ ] **2.2.10.1** Add `poly-server` feature flag to `poly-core` and `poly-client`
- [ ] **2.2.10.2** Implement `ClientBackend` for `PolyServerClient` (REST + WS)
- [ ] **2.2.10.3** Auth flow in settings UI: hostname + username + password → signup OR signin
- [ ] **2.2.10.4** WS reconnect with exponential backoff on disconnect
- [ ] **2.2.10.5** Map Poly Server API responses to shared `poly-client` types

---

## 2.2.11 UI Extensions (client-side, in poly-core)

- [ ] **2.2.11.1** Settings → Accounts: **Logout** button per account (calls `POST /auth/signout`, removes token, removes from UI)
- [ ] **2.2.11.2** Settings → Accounts → Device Management: list devices, revoke device button
- [ ] **2.2.11.3** Server sidebar — **right-click context menu** on server icon:
  - Remove from Favourites
  - Copy Server ID
  - Leave Server (for non-owned)
  - Delete Server (for owner)
- [ ] **2.2.11.4** User list / DM list — **right-click context menu** on user:
  - Send Direct Message
  - Remove from Favourites (if favourited)
  - Add Friend / Remove Friend
  - View Profile
- [ ] **2.2.11.5** Message — **right-click context menu**:
  - Reply
  - Edit (own messages)
  - Delete (own messages)
  - Copy Text
  - React (emoji picker)

---

## 2.2.12 Testing

- [x] **2.2.12.1** Integration test file created: `servers/server/tests/integration_test.rs` — auth flow scaffold
- [ ] **2.2.12.2** Test: message send → WS broadcast received by other connected client
- [ ] **2.2.12.3** Test: device revocation → revoked WS receives `DeviceRevoked` and closes
- [ ] **2.2.12.4** Test: SurrealDB permissions — user B cannot delete user A's message

---

---

## Session Notes

### Session 2026-02-28 — Scaffold + Compile Fixes

**Scaffold created (all source files):**
- `src/main.rs`, `src/lib.rs`, `src/error.rs`, `src/config.rs`, `src/db.rs`
- `src/models/mod.rs` — all 14 tables as Rust structs with `String` IDs (SurrealDB 3.x)
- `src/auth/mod.rs`, `src/auth/routes.rs`
- `src/api/servers.rs`, `src/api/channels.rs`, `src/api/messages.rs`, `src/api/users.rs`, `src/api/upload.rs`
- `src/ws/mod.rs`, `src/ws/events.rs`
- `cranky.toml`, `README.md`, `agents.md`, `tests/integration_test.rs`
- `docs/poly-server-protocol.md` — client/server protocol specification

**SurrealDB 3.0.1 compatibility fixes applied to ALL files:**
- `SurrealKV` engine → `SurrealKv`
- `surrealdb::sql::Thing` removed — all ID/FK fields use `String`
- All `.bind(("key", &String))` → `.bind(("key", owned_string))`
- All `.take::<ModelType>()` → `serde_json::Value` intermediate + `serde_json::from_value::<T>()`
- `.take::<Option<i64>>("count")` for `SELECT count() GROUP ALL` — **works correctly** (confirmed source)
- `.take::<Option<String>>("field")` for scalar field extraction — **works correctly**

**`cargo check -p poly-server`**: 0 errors, 7 expected dead-code warnings on public API stubs

**Implementation status of HTTP/WS handlers:** all endpoint scaffolds are wired up with SurrealDB queries and WebSocket broadcasts. The implementation is a functional reference; end-to-end tests and client integration remain.

---

## Completion Criteria

- [ ] Server starts with `cargo run -p poly-server` pointing at a local DB path
- [ ] Demo client (`poly-demo`) can be replaced by `PolyServerClient` connecting to local server
- [ ] Can: sign up, sign in, create server, create channels, send messages, receive messages in real-time
- [ ] Device management: list devices, revoke one, affected WS disconnects
- [ ] Voice: two clients can join a voice channel and exchange WebRTC signals
- [ ] Right-click context menus work in the client for servers + users + messages
- [ ] Account logout clears tokens and removes the account from the UI
