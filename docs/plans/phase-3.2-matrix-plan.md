# Phase 3.2 Plan — Matrix Client

> **Created:** 2026-03-30
> **Updated:** 2026-04-03
> **Status:** 🟡 In Progress — Core HTTP client + ClientBackend impl complete
> **Crate:** `poly-matrix`
> **Goal:** Chat with Matrix homeservers. Spaces = servers, rooms = channels, E2EE supported.
> **Implementation**: Custom Matrix HTTP client (same pattern as Stoat). No matrix-sdk anywhere.
> **Depends on:** Phase 3.1 (WebRTC infrastructure built there is reused here)

---

## !! CRITICAL — WASM Plugin Architecture Decision !!

### Why matrix-sdk Cannot Go in the Plugin

`poly-matrix` must build as a **WASM Component Model plugin** (`wasm32-wasip2`), per DECISION D21.
`matrix-sdk` cannot be compiled into this target for two hard reasons:

1. **`reqwest`** (its HTTP client) has no WASI P2 backend — only browser WASM (`wasm32-unknown-unknown`) and native. No working wasip2 path exists in 0.12+.
2. **`tokio`** has no wasip2 support outside experimental/WasmEdge forks incompatible with Component Model.
3. `matrix-sdk`'s `ClientBuilder::http_client()` only accepts a concrete `reqwest::Client` — no pluggable HTTP trait exists. Forking to add abstraction is a major maintenance burden.

**If matrix-sdk ran on the host side instead** (the "host-proxy" pattern), updating the Matrix client would require shipping a new host binary — not a new `.wasm` file. That defeats plugin updatability entirely.

### Chosen Strategy: Custom Matrix Client in the WASM Guest

The same approach as `clients/stoat/src/guest.rs`: implement the Matrix client-server API **directly in `guest.rs`** using host-mediated HTTP (`host_api::http_request()` WIT import). No matrix-sdk in the WASM artifact. Plugin ships as a self-contained `.wasm` file and can be updated independently.

```
┌────────────────────────────────────────────────────────────┐
│  poly-matrix.wasm  (wasm32-wasip2, self-contained)         │
│                                                            │
│  guest.rs: custom Matrix HTTP client                       │
│    ├── auth.rs    password + SSO token exchange            │
│    ├── sync.rs    /sync long-poll, timeline parsing        │
│    ├── rooms.rs   Space/channel/DM listing                 │
│    ├── messages.rs send/receive/paginate                   │
│    ├── spaces.rs  Space → server mapping, fake servers     │
│    └── e2ee.rs    vodozemac (pure Rust, wasip2-compatible) │
│                                                            │
│    ← imports host_api::http_request()  (WIT)               │
│    ← imports host_api::kv_get/set()    (WIT, sessions/E2EE)│
└───────────────────────┬────────────────────────────────────┘
                        │ WIT imports (HTTP + KV only)
┌───────────────────────▼────────────────────────────────────┐
│  Plugin host  (native Rust, tokio + reqwest)               │
│  Generic capabilities: HTTP proxy, KV store (SQLite/Surreal)│
│  Knows nothing about Matrix — fully generic                │
└────────────────────────────────────────────────────────────┘
```

**The host owns zero Matrix logic.** It only proxies HTTP calls and provides KV storage. All Matrix protocol knowledge lives in the updatable `.wasm` plugin.

### E2EE in the WASM Plugin

`vodozemac` (pure-Rust Olm/Megolm, no networking, no async runtime) should compile to `wasm32-wasip2` without changes — it is purely arithmetic and memory. **Requires an explicit `cargo component build` verification step before implementation begins.** If vodozemac fails on wasip2, E2EE is gated to native builds only until upstream fixes it.

### No matrix-sdk — Anywhere

`matrix-sdk` is not used at all, neither in the WASM plugin nor in the native build. Remove the commented-out dependency from `Cargo.toml`. The native `MatrixClient` in `src/lib.rs` will also implement the Matrix HTTP API directly (same logic as the guest, sharing modules where possible). Rationale: matrix-sdk is a dependency we can't control, can't compile to wasip2, and adds significant bloat. We only need the parts of the Matrix protocol Poly actually uses — implementing those directly keeps us in full control.

---

## 3.2.0 Architecture Decisions (Pre-Implementation)

- [x] **3.2.0.1** Confirm matrix-sdk cannot compile to wasm32-wasip2 — **CONFIRMED** (reqwest + tokio blockers)
- [x] **3.2.0.2** Confirm host-proxy pattern breaks plugin updatability — **CONFIRMED**, rejected
- [x] **3.2.0.3** Chosen strategy: custom Matrix client in guest (same as Stoat) — **DECIDED**
- [ ] **3.2.0.4** Verify vodozemac compiles to wasm32-wasip2 (`cargo component build` smoke test)
- [x] **3.2.0.5** Confirm `host_api::http_request()` is sufficient for Matrix — **CONFIRMED**. `/sync` is plain HTTP long-poll, not WebSocket. The host blocks the guest for up to `timeout` ms then returns the full JSON response. No streaming needed.
- [x] **3.2.0.6** No matrix-sdk anywhere (native or WASM) — custom Matrix HTTP client throughout — **DECIDED**
- [x] **3.2.0.7** Sync strategy: classic `/sync` long-poll — **DECIDED**. Universally supported by all homeservers. Sliding Sync deferred to later optimization.
- [ ] **3.2.0.8** Decide session storage: serialize session tokens to `host_api::kv_set()` vs a dedicated WIT import

---

## 3.2.1 Research & Planning

- [x] **3.2.1.1** matrix-sdk WASM/WASI compatibility — **DONE** (see section above)
- [x] **3.2.1.2** Document Spaces → server mapping strategy — **DONE** (see 3.2.2)
- [ ] **3.2.1.3** Document Matrix client-server HTTP API endpoints needed (auth, sync, send, rooms, members, profile)
- [ ] **3.2.1.4** Document SSO/OIDC login flow in desktop context (localhost callback, token exchange)
- [ ] **3.2.1.5** Document E2EE: vodozemac API surface for Olm/Megolm session management
- [ ] **3.2.1.6** Document VoIP signaling: `m.call.*` events, group calls via MSC3401
- [ ] **3.2.1.7** Research public homeserver discovery: matrix.org default + `GET /_matrix/client/v3/publicRooms`
- [ ] **3.2.1.8** Study Fractal / Stoat guest.rs for HTTP client patterns applicable to Matrix
- [ ] **3.2.1.9** Update `clients/matrix/agents.md` with all findings

**Reference**: `matrixdocs.github.io` — community developer docs (protocol, clients, bridges, SDKs)

---

## 3.2.2 Spaces & "Fake Servers" — The Core Mapping

This is the most design-sensitive part of the Matrix integration, explicitly called out in the original product plan:

> *"For Matrix, same if it does have servers, otherwise we just want to add matrix channels to our favourites under one or more self-created matrix-server categories so we can emulate a discord server. If matrix does have servers now use those."*

### What Matrix Actually Has

| Concept | Exists? | Notes |
|---------|---------|-------|
| Spaces | ✓ Yes | Hierarchical room groupings, acts like Discord servers |
| Sub-spaces | ✓ Yes | Nested Spaces = categories |
| Rooms | ✓ Yes | Text channels; voice rooms via Jitsi/Element Call |
| DMs | ✓ Yes | `m.direct` account data tag |
| Multi-user rooms | ✓ Yes | Rooms with 3+ members, no Space parent |
| Federation | ✓ Yes | Any homeserver via `#room:server.tld` aliases |

### The Problem with Real Matrix Clients

In Element, Fractal, etc., rooms NOT in any Space are shown as a flat, unsorted list. Most Matrix users have a mix of Spaces and orphan rooms (joined from links, bridges, etc.), which is disorienting to Discord users. Poly must solve this.

### Poly's Three-Tier Display Model

```
Sidebar server list
├── [🌐] matrix.org Space        ← real Space, fetched from homeserver
│   ├── #general:matrix.org
│   └── #offtopic:matrix.org
├── [🌐] GNOME Space              ← real Space
│   └── #gnome-desktop:gnome.org
├── [⭐] My Favourites            ← FAKE SERVER (user-created, local storage)
│   ├── #rust:mozilla.org         ← orphan rooms dragged here
│   └── #dioxus:server.tld
└── [💬] Direct Messages          ← pseudo-server for all DMs (auto-generated)
    ├── @alice:matrix.org
    └── @bob:example.com
```

### Fake Servers — Detailed Design

**Definition**: A "fake server" is a user-created named grouping of Matrix rooms that have no common Space.

- Stored **locally** via `crates/core` storage layer (SQLite by default, `storage-sqlite` feature) — never sent to the homeserver
- Displayed identically to real Spaces in the Poly sidebar
- Marked with a configurable icon (default ⭐) vs. real Spaces (🌐 or Space avatar)
- Supports user-named sub-categories
- Supports drag-and-drop for adding/reordering rooms

**Schema** (conceptual — stored via `crates/core` storage layer, SQLite by default):
```sql
-- fake_server table
CREATE TABLE fake_server (id, account_id, name, icon, position);
CREATE TABLE fake_server_room (fake_server_id, room_id, category, position);
```

**UX flow:**
1. User sees orphan rooms in a collapsible "Unsorted Rooms" catch-all
2. Right-click a room → "Add to..." → creates new fake server or selects existing
3. Fake server appears in sidebar, room moves into it
4. Rename/reorder/delete fake servers from settings or right-click menu

**What rooms qualify**: Any joined room with no parent Space in the user's account data (or whose parent Space the user explicitly hid).

---

## 3.2.3 Core Matrix Client (WASM Guest)

> Implement Matrix client-server API directly in `guest.rs` modules. Use `host_api::http_request()` for all HTTP. No matrix-sdk.

- [x] **3.2.3.1** Scaffold module files — **DONE** (api.rs, config.rs, http.rs, guest.rs with real implementations)
- [x] **3.2.3.2** Username/password login: `POST /_matrix/client/v3/login` — **DONE** (native http.rs + guest.rs)
- [ ] **3.2.3.3** SSO login: open browser to homeserver SSO URL, capture `loginToken` via localhost callback, exchange for session
- [ ] **3.2.3.4** Session persistence: serialize `access_token` + `device_id` to `host_api::kv_set()`
- [x] **3.2.3.5** Fetch joined rooms: `GET /_matrix/client/v3/joined_rooms` — **DONE**
- [x] **3.2.3.6** Fetch Space hierarchy: `GET /_matrix/client/v1/rooms/{roomId}/hierarchy` — **DONE**
- [x] **3.2.3.7** Map Spaces → Poly `Server` (name, avatar) — **DONE** (get_servers, get_server)
- [x] **3.2.3.8** Map Space children (rooms as channels) — **DONE** (get_channels via hierarchy)
- [ ] **3.2.3.9** Detect orphan rooms (joined, no parent Space) — return as unassigned
- [ ] **3.2.3.10** Fake server CRUD via `crates/core` storage layer (`fake_server`, `fake_server_room` tables — SQLite by default)
- [x] **3.2.3.11** DM rooms → `DmChannel` (via `m.direct` account data) — **DONE** (get_dm_channels)
- [ ] **3.2.3.12** Multi-user rooms → `Group`
- [x] **3.2.3.13** Send message: `PUT /_matrix/client/v3/rooms/{roomId}/send/m.room.message/{txnId}` — **DONE** (send_message + send_reply_message)
- [x] **3.2.3.14** Paginate history: `GET /_matrix/client/v3/rooms/{roomId}/messages` — **DONE** (get_messages with sync-based pagination)
- [x] **3.2.3.15** User profile + avatar: `GET /_matrix/client/v3/profile/{userId}` — **DONE** (get_user, fetch_profile)
- [x] **3.2.3.16** Room membership list: `GET /_matrix/client/v3/rooms/{roomId}/members` — **DONE** (get_channel_members)
- [x] **3.2.3.17** Federation: join room by alias `POST /_matrix/client/v3/join/{roomAliasOrId}` — **DONE** (http.rs join_room)
- [x] **3.2.3.18** Implement `ClientBackend` trait — **DONE** (full native impl in lib.rs, guest.rs auth in WASM)
- [ ] **3.2.3.19** MXC URL → HTTP URL conversion helper (`mxc://server/media_id` → `https://server/_matrix/media/v3/download/server/media_id`)
- [ ] **3.2.3.20** Custom homeserver URL support — currently hardcoded to `DEFAULT_HOMESERVER` in guest.rs; accept from auth credentials or settings

### 3.2.3.W WASM Guest — Remaining Methods

> `guest.rs` currently only implements auth (`authenticate`, `logout`, `is_authenticated`, `get_user`). All other `Guest` trait methods return empty stubs. Port the native `lib.rs` logic to guest.rs using `host_api::http_request()`.

- [ ] **3.2.3.W1** `get_servers()` — fetch joined rooms, identify Spaces via room state, return as `Server` list
- [ ] **3.2.3.W2** `get_server(id)` — fetch single Space metadata
- [ ] **3.2.3.W3** `get_channels(server_id)` — Space hierarchy → channel list
- [ ] **3.2.3.W4** `get_channel(id)` — fetch single room metadata
- [ ] **3.2.3.W5** `send_message(channel_id, content)` — PUT send event with txn_id
- [ ] **3.2.3.W6** `send_reply_message(channel_id, reply_to, content)` — send with `m.relates_to`
- [ ] **3.2.3.W7** `get_messages(channel_id, query)` — paginated message history via `/messages`
- [ ] **3.2.3.W8** `get_channel_members(channel_id)` — room members endpoint
- [ ] **3.2.3.W9** `get_dm_channels()` — `m.direct` account data → DmChannel list
- [ ] **3.2.3.W10** `open_direct_message_channel(user_id)` — create/find DM room
- [ ] **3.2.3.W11** `handle_ws_data()` — not used by Matrix (HTTP long-poll, not WS). Sync loop runs via `http-request` and calls `emit-event` for each parsed timeline event. Guest manages `since` token in thread-local state.
- [ ] **3.2.3.W12** Transaction ID generation in WASM guest (uuid v4 or counter-based)

---

## 3.2.4 Real-Time Sync

> **Native:** `event_stream()` returns `Pin<Box<dyn Stream<Item = ClientEvent> + Send>>` — spawn a tokio task running the `/sync` loop, yield events via channel.
> **WASM Guest:** Guest initiates `/sync` loop via `http-request` host import. On each sync response, guest parses timeline events and calls `emit-event` for each one. Guest manages `since` token in thread-local state. The host triggers sync by calling `handle-ws-data` (unused for Matrix) or the guest self-drives via a startup hook.

- [x] **3.2.4.1** Sync endpoint: `GET /_matrix/client/v3/sync?timeout=&since=` — **DONE** (http.rs sync method, used by get_messages for pagination)
- [ ] **3.2.4.2** Native `event_stream()` impl: tokio task with `/sync` loop, `since` token tracking, yield `ClientEvent` via `futures::channel::mpsc`
- [ ] **3.2.4.3** Parse `m.room.message` timeline events → `ClientEvent::MessageReceived`
- [ ] **3.2.4.4** Parse `m.room.message` with `m.replace` relation → `ClientEvent::MessageEdited`
- [ ] **3.2.4.5** Parse `m.room.redaction` → `ClientEvent::MessageDeleted`
- [ ] **3.2.4.6** Handle join/leave/invite room state from sync → `ClientEvent::ChannelUpdated`, `ClientEvent::ServerUpdated`
- [ ] **3.2.4.7** Typing indicators: `m.typing` ephemeral events → `ClientEvent::TypingStarted`
- [ ] **3.2.4.8** Read receipts: `m.read` in ephemeral events
- [ ] **3.2.4.9** Presence updates → `ClientEvent::PresenceChanged` (if homeserver supports it — many disable it)
- [ ] **3.2.4.10** Push notification rules: parse `m.push_rules` for mention/DM highlight → `ClientEvent::NotificationReceived`
- [ ] **3.2.4.11** Connection state management: emit `ClientEvent::ConnectionStateChanged` on sync errors/reconnects
- [ ] **3.2.4.12** (Optional) Sliding Sync: faster initial load, opt-in per homeserver — implement after classic sync works

---

## 3.2.5 E2EE (vodozemac in WASM Guest)

> `vodozemac` is pure-Rust crypto with no tokio/reqwest dependency. Targets: compile directly into the WASM plugin.

- [ ] **3.2.5.1** Verify `vodozemac` compiles to `wasm32-wasip2` — `cargo component build` smoke test. **Gate all E2EE work on this.**
- [ ] **3.2.5.2** Add `vodozemac` to `poly-matrix` WASM dependencies (not behind `native` feature)
- [ ] **3.2.5.3** Implement Olm session creation and key exchange for 1:1 encrypted rooms
- [ ] **3.2.5.4** Implement Megolm session management for group encrypted rooms
- [ ] **3.2.5.5** Encrypt outgoing messages: create/reuse Megolm session, wrap in `m.room.encrypted`
- [ ] **3.2.5.6** Decrypt incoming `m.room.encrypted` events using stored session keys
- [ ] **3.2.5.7** Key storage: persist Olm/Megolm session state via `host_api::kv_set()` (encrypted with device key)
- [ ] **3.2.5.8** Device verification: QR code + emoji SAS flows (using `m.key.verification.*` events)
- [ ] **3.2.5.9** Cross-signing: bootstrap and verify via MSK/SSK/USK
- [ ] **3.2.5.10** Key backup / SSSS (4S): upload encrypted key backup to homeserver
- [ ] **3.2.5.11** Expose verification state in `ClientEvent` (verified / unverified device badges in UI)

---

## 3.2.5B Mock Test Server & Manual UI Testing

> See Phase 4 plan (`docs/phase-4-test-servers-plan.md` §4.3) for full details. This section tracks Matrix-specific test server integration.

**Test accounts:** Owl + Axolotl (cartoony avatar PNGs matching Cat/Dog style)
**Crate:** `servers/test-matrix/` (binary: `poly-test-matrix`)

- [ ] **3.2.5B.1** Build mock Matrix homeserver implementing the 15 CS API endpoints the plugin calls (see §4.3 checklist)
- [ ] **3.2.5B.2** Mock `/sync` long-poll — return timeline events, state, ephemeral; support `since` + `timeout`
- [ ] **3.2.5B.3** `/reset` and `/seed` endpoints with demo data (2 users, 2 Spaces with rooms, DM rooms, m.direct account data, messages)
- [ ] **3.2.5B.4** `POST /register` for signup flow testing
- [ ] **3.2.5B.5** Integration test: `poly-matrix` plugin authenticates → list Spaces → list channels → send message → verify in sync → logout
- [ ] **3.2.5B.6** Manual UI test: connect Poly app to `localhost` test server, verify sidebar/chat/DMs render correctly

---

## 3.2.6 WASM Plugin Verification

- [ ] **3.2.6.1** `cargo component build -p poly-matrix --target wasm32-wasip2` — confirm clean build with no unwanted deps
- [ ] **3.2.6.2** Run all 10 existing stub tests — confirm they still pass through plugin host
- [ ] **3.2.6.3** Integration test: login to a real Matrix homeserver (matrix.org test account) via plugin
- [ ] **3.2.6.4** Integration test: send and receive a message end-to-end

---

## 3.2.7 Voice/Video (Matrix VoIP)

- [ ] **3.2.7.1** Matrix VoIP signaling: emit/handle `m.call.invite`, `m.call.answer`, `m.call.candidates`, `m.call.hangup`
- [ ] **3.2.7.2** Integrate with shared WebRTC infrastructure from Phase 3.1
- [ ] **3.2.7.3** 1:1 voice calls
- [ ] **3.2.7.4** 1:1 video calls
- [ ] **3.2.7.5** Group calls — MSC3401 / Element Call (defer if matrix-sdk doesn't expose it cleanly by then)

---

## 3.2.8 Public Server Directory

- [ ] **3.2.8.1** Show `matrix.org` as default homeserver in "Add Account" flow
- [ ] **3.2.8.2** Curated list of public homeservers in app (hardcoded + user-addable)
- [ ] **3.2.8.3** Room directory: `GET /_matrix/client/v3/publicRooms` per homeserver
- [ ] **3.2.8.4** "Join by alias" flow: user pastes `#room:server.tld` to join any federated room

---

## 3.2.9 Multi-Account & Poly UI Integration

> From the original product plan — Matrix accounts must look identical to Discord/Stoat accounts in the Poly sidebar.

- [ ] **3.2.9.1** Support multiple Matrix accounts simultaneously (one plugin instance per `@user:homeserver`)
- [ ] **3.2.9.2** Each Matrix server (Space or fake server) in the sidebar shows: account avatar (micro-icon top-left), Matrix network logo, server icon
- [ ] **3.2.9.3** Channel list banner shows homeserver name and Matrix logo when a Matrix server is active
- [ ] **3.2.9.4** "Add to Favourites" from account settings → brings Space/room into main sidebar
- [ ] **3.2.9.5** Notifications page aggregates Matrix DMs and @mentions across all Matrix accounts

---

## Tech Notes

### HTTP Client Pattern (follow Stoat)
The guest calls `host_api::http_request(method, url, headers, body)` for all Matrix HTTP. This is synchronous from the guest's perspective. The host executes it via tokio+reqwest and returns the response. See `clients/stoat/src/guest.rs` for the established pattern.

### `/sync` Long-Poll via http_request
Matrix `/sync` with `timeout=30000` keeps the connection open for up to 30s. Whether `host_api::http_request()` supports response streaming or just waits for completion needs confirming in 3.2.0.5. If it blocks for 30s per call, that's fine — the sync loop just calls it in a tight loop. If it times out before 30s, use `timeout=0` (immediate) instead and poll more frequently.

### SSO Desktop Flow
1. Fetch SSO URL: `GET /_matrix/client/v3/login/sso/redirect?redirectUrl=http://localhost:PORT`
2. Open in system browser (or Wry/Electron webview)
3. Host spins up temp HTTP listener on `PORT`
4. Homeserver redirects to `http://localhost:PORT?loginToken=TOKEN`
5. Exchange: `POST /_matrix/client/v3/login` with `type: m.login.token`
6. Store `access_token` + `device_id` via `kv_set()`

### Sliding Sync vs Classic Sync
- **Classic `/sync`**: Universally supported. Use as default.
- **Sliding Sync**: Faster initial load, requires proxy or native homeserver support. Implement as opt-in enhancement after classic works.

### Reference
- Fractal (GNOME, Rust + matrix-sdk) — protocol usage patterns
- Element Aurora (experimental WASM prototype) — shows the WASM limits we're working around
- matrixdocs.github.io — community protocol reference

---

## Completion Criteria

- [ ] Can log into `matrix.org` and other homeservers (SSO/OIDC + password)
- [ ] Spaces display as servers with categories in sidebar
- [ ] Rooms display as channels with correct membership and topic
- [ ] Orphan rooms (not in any Space) visible and assignable to fake servers
- [ ] Fake servers: user can create, name, populate, reorder, delete
- [ ] E2EE works for 1:1 and group encrypted rooms (if vodozemac wasip2 verified)
- [ ] Voice and video calls work (using shared WebRTC from 3.1)
- [ ] Federation: can join rooms on any homeserver via alias
- [ ] Multiple Matrix accounts supported simultaneously
- [ ] Each Matrix server shows account/network source icons in the sidebar
- [ ] Notifications aggregated across all Matrix accounts
- [ ] WASM plugin builds standalone: `cargo component build -p poly-matrix --target wasm32-wasip2`
- [ ] No matrix-sdk anywhere in the crate (verify Cargo.toml has no matrix-sdk dep at all)
- [ ] All 10 existing stub tests pass
