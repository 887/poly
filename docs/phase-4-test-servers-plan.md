# Phase 4 Plan — Test Server Suite & Demo Data

> **Created:** 2026-04-03
> **Status:** 🟡 In Progress
> **Goal:** Ship a suite of mock server binaries (one per messenger backend) so developers can do full manual UI testing and automated E2E tests without connecting to real instances like matrix.org, discord.com, etc.
> **Depends on:** Phase 3.2 (Matrix client), existing Stoat + Poly Server implementations

---

## Design Principles

1. **Each backend gets its own standalone test server binary** — small axum app implementing the minimum API surface the Poly plugin actually calls.
2. **All servers follow the existing `TestServer` pattern** — dynamic port via `TcpListener::bind("127.0.0.1:0")`, oneshot shutdown channel, temp SurrealKV database.
3. **Three lifecycle endpoints per server:**
   - **`POST /seed`** — populate demo data (idempotent, skips if already present)
   - **`POST /reset`** — wipe all data to empty state
   - **`POST /reseed`** — wipe + re-seed in one call (most common between test runs)
5. **Two animal test accounts per backend** — each with a cartoony profile image in the same style as the existing Cat and Dog avatars (`clients/demo/assets/`).
6. **Signup flow supported** — each server allows creating new accounts so the "Add Account" UI can be tested end-to-end.
7. **No heavy external dependencies** — no real Matrix homeserver (Synapse/Dendrite), no real Discord bot gateway, no real Teams Graph API. Pure Rust mock servers.
8. **Crate location:** Separate `servers/test-{backend}/` crates per backend + shared `servers/test-common/` library (D4.0.1).
9. **Avatar images:** Reuse existing Bojack-style PNGs from `clients/demo/assets/` — served by each test server at runtime (D4.0.4).

---

## 4.0 Architecture Decisions

- [x] **4.0.1** ✅ **Separate crates** — `servers/test-matrix/`, `servers/test-stoat/`, `servers/test-discord/`, `servers/test-teams/`, `servers/test-poly/`, plus `servers/test-common/` (shared lib) and `servers/test-runner/` (orchestrator). Each backend has a fundamentally different API shape; separate crates compile in parallel and match the existing `servers/server/` + `servers/backup-server/` pattern.
- [x] **4.0.2** ✅ **In-memory state** — `DashMap` / `RwLock<HashMap>` for mock servers. No SurrealKV overhead for mocks; `/reset` is just `.clear()`. Exception: `test-poly` wraps the real `poly-server` lib and inherits SurrealKV naturally.
- [x] **4.0.3** ✅ **Shared `servers/test-common/` crate** — provides `TestServerBase` (dynamic port, oneshot shutdown, base URL), `/health` + `/reset` + `/seed` route builders, CLI arg parser (`--port`, `--seed`, `--verbose`), simple opaque token auth helpers, and `Seedable` trait.
- [x] **4.0.4** ✅ **Reuse existing avatars** — Bojack-style PNGs already in `clients/demo/assets/` (stoat, raccoon, koala, kangaroo, sheep, walrus, cockatoo, parrot, owl + cat, dog). Each test server serves them from that shared location. Axolotl PNG still needed (only SVG placeholder exists).

---

## 4.1 Shared Test Infrastructure

> Common utilities shared by all test server binaries.

- [ ] **4.1.1** Create shared helper: `TestServerBase` struct — dynamic port binding, oneshot shutdown, temp dir, base URL accessor. Extract from existing `servers/server/tests/integration.rs` pattern.
- [ ] **4.1.2** Create shared `/health` route returning `200 OK` with `{"status": "ok", "backend": "<name>"}`.
- [ ] **4.1.3** Create shared `/reset` route handler pattern: wipe all data to empty state, return `200`.
- [ ] **4.1.4** Create shared `/seed` route handler pattern: populate demo data if not already present, return `200`.
- [ ] **4.1.4b** Create shared `/reseed` route handler pattern: reset + seed in one call, return `200`.
- [ ] **4.1.5** Create shared auth helpers: simple token-based auth middleware (JWT or opaque tokens), user registration, login.
- [ ] **4.1.6** Create CLI runner: each test server binary accepts `--port <PORT>` (override dynamic), `--seed` (auto-seed on start), `--verbose` (tracing logs).

---

## 4.2 Animal Avatars

> Generate 10 cartoony animal profile images matching the existing Cat and Dog style.

**Style reference:** `clients/demo/assets/cat.png` (calico cat with pearl necklace, purple circle bg, ~740x729) and `clients/demo/assets/dog.png` (golden retriever with sunglasses and collar, blue circle bg, ~717x705).

**Requirements:** PNG with transparency, ~740x720, colored circle background, anthropomorphized animal with one fun accessory (hat, glasses, scarf, etc.), consistent art style across all 10.

### Per-Backend Animals

| Backend | Animal 1 | Animal 2 | Asset Location |
|---------|----------|----------|----------------|
| Stoat | Stoat | Raccoon | `servers/test-stoat/assets/` or `clients/stoat/assets/` |
| Discord | Koala | Kangaroo | `servers/test-discord/assets/` or `clients/discord/assets/` |
| Teams | Sheep | Walrus | `servers/test-teams/assets/` or `clients/teams/assets/` |
| Poly Server | Cockatoo | Parrot | `servers/test-poly/assets/` or `clients/server-client/assets/` |
| Matrix | Owl | Axolotl | `servers/test-matrix/assets/` or `clients/matrix/assets/` |

- [ ] **4.2.1** Generate Stoat avatar (stoat with accessory, green circle bg)
- [ ] **4.2.2** Generate Raccoon avatar (red panda with accessory, orange circle bg)
- [ ] **4.2.3** Generate Koala avatar (koala with accessory, grey circle bg)
- [ ] **4.2.4** Generate Kangaroo avatar (kangaroo with accessory, tan circle bg)
- [ ] **4.2.5** Generate Sheep avatar (sheep with accessory, light blue circle bg)
- [ ] **4.2.6** Generate Walrus avatar (walrus with accessory, teal circle bg)
- [ ] **4.2.7** Generate Cockatoo avatar (cockatoo with accessory, yellow circle bg)
- [ ] **4.2.8** Generate Parrot avatar (parrot with accessory, red circle bg)
- [ ] **4.2.9** Generate Owl avatar (owl with accessory, dark blue circle bg)
- [ ] **4.2.10** Generate Axolotl avatar (axolotl with accessory, pink circle bg)
- [ ] **4.2.11** Place all avatars in correct asset directories; verify consistent dimensions and style.

---

## 4.3 Matrix Test Server

> Mock Matrix homeserver implementing the subset of the Client-Server API that `poly-matrix` calls. No federation, no E2EE key exchange, no media server — just enough for rooms, messages, and sync.

**Crate:** `servers/test-matrix/` (binary: `poly-test-matrix`)

### API Surface to Implement

Based on what `clients/matrix/src/http.rs` and `clients/matrix/src/guest.rs` actually call:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/_matrix/client/v3/login` | POST | Password + token login |
| `/_matrix/client/v3/account/whoami` | GET | Validate token, return user_id |
| `/_matrix/client/v3/logout` | POST | Invalidate token |
| `/_matrix/client/v3/profile/{userId}` | GET | Display name + avatar |
| `/_matrix/client/v3/joined_rooms` | GET | List joined room IDs |
| `/_matrix/client/v3/sync` | GET | Long-poll sync (rooms, timeline, state, ephemeral) |
| `/_matrix/client/v3/rooms/{roomId}/messages` | GET | Paginate message history |
| `/_matrix/client/v3/rooms/{roomId}/send/{eventType}/{txnId}` | PUT | Send message |
| `/_matrix/client/v3/rooms/{roomId}/members` | GET | Room member list |
| `/_matrix/client/v3/rooms/{roomId}/state` | GET | Room state events |
| `/_matrix/client/v1/rooms/{roomId}/hierarchy` | GET | Space hierarchy |
| `/_matrix/client/v3/join/{roomIdOrAlias}` | POST | Join room |
| `/_matrix/client/v3/publicRooms` | GET | Public room directory |
| `/_matrix/client/v3/user/{userId}/account_data/{type}` | GET | m.direct, etc. |
| `/_matrix/client/v3/register` | POST | Signup (for testing new accounts) |

### Checklist

- [ ] **4.3.1** Create `servers/test-matrix/` crate with `Cargo.toml` (axum, serde, serde_json, tokio, uuid, tracing)
- [ ] **4.3.2** Implement in-memory state: users (HashMap), rooms, room state events, timeline events, tokens
- [ ] **4.3.3** Implement `POST /register` — create user, return access_token + device_id + user_id
- [ ] **4.3.4** Implement `POST /login` — m.login.password flow, return access_token + device_id + user_id
- [ ] **4.3.5** Implement `GET /account/whoami` — validate bearer token, return user_id + device_id
- [ ] **4.3.6** Implement `POST /logout` — invalidate token
- [ ] **4.3.7** Implement `GET /profile/{userId}` — return displayname + avatar_url
- [ ] **4.3.8** Implement `GET /joined_rooms` — return list of room IDs user has joined
- [ ] **4.3.9** Implement `GET /sync` — return rooms (join: timeline, state, ephemeral), support `since` + `timeout` params. For `timeout > 0`, hold connection open (simulate long-poll) and return new events.
- [ ] **4.3.10** Implement `PUT /rooms/{roomId}/send/{eventType}/{txnId}` — store event, assign event_id, return it. Broadcast to sync waiters.
- [ ] **4.3.11** Implement `GET /rooms/{roomId}/messages` — paginated message history with `from`, `dir`, `limit` params
- [ ] **4.3.12** Implement `GET /rooms/{roomId}/members` — return m.room.member state events
- [ ] **4.3.13** Implement `GET /rooms/{roomId}/state` — return all state events for room
- [ ] **4.3.14** Implement `GET /rooms/{roomId}/hierarchy` — return Space children (rooms in a Space)
- [ ] **4.3.15** Implement `POST /join/{roomIdOrAlias}` — add user to room membership
- [ ] **4.3.16** Implement `GET /publicRooms` — return seeded public rooms
- [ ] **4.3.17** Implement `GET /user/{userId}/account_data/{type}` — return m.direct mappings
- [ ] **4.3.18** Implement `/seed`, `/reset`, and `/reseed` endpoints
- [ ] **4.3.19** Seed demo data: 2 users (Owl + Axolotl), 2 Spaces (each with 3 rooms), 3 DM rooms, sample messages, m.direct account data
- [ ] **4.3.20** Serve avatar images from `/avatars/{filename}` (static file serving from assets dir)
- [ ] **4.3.21** Wire up CLI entry point with `--port`, `--seed`, `--verbose` flags
- [ ] **4.3.22** Integration test: `poly-matrix` plugin authenticates against test server
- [ ] **4.3.23** Integration test: list servers (Spaces), channels, messages
- [ ] **4.3.24** Integration test: send message, verify it appears in sync
- [ ] **4.3.25** Integration test: signup flow (register new account, login, join room)

---

## 4.4 Stoat Test Server

> Mock Stoat/Revolt API server. Stoat uses a REST API + WebSocket for real-time events.

**Crate:** `servers/test-stoat/` (binary: `poly-test-stoat`)

### API Surface to Implement

Based on what `clients/stoat/src/http.rs` and `clients/stoat/src/guest.rs` actually call. Research the exact endpoints from the Stoat client code before implementing.

- [ ] **4.4.1** Create `servers/test-stoat/` crate
- [ ] **4.4.2** Audit `clients/stoat/src/http.rs` and `clients/stoat/src/guest.rs` — list every HTTP endpoint and WebSocket event the Stoat plugin calls
- [ ] **4.4.3** Implement in-memory state: users, servers, channels, messages, tokens
- [ ] **4.4.4** Implement auth endpoints: signup, login, token validation
- [ ] **4.4.5** Implement server endpoints: list, get, create
- [ ] **4.4.6** Implement channel endpoints: list by server, get, create
- [ ] **4.4.7** Implement message endpoints: list (paginated), send, edit, delete
- [ ] **4.4.8** Implement user endpoints: get profile, list members
- [ ] **4.4.9** Implement DM endpoints: list, open, send
- [ ] **4.4.10** Implement WebSocket endpoint: authenticate, broadcast events (message_create, typing, presence)
- [ ] **4.4.11** Implement `/seed`, `/reset`, and `/reseed` endpoints
- [ ] **4.4.12** Seed demo data: 2 users (Stoat + Raccoon), 2 servers, channels, messages
- [ ] **4.4.13** Serve avatar images
- [ ] **4.4.14** Wire up CLI entry point
- [ ] **4.4.15** Integration test: full flow (auth → list servers → list channels → send message → receive via WS)
- [ ] **4.4.16** Integration test: signup flow

---

## 4.5 Discord Test Server

> Mock Discord API server. Discord uses REST + WebSocket Gateway for real-time events.

**Crate:** `servers/test-discord/` (binary: `poly-test-discord`)

### API Surface to Implement

Based on what `clients/discord/` calls. Discord uses bot-style token auth, REST for CRUD, and a WebSocket Gateway for events.

- [ ] **4.5.1** Create `servers/test-discord/` crate
- [ ] **4.5.2** Audit `clients/discord/src/` — list every REST endpoint and Gateway event the Discord plugin calls
- [ ] **4.5.3** Implement in-memory state: users, guilds, channels, messages, tokens
- [ ] **4.5.4** Implement auth: token validation (simulated bot/user token)
- [ ] **4.5.5** Implement guild endpoints: list guilds, get guild, guild members
- [ ] **4.5.6** Implement channel endpoints: list by guild, get, create
- [ ] **4.5.7** Implement message endpoints: list (paginated), send, edit, delete, reactions
- [ ] **4.5.8** Implement user endpoints: get current user (`/users/@me`), get user by ID
- [ ] **4.5.9** Implement DM endpoints: list DM channels, create DM, send message
- [ ] **4.5.10** Implement Gateway WebSocket: IDENTIFY, READY, dispatch events (MESSAGE_CREATE, TYPING_START, PRESENCE_UPDATE, GUILD_CREATE)
- [ ] **4.5.11** Implement `/seed`, `/reset`, and `/reseed` endpoints
- [ ] **4.5.12** Seed demo data: 2 users (Koala + Kangaroo), 2 guilds (with categories + channels), DM channel, messages
- [ ] **4.5.13** Serve avatar images via CDN-like path (e.g. `/cdn/avatars/{user_id}/{hash}.png`)
- [ ] **4.5.14** Wire up CLI entry point
- [ ] **4.5.15** Integration test: full flow (auth → guilds → channels → messages → Gateway events)
- [ ] **4.5.16** Integration test: signup/token registration flow

---

## 4.6 Teams Test Server

> Mock Microsoft Teams/Graph API server. Teams uses Microsoft Graph REST API + subscriptions for real-time events.

**Crate:** `servers/test-teams/` (binary: `poly-test-teams`)

### API Surface to Implement

Based on what `clients/teams/src/` calls. Teams uses OAuth2 bearer tokens and the Microsoft Graph API.

- [ ] **4.6.1** Create `servers/test-teams/` crate
- [ ] **4.6.2** Audit `clients/teams/src/` — list every Graph API endpoint the Teams plugin calls
- [ ] **4.6.3** Implement in-memory state: users, teams, channels, messages, tokens
- [ ] **4.6.4** Implement auth: mock OAuth2 token endpoint, bearer token validation
- [ ] **4.6.5** Implement teams endpoints: list joined teams, get team
- [ ] **4.6.6** Implement channel endpoints: list by team, get channel
- [ ] **4.6.7** Implement message endpoints: list (paginated), send, reply
- [ ] **4.6.8** Implement user endpoints: `/me`, get user profile, profile photo
- [ ] **4.6.9** Implement chat endpoints: list chats (1:1 and group), send message
- [ ] **4.6.10** Implement presence endpoint: get/set presence
- [ ] **4.6.11** Implement change notifications: mock subscription endpoint + WebSocket/webhook for real-time events
- [ ] **4.6.12** Implement `/seed`, `/reset`, and `/reseed` endpoints
- [ ] **4.6.13** Seed demo data: 2 users (Sheep + Walrus), 2 teams (with channels), chat threads, messages
- [ ] **4.6.14** Serve avatar images
- [ ] **4.6.15** Wire up CLI entry point
- [ ] **4.6.16** Integration test: full flow (auth → teams → channels → messages → events)
- [ ] **4.6.17** Integration test: signup/token flow

---

## 4.7 Poly Server Test Instance

> The Poly Server already exists at `servers/server/`. Unlike the other 4 mock servers (which are in-memory fakes of external APIs), `test-poly` wraps the **real** `poly-server` as a library dependency — so improvements to test infrastructure flow back into the production server. This is the only backend where we own both sides.

**Crate:** `servers/test-poly/` (binary: `poly-test-poly`) — thin wrapper around `poly-server` lib

- [ ] **4.7.1** Create `servers/test-poly/` crate that depends on `poly-server` (as a library)
- [ ] **4.7.2** Add `/reset` route: drop all SurrealKV data (empty state)
- [ ] **4.7.3** Add `/seed` route: create demo data if not present
- [ ] **4.7.3b** Add `/reseed` route: reset + seed in one call
- [ ] **4.7.4** Seed demo data: 2 accounts (Cockatoo + Parrot), 2 servers, channels with categories, messages, friend relationship between the two accounts, DM conversation
- [ ] **4.7.5** Serve avatar images for Cockatoo + Parrot
- [ ] **4.7.6** Wire up CLI entry point (same flags as other test servers)
- [ ] **4.7.7** Integration test: verify `/reset` clears all data and re-seeds correctly
- [ ] **4.7.8** Integration test: verify signup flow creates a clean new account
- [ ] **4.7.9** Integration test: full CRUD (servers, channels, messages, DMs, friends)

---

## 4.8 Demo Data Specification

> Consistent demo data structure across all test servers for predictable UI testing.

### Per-Server Seed Data

Each test server seeds the following on `/seed` or startup with `--seed`:

| Data | Count | Details |
|------|-------|---------|
| User accounts | 2 | Animal 1 + Animal 2, with display names, avatar URLs, passwords `"testpass123"` |
| Servers/Spaces/Guilds/Teams | 2 | "Animal Hangout" (general chat) + "Project Burrow" (work-themed) |
| Categories per server | 2 | "General" + "Off-Topic" |
| Channels per category | 2-3 | `#general`, `#random`, `#announcements` in General; `#memes`, `#music` in Off-Topic |
| Messages per channel | 10-20 | Mix of text, replies, reactions; timestamps spread over last 7 days |
| DM conversation | 1 | Between Animal 1 and Animal 2, 5-10 messages |
| Friend relationship | 1 | Animal 1 ↔ Animal 2 are mutual friends (where backend supports friends) |

### Test Account Credentials

| Backend | Animal 1 | Username/Email | Animal 2 | Username/Email |
|---------|----------|----------------|----------|----------------|
| Matrix | Owl | `@owl:localhost` | Axolotl | `@axolotl:localhost` |
| Stoat | Stoat | `stoat` | Raccoon | `raccoon` |
| Discord | Koala | `koala` | Kangaroo | `kangaroo` |
| Teams | Sheep | `sheep@test.local` | Walrus | `walrus@test.local` |
| Poly | Cockatoo | `cockatoo` | Parrot | `parrot` |

**Password for all test accounts:** `testpass123`

### Server/Space Names

| Backend | Server 1 | Server 2 |
|---------|----------|----------|
| Matrix | "The Hollow Tree" (Space) | "Neon Reef" (Space) |
| Stoat | "The Burrow" | "Crimson Den" |
| Discord | "Eucalyptus Lounge" | "Outback Hub" |
| Teams | "Woolly Workshop" | "Arctic Office" |
| Poly | "Feather Nest" | "Tropical Canopy" |

---

## 4.9 Test Runner & Orchestration

> Run all test servers simultaneously for full multi-backend UI testing.

- [ ] **4.9.1** Create `servers/test-runner/` binary that spawns all 5 test servers on sequential ports (e.g., 9100-9104)
- [ ] **4.9.2** Print a summary table on startup: backend, port, status, Animal 1, Animal 2
- [ ] **4.9.3** `/reseed-all` endpoint on the runner: calls `/reseed` on each test server
- [ ] **4.9.4** Graceful shutdown: Ctrl+C stops all servers cleanly
- [ ] **4.9.5** Integration test: start runner, verify all 5 backends respond to `/health`
- [ ] **4.9.6** Document usage in `docs/testing.md`: how to start servers, connect Poly, reset between tests

---

## 4.10 Poly App Integration

> Make the Poly app aware of test server URLs so developers can quickly connect.

- [ ] **4.10.1** Add a dev-only "Test Servers" section in Settings or Add Account — pre-fills backend URL with `localhost:PORT`
- [ ] **4.10.2** For Matrix: support custom homeserver URL in the Add Account flow (not just `matrix.org`)
- [ ] **4.10.3** For Stoat: support custom server URL in the Add Account flow
- [ ] **4.10.4** For Discord: support custom API base URL override (dev/test mode)
- [ ] **4.10.5** For Teams: support custom Graph API base URL override (dev/test mode)
- [ ] **4.10.6** For Poly Server: already supports custom URL — verify it works with test instance

---

## 4.11 E2E Smoke Tests (Automated)

> Automated tests that exercise each backend through the actual Poly plugin stack.

- [ ] **4.11.1** Matrix E2E: start test-matrix → authenticate poly-matrix plugin → list servers → list channels → send message → verify in sync → logout
- [ ] **4.11.2** Stoat E2E: start test-stoat → authenticate poly-stoat plugin → list servers → send message → verify via WS → logout
- [ ] **4.11.3** Discord E2E: start test-discord → authenticate poly-discord plugin → list guilds → send message → verify via Gateway → logout
- [ ] **4.11.4** Teams E2E: start test-teams → authenticate poly-teams plugin → list teams → send message → verify → logout
- [ ] **4.11.5** Poly Server E2E: start test-poly → authenticate poly-server-client → list servers → send message → verify via WS → logout
- [ ] **4.11.6** Cross-backend E2E: start all 5 servers → authenticate all 5 backends → verify sidebar shows servers from all backends → send messages across backends

---

## Completion Criteria

- [ ] All 5 test server binaries build and run independently
- [ ] `poly-test-runner` starts all 5 servers with one command
- [ ] Each server has `/health`, `/seed`, `/reset`, `/reseed` endpoints
- [ ] Each server supports signup (new account creation)
- [ ] 10 animal avatar images generated in consistent cartoony style
- [ ] Demo data seeded: 2 users, 2 servers, channels, messages, DMs per backend
- [ ] All 5 E2E smoke tests pass
- [ ] Poly app can connect to all 5 test servers via localhost
- [ ] `/reseed` on each server reliably clears and re-seeds data
- [ ] Documentation in `docs/testing.md` covers setup and usage

---

## Appendix: Existing Infrastructure to Reuse

| Component | Location | Reuse For |
|-----------|----------|-----------|
| `TestServer` pattern | `servers/server/tests/integration.rs` | Dynamic port binding, shutdown, temp DB |
| `AppState` + router | `servers/server/src/lib.rs` | Poly test server (wrap directly) |
| Demo data generators | `clients/demo/src/data.rs` | Message/user/server templates |
| Cat/Dog avatar style | `clients/demo/assets/` | Art direction reference for new avatars |
| WASM plugin test harness | `crates/plugin-host-tests/` | E2E plugin tests against mock servers |
| `PolyServerHttpClient` | `clients/server-client/` | Poly Server integration tests |
