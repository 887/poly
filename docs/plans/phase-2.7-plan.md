# Phase 2.7 Plan — Poly Server Test Client

> **Status:** ⬜ Not Started  
> **Moved from:** Phase 3.0 (Poly-Server Test Client)  
> **Priority:** HIGH — first real backend integration, validates `ClientBackend` path  
> **Parent:** [Overall Plan](overall-plan.md) | [Phase 2 Plan](phase-2-plan.md)  
> **Protocol:** [poly-server-protocol.md](poly-server-protocol.md)  

---

## Overview

Build a full `ClientBackend` implementation for our own `poly-server`, proving the entire account lifecycle end-to-end before tackling external protocols (Stoat, Matrix, Discord, Teams).

**Key design decisions:**
- **Ed25519 challenge-response auth** — no passwords. Users sign up and sign in using the identity keypair generated during the setup wizard. The server stores the public key; the client proves ownership by signing a random challenge.
- **Multiple server support** — users can connect to multiple poly-server instances. `127.0.0.1:7080` is the built-in default, editable during signup.
- **UserID-based identity** — each server assigns a stable `user:ulid` ID. Usernames are unique per-server but freely changeable. Display names are cosmetic.
- **Backup server default URL** — pre-fill text inputs with `http://127.0.0.1:8080` (editable).

---

## 2.7.0 Pre-Work: Auth Model Change (Server-Side)

> **Goal:** Replace argon2 password auth with Ed25519 challenge-response on the poly-server.

### 2.7.0.1 New Auth Flow Design

```
┌─ SIGNUP ──────────────────────────────────────────────────────────┐
│                                                                    │
│  Client                                   Server                   │
│    │                                        │                      │
│    │  POST /auth/signup                     │                      │
│    │  { public_key, email, username, display_name } │              │
│    │ ─────────────────────────────────────► │                      │
│    │   (server stores public_key + user)    │                      │
│    │ ◄──────────────────────────────────── │                      │
│    │   201 { token, user_id, device_id }    │                      │
│    │                                        │                      │
└────┘────────────────────────────────────────┘──────────────────────┘

┌─ SIGNIN (challenge-response) ─────────────────────────────────────┐
│                                                                    │
│  Client                                   Server                   │
│    │                                        │                      │
│    │  POST /auth/challenge                  │                      │
│    │  { public_key }                        │                      │
│    │ ─────────────────────────────────────► │                      │
│    │   (server generates 32-byte nonce)     │                      │
│    │ ◄──────────────────────────────────── │                      │
│    │   200 { challenge, expires_at }        │                      │
│    │                                        │                      │
│    │  POST /auth/verify                     │                      │
│    │  { public_key, challenge, signature }  │                      │
│    │ ─────────────────────────────────────► │                      │
│    │   (server verifies Ed25519 sig)        │                      │
│    │ ◄──────────────────────────────────── │                      │
│    │   200 { token, user_id, device_id }    │                      │
│    │                                        │                      │
└────┘────────────────────────────────────────┘──────────────────────┘
```

### Checklist

- [ ] **2.7.0.2** Add `ed25519-dalek` to poly-server `Cargo.toml` dependencies
- [ ] **2.7.0.3** Add `hex` to poly-server dependencies (for public key encoding)
- [ ] **2.7.0.4** Add `rand` to poly-server dependencies (for challenge nonce generation)
- [ ] **2.7.0.5** Update `user` DB schema — add `public_key: String` field, remove `password_hash` requirement (make optional for migration)
- [ ] **2.7.0.6** Create `challenge` DB table — `{ id, public_key, nonce, expires_at, used }`
- [ ] **2.7.0.7** Implement `POST /auth/signup` (key-based) — accept `{ public_key, email, username, display_name }`, store hex-encoded public key, reject duplicates on username, email, and public_key
- [ ] **2.7.0.8** Implement `POST /auth/challenge` — accept `{ public_key }`, look up user, generate 32-byte random nonce, store in `challenge` table with 60s TTL, return `{ challenge: hex_nonce, expires_at }`
- [ ] **2.7.0.9** Implement `POST /auth/verify` — accept `{ public_key, challenge, signature }`, verify Ed25519 signature of the challenge bytes using stored public key, invalidate challenge, create device, return `{ token, user_id, device_id }`
- [ ] **2.7.0.10** Remove old password fields from `SignupRequest`/`SigninRequest` (or keep as optional for migration path)
- [ ] **2.7.0.11** Remove `argon2` dependency from poly-server (no longer needed)
- [ ] **2.7.0.12** Update `POST /auth/signin` to redirect to challenge flow (or remove entirely)
- [ ] **2.7.0.13** Add challenge expiry cleanup — expired challenges auto-deleted (background task or on-demand)
- [ ] **2.7.0.14** Update existing integration tests to use key-based auth
- [ ] **2.7.0.15** Update `docs/poly-server-protocol.md` to reflect new auth flow

---

## 2.7.1 Server-Client Crate (`servers/server-client/`)

> **Goal:** HTTP + WebSocket client library for connecting to poly-server instances.

- [ ] **2.7.1.1** Create `servers/server-client/` crate with `Cargo.toml`, `agents.md`, `README.md`, `cranky.toml`
- [ ] **2.7.1.2** Dependencies: `reqwest` (JSON + multipart), `tokio-tungstenite` (native-tls), `ed25519-dalek`, `hex`, `serde`, `serde_json`, `thiserror`, `tracing`, `tokio`, `futures`, `tokio-stream` 
- [ ] **2.7.1.3** Define `PolyServerConfig` — `{ base_url: String, public_key: [u8; 32], private_key: [u8; 32] }`
- [ ] **2.7.1.4** Implement `PolyServerHttpClient` — reqwest-based HTTP client with `Authorization: Bearer` header injection
- [ ] **2.7.1.5** Implement `signup(username, email, display_name)` — POST /auth/signup with public_key
- [ ] **2.7.1.6** Implement `signin()` — POST /auth/challenge → sign nonce → POST /auth/verify
- [ ] **2.7.1.7** Implement `signout()` — POST /auth/signout
- [ ] **2.7.1.8** Implement `server_info()` — GET /server-info
- [ ] **2.7.1.9** Implement `get_servers()` → GET /servers
- [ ] **2.7.1.10** Implement `get_channels(server_id)` → GET /servers/:id/channels
- [ ] **2.7.1.11** Implement `get_messages(channel_id, query)` → GET /channels/:id/messages
- [ ] **2.7.1.12** Implement `send_message(channel_id, content)` → POST /channels/:id/messages
- [ ] **2.7.1.13** Implement `get_dm_channels()` → GET /channels/@dms
- [ ] **2.7.1.14** Implement `create_dm(user_id)` → POST /channels/dm
- [ ] **2.7.1.15** Implement `get_user(id)` → GET /users/:id
- [ ] **2.7.1.16** Implement `get_friends()` → GET /friends
- [ ] **2.7.1.17** Implement `upload_attachment(file)` → POST /attachments (multipart)
- [ ] **2.7.1.18** Implement `create_server(name)` → POST /servers
- [ ] **2.7.1.19** Implement `create_invite(server_id)` → POST /servers/:id/invites
- [ ] **2.7.1.20** Implement `use_invite(code)` → POST /invites/:code/use
- [ ] **2.7.1.21** Implement `get_devices()` → GET /auth/devices
- [ ] **2.7.1.22** Implement `revoke_device(device_id)` → DELETE /auth/devices/:id

### WebSocket Client

- [ ] **2.7.1.23** Implement WS connection to `ws://host/ws?token=<JWT>` using tokio-tungstenite
- [ ] **2.7.1.24** Auto-reconnect with exponential backoff on disconnect
- [ ] **2.7.1.25** Parse `ServerEvent` JSON frames into typed enum
- [ ] **2.7.1.26** Expose event stream as `tokio::sync::broadcast` or `async Stream`
- [ ] **2.7.1.27** Send `typing_start` client messages
- [ ] **2.7.1.28** Send `heartbeat` client messages periodically
- [ ] **2.7.1.29** Handle `device_revoked` event → trigger logout

---

## 2.7.2 `ClientBackend` Implementation

> **Goal:** Bridge `PolyServerHttpClient` + WS into the `poly-client::ClientBackend` trait.

- [ ] **2.7.2.1** Create `PolyServerBackend` struct implementing `ClientBackend`
- [ ] **2.7.2.2** Implement `authenticate()` — use Ed25519 challenge-response flow
- [ ] **2.7.2.3** Implement `logout()` — call signout + disconnect WS
- [ ] **2.7.2.4** Implement `is_authenticated()` — check token presence + WS connected
- [ ] **2.7.2.5** Implement `get_servers()` — map poly-server `Server` → `poly_client::Server`
- [ ] **2.7.2.6** Implement `get_server(id)` — fetch + map single server
- [ ] **2.7.2.7** Implement `get_channels(server_id)` — map channels + categories
- [ ] **2.7.2.8** Implement `get_channel(id)` — fetch + map single channel
- [ ] **2.7.2.9** Implement `send_message()` — map `MessageContent` → API call
- [ ] **2.7.2.10** Implement `get_messages()` — map API pagination to `MessageQuery`
- [ ] **2.7.2.11** Implement `get_user(id)` — fetch + map user
- [ ] **2.7.2.12** Implement `get_friends()` — fetch + map friend list
- [ ] **2.7.2.13** Implement `get_channel_members()` — fetch server members for channel
- [ ] **2.7.2.14** Implement `get_groups()` — map group DM channels
- [ ] **2.7.2.15** Implement `get_dm_channels()` — fetch + map DMs
- [ ] **2.7.2.16** Implement `get_notifications()` — derive from unread state (poly-server doesn't have a dedicated notifications API yet)
- [ ] **2.7.2.17** Implement `get_voice_participants()` — stub (voice comes later)
- [ ] **2.7.2.18** Implement `get_presence()`/`set_presence()` — map to poly-server user status
- [ ] **2.7.2.19** Implement `event_stream()` — bridge WS events → `ClientEvent` stream
- [ ] **2.7.2.20** Implement `backend_type()` → `BackendType::PolyServer`
- [ ] **2.7.2.21** Implement `backend_name()` → `"Poly Server"`
- [ ] **2.7.2.22** Add `BackendType::PolyServer` variant to `poly-client` enum (if not present)
- [ ] **2.7.2.23** Register `PolyServerBackend` in `ClientManager` startup flow

---

## 2.7.3 Settings UI: Add Poly Server Account

> **Goal:** UI flow in Settings → Accounts to connect to a poly-server instance.

### Add Account Wizard

- [ ] **2.7.3.1** Replace `AccountsSettings` stub with full implementation
- [ ] **2.7.3.2** "Add Account" button opens a backend picker (Poly Server, Stoat, Matrix, Discord, Teams)
- [ ] **2.7.3.3** "Poly Server" picker opens the poly-server connection wizard
- [ ] **2.7.3.4** **Step 1: Server URL** — text input pre-filled with `http://127.0.0.1:7080`, "Connect" button → probe `/server-info`
- [ ] **2.7.3.5** Show server name + version + invite_only status from probe
- [ ] **2.7.3.6** **Step 2: Sign Up / Sign In** toggle
- [ ] **2.7.3.7** Sign Up form: username input + display name input + "Create Account" button
- [ ] **2.7.3.8** Sign In: automatic — uses existing Ed25519 keypair to challenge-response authenticate. Button: "Sign In with Identity Key"
- [ ] **2.7.3.9** On success: store `AccountToken { backend: "poly-server", account_id: user_id, token, display_name }` in storage + store server URL
- [ ] **2.7.3.10** On error: show error message (username taken, server unreachable, etc.)
- [ ] **2.7.3.11** Show connected poly-server accounts in the accounts list with status indicator

### Account Management

- [ ] **2.7.3.12** Per-account view: server name, user info, connected devices list
- [ ] **2.7.3.13** "Change Username" action — PUT /users/@me (needs new endpoint)
- [ ] **2.7.3.14** "Change Display Name" action — PUT /users/@me
- [ ] **2.7.3.15** "Disconnect" button — signs out from the server, removes token
- [ ] **2.7.3.16** "Remove Account" button — full disconnect + remove stored data

### Multiple Server Support

- [ ] **2.7.3.17** Support adding multiple poly-server connections (different URLs)
- [ ] **2.7.3.18** Each poly-server appears as a server source in the sidebar with "Poly" badge
- [ ] **2.7.3.19** Different poly-server instances can have different user accounts (same keypair)

---

## 2.7.4 Backup Server: Default URL in Settings

> **Goal:** Pre-fill the backup server URL input with the default `http://127.0.0.1:8080`.

- [ ] **2.7.4.1** Update all `locales/*/main.ftl` — change `settings-backup-url-placeholder` from `https://backup.example.com` to `http://127.0.0.1:8080`
- [ ] **2.7.4.2** Pre-fill the URL text input with the default value (not just placeholder)
- [ ] **2.7.4.3** Make the URL fully editable so users can change it to their own server
- [ ] **2.7.4.4** Ensure the probe flow still works correctly with pre-filled URL

---

## 2.7.5 Poly Server: Minor Fixes (from Phase 2.6.12)

> **Goal:** Fix known issues in poly-server before integration testing.

- [ ] **2.7.5.1** Add `PUT /users/@me` endpoint — update username and/or display_name
- [ ] **2.7.5.2** Cascade delete on server deletion (channels, messages, memberships, categories)
- [ ] **2.7.5.3** Wire `typing_start` WS client message to broadcast `typing_start` server event
- [ ] **2.7.5.4** Wire `heartbeat` WS client message to update `device.last_seen`
- [ ] **2.7.5.5** Wire `voice_signal` WS relay (forward to target user)
- [ ] **2.7.5.6** Fix `test_file_upload_and_access` — uses wrong URL patterns for invite routes
- [ ] **2.7.5.7** Emit `server_member_added`/`server_member_removed` WS events on join/leave
- [ ] **2.7.5.8** Emit `channel_created`/`channel_deleted` WS events on channel CRUD

---

## 2.7.6 End-to-End Integration Tests

> **Goal:** Full E2E test suite like `backup-server` has, proving the complete lifecycle.

### Test Infrastructure

- [ ] **2.7.6.1** Create `servers/server-client/tests/integration.rs`
- [ ] **2.7.6.2** Test helper: spawn poly-server on random port with temp DB
- [ ] **2.7.6.3** Test helper: generate Ed25519 keypair for test users
- [ ] **2.7.6.4** Test helper: create authenticated `PolyServerHttpClient` (signup + auto-token)

### Auth Tests

- [ ] **2.7.6.5** `test_signup_and_signin` — signup with key, challenge-response signin, verify token works
- [ ] **2.7.6.6** `test_duplicate_username_rejected` — signup two users with same username fails
- [ ] **2.7.6.7** `test_duplicate_public_key_rejected` — signup same key twice fails
- [ ] **2.7.6.8** `test_challenge_expiry` — request challenge, wait past expiry, verify fails
- [ ] **2.7.6.9** `test_wrong_signature_rejected` — sign challenge with wrong key, verify fails
- [ ] **2.7.6.10** `test_device_list_and_revoke` — list devices, revoke one, verify token invalid
- [ ] **2.7.6.11** `test_signout` — signout revokes current device session

### Server CRUD Tests

- [ ] **2.7.6.12** `test_create_server_and_list` — create server, verify in list
- [ ] **2.7.6.13** `test_server_invite_flow` — create invite, second user uses it, verify membership
- [ ] **2.7.6.14** `test_server_update_and_delete` — update name, delete, verify cleanup

### Channel & Message Tests

- [ ] **2.7.6.15** `test_create_channel_and_send_message` — create channel, send message, get messages
- [ ] **2.7.6.16** `test_message_pagination` — send 60 messages, paginate with before cursor
- [ ] **2.7.6.17** `test_dm_channel_flow` — create DM, send messages, list DMs
- [ ] **2.7.6.18** `test_message_edit_and_delete` — edit message, soft-delete, verify content replaced
- [ ] **2.7.6.19** `test_reactions` — add/remove reactions, list reactions on message
- [ ] **2.7.6.20** `test_file_upload_and_attach` — upload file, attach to message, download

### WebSocket Tests

- [ ] **2.7.6.21** `test_ws_connect_and_ping` — connect WS, receive ping event
- [ ] **2.7.6.22** `test_ws_message_created_event` — user A sends message, user B receives WS event
- [ ] **2.7.6.23** `test_ws_message_edited_event` — edit message, verify WS event
- [ ] **2.7.6.24** `test_ws_device_revoked_event` — revoke device, verify WS disconnect event
- [ ] **2.7.6.25** `test_ws_reconnect` — disconnect WS, reconnect with same token, verify events resume

### ClientBackend Tests

- [ ] **2.7.6.26** `test_client_backend_lifecycle` — authenticate, get_servers, get_channels, send_message, logout
- [ ] **2.7.6.27** `test_client_backend_event_stream` — verify event_stream produces ClientEvents

---

## 2.7.7 Visual Verification via Desktop DevTools MCP

> **Goal:** Launch the app, connect to local poly-server, verify all UI flows work visually.

- [ ] **2.7.7.1** Build `desktop-devtools` with poly-server feature
- [ ] **2.7.7.2** Launch local poly-server instance
- [ ] **2.7.7.3** Navigate to Settings → Accounts
- [ ] **2.7.7.4** Screenshot: "Add Account" picker showing Poly Server option
- [ ] **2.7.7.5** Screenshot: Server URL step with pre-filled `127.0.0.1:7080`
- [ ] **2.7.7.6** Screenshot: Sign Up form with username + display name
- [ ] **2.7.7.7** Screenshot: Successfully connected account in account list
- [ ] **2.7.7.8** Navigate to server sidebar — verify poly-server appears with "Poly" badge
- [ ] **2.7.7.9** Screenshot: Channel list from poly-server
- [ ] **2.7.7.10** Screenshot: Send a message in a channel, verify it appears
- [ ] **2.7.7.11** Screenshot: Backup settings with pre-filled URL
- [ ] **2.7.7.12** Fix any visual issues found during verification

---

## 2.7.8 Documentation Updates

- [ ] **2.7.8.1** Update `docs/poly-server-protocol.md` — new auth section with challenge-response flow
- [ ] **2.7.8.2** Create `servers/server-client/agents.md` — crate architecture and usage
- [ ] **2.7.8.3** Create `servers/server-client/README.md` — public API docs
- [ ] **2.7.8.4** Update `crates/core/agents.md` — document poly-server backend integration
- [ ] **2.7.8.5** Update `docs/overall-plan.md` — note Phase 2.7 completion
- [ ] **2.7.8.6** Update `docs/phase-3-plan.md` — remove section 3.0 (moved to 2.7)

---

## Completion Criteria

- [ ] Poly-server uses Ed25519 challenge-response auth (no passwords)
- [ ] `servers/server-client/` crate exists with full HTTP + WS client
- [ ] `PolyServerBackend` implements `ClientBackend` trait
- [ ] Settings → Accounts can add/remove poly-server connections
- [ ] Default server URL `http://127.0.0.1:7080` pre-filled in UI
- [ ] Backup server URL `http://127.0.0.1:8080` pre-filled in UI
- [ ] Usernames are per-server, changeable; identity is by UserID
- [ ] Multiple poly-server connections supported
- [ ] All E2E integration tests pass
- [ ] Visual verification via desktop-devtools confirms UI works
- [ ] Protocol documentation updated
- [ ] `cargo cranky --workspace` passes with zero warnings

---

## Implementation Order

1. **2.7.0** — Server-side auth model change (prerequisite for everything)
2. **2.7.5** — Poly server minor fixes (clean up before client work)
3. **2.7.1** — Server-client crate (HTTP + WS)
4. **2.7.2** — `ClientBackend` implementation
5. **2.7.3** — Settings UI (accounts wizard)
6. **2.7.4** — Backup server default URL
7. **2.7.6** — E2E integration tests
8. **2.7.7** — Visual verification
9. **2.7.8** — Documentation

---
