# Work Plan — Phase 3 clients + Phase 5 CLI

> Generated: 2026-04-05. Tracks all open work from this session.

---

## A — UI Fixes ✅

- [x] **A1** Remove `← Choose Backend` back button from stoat, lemmy, hackernews signup forms
- [x] **A2** Test Accounts page: backend label badge on each card
- [x] **A3** Test Accounts page: Matrix/HN/Lemmy/Discord/Teams entries (disabled when not compiled)
- [x] **A4** Plugins page: `demo_forum` now visible as "Demo (Forum)"
- [x] **A5** Plugins page: HN and Lemmy entries (now real, `available: cfg!(feature)`)
- [x] **A6** Not a real bug — `demo_forum` sessions have slug `demo_forum` and display under their own row; `slug()` returns the raw string so no cross-contamination

---

## B — Lemmy Client (workspcacemsg2)

Reference plan: `docs/plan-lemmy-client.md`

- [ ] **B1** Create `clients/lemmy/` crate with `Cargo.toml` mirroring `clients/stoat/`
- [ ] **B2** Implement `LemmyClient` struct implementing `ClientBackend` trait
  - `authenticate(EmailPassword)` → JWT token via `POST /api/v3/user/login`
  - `get_servers()` → subscribed communities → Vec<Server>
  - `get_channels(server_id)` → single implicit "Posts" channel per community (ChannelType::Forum)
  - `get_messages(channel_id)` → community posts → Vec<Message> (forum posts)
  - `get_messages(post_id)` → post comments → Vec<Message> (threaded replies)
  - `list_dms()` → private messages from `/api/v3/private_message/list`
  - `list_friends()` → empty / stub (Lemmy has no friends concept)
  - `get_user(id)` → `/api/v3/user?person_id=`
- [ ] **B3** `clients/lemmy/src/signup.rs` — login form (instance URL + username + password)
- [ ] **B4** Register signup entry in `clients/lemmy/src/lib.rs`
- [ ] **B5** Add lemmy feature flag to `crates/core/Cargo.toml` + `apps/web/Cargo.toml`
- [ ] **B6** Compile and check: `cargo check -p poly-lemmy`

Test server:
- [ ] **B7** Create `servers/test-lemmy/` — Axum stub implementing Lemmy REST API v3  
  Endpoints needed: `/api/v3/user/login`, `/api/v3/site`, `/api/v3/community/list`,  
  `/api/v3/post/list`, `/api/v3/comment/list`, `/api/v3/private_message/list`  
  In-memory state, no real federation. Pattern: copy `servers/test-stoat/` structure.

---

## C — Hacker News Client (workspcacemsg3)

Reference plan: `docs/plan-hackernews-client.md`

- [ ] **C1** Create `clients/hackernews/` crate with `Cargo.toml`
- [ ] **C2** Implement `HackerNewsClient` struct implementing `ClientBackend`
  - No auth required (read-only; optional auth for voting/commenting)
  - `get_servers()` → single virtual server "Hacker News"
  - `get_channels(server_id)` → 6 fixed channels: Top, New, Best, Ask HN, Show HN, Jobs (ChannelType::Forum)
  - `get_messages(channel_id)` → fetch story IDs + resolve top 20 stories → Vec<Message>
    - Each story = a Message with score as `🔥` reaction count, `💬` for descendants
  - `get_messages(story_id)` → recursively resolve `kids` comments → threaded Vec<Message>
  - `list_dms()` → empty (HN has no DMs)
  - `list_friends()` → empty
  - `authenticate(Guest {})` → success (no-op, guest mode)
- [ ] **C3** `clients/hackernews/src/signup.rs` — one-click "Add Hacker News" (no credentials)
- [ ] **C4** Register signup entry
- [ ] **C5** Add `hackernews` feature flag to workspace Cargo.toml + apps
- [ ] **C6** `cargo check -p poly-hackernews`

Test server:
- [ ] **C7** Create `servers/test-hackernews/` — Axum stub of Firebase HN API  
  Endpoints: `/v0/topstories.json`, `/v0/newstories.json`, `/v0/beststories.json`,  
  `/v0/askstories.json`, `/v0/showstories.json`, `/v0/jobstories.json`,  
  `/v0/item/{id}.json`  
  Serves hardcoded mock stories/comments. No live fetching.

---

## D — Phase 5 CLI Pipeline (main agent)

Reference: `docs/phase-5.1-plan.md`, existing: `mcp/chat-mcp/src/`

### D1 — Audit existing MCP tools ✅
- [x] `login` works for stoat test server (verified end-to-end)
- [x] `poly-cli` dynamic dispatch works: health → login → list_servers → list_channels → list_dms → get_messages

### D2 — Easy-signin bypass (test server only) ✅
- [x] `test_signin` MCP tool in `mcp/chat-mcp/src/tools.rs`
  - Input: `{backend, url, username}` — no password
  - Guard: only allowed if `url` contains `localhost` or `127.0.0.1`
  - Calls `/test/auth/token` → logs in with returned token
- [x] `/test/auth/token` endpoint on ALL test servers (stoat, matrix, lemmy, discord, teams)

### D3 — Full stoat + lemmy CLI login flow ✅
- [x] `poly-cli call login --backend stoat --url http://localhost:9101 --username stoat --password testpass123`
- [x] `poly-cli call list_servers --backend stoat` → 2 servers
- [x] `poly-cli call list_channels --backend stoat --server_id SRV001` → 3 channels
- [x] `poly-cli call list_dms --backend stoat` → 1 DM
- [x] `poly-cli call list_friends --backend stoat` → []
- [x] `poly-cli call get_messages --backend stoat --channel_id CH001 --limit 3` → messages
- [x] `poly-cli call test_signin --backend stoat --url http://localhost:9101 --username raccoon` (no password)
- [x] Lemmy: login → list_servers → list_channels + test_signin verified
- [ ] MCP → poly-web UI SSE broadcast (future phase)

### D4 — Test account easy-signin in UI ✅
- [x] `test_account_authenticate()` dispatch in signup/mod.rs
- [x] stoat, lemmy, hackernews all get working "Add Account" buttons

### D5 — Extend to other backends ✅
- [x] matrix test server: `/test/auth/token` (normalizes @user:localhost)
- [x] discord test server: `/test/auth/token`
- [x] teams test server: `/test/auth/token`
- [x] HN: no login needed (guest session)
- [x] lemmy: `/test/auth/token` on test server

---

## E — Plugin Registrations (after B + C land)

- [ ] **E1** Register `LemmyClient` signup entry in app init (`apps/web/src/main.rs` or init chain)
- [ ] **E2** Register `HackerNewsClient` signup entry
- [ ] **E3** Update `NATIVE_BACKENDS` in `plugins.rs` with real `available` flags
- [ ] **E4** Add lemmy + hackernews to test accounts panel in signup/mod.rs

---

## Parallel Agent Assignment

| Stream | Workspace | Priority |
|--------|-----------|----------|
| A — UI Fixes | main (workspcacemsg) | Now |
| B — Lemmy client + server | workspcacemsg2 | Now (parallel) |
| C — HN client + server | workspcacemsg3 | Now (parallel) |
| D — Phase 5 CLI | main (after A) | After A done |
| E — Plugin wiring | main | After B+C land |
