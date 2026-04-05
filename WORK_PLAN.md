# Work Plan — Phase 3 clients + Phase 5 CLI

> Generated: 2026-04-05. Tracks all open work from this session.

---

## A — UI Fixes (fast, main agent)

- [ ] **A1** Remove `← Choose Backend` back button from stoat signup  
  File: `clients/stoat/src/signup.rs` lines 51-56 (the button with class `signup-nav-back`)

- [ ] **A2** Test Accounts page: add backend-type label to each card  
  File: `crates/core/src/ui/signup/mod.rs` — `TEST_ACCOUNTS` const + `TestAccountsPanel` rendering  
  Currently shows no label identifying which backend (Stoat/Matrix etc.)

- [ ] **A3** Test Accounts page: add accounts for all backends  
  Add: Matrix (localhost:8448), HN (no login needed), Lemmy (localhost:8536),  
  Discord (localhost:9102), Teams (localhost:9103)  
  Note: HN is read-only, login = add with no creds

- [ ] **A4** Plugins page: add `demo_forum` as visible entry "Demo (Forum)"  
  File: `crates/core/src/ui/settings/plugins.rs` — `NATIVE_BACKENDS` const  
  Currently demo_forum runs but is invisible in plugins; user wants it shown  
  Slug: `demo_forum`, available: `true`

- [ ] **A5** Plugins page: add stub entries for HN and Lemmy (`available: cfg!(feature = "hackernews")` etc.)  
  Both show "not in this build" until the clients are compiled in

- [ ] **A6** Investigate "32 active accounts" on Demo  
  Code: `plugins.rs` line 357 — counts `sessions` by backend slug.  
  Demo creates 3 sessions (cat=demo, dog=demo, platypus=demo_forum).  
  Check if `demo_forum` sessions are being counted under `demo` slug — fix if so.  
  Root: demo_forum sessions have slug `demo_forum`, so they shouldn't match `demo`.  
  May be a misread — verify in live app. If real bug, fix the slug comparison.

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

### D1 — Audit existing MCP tools
- [ ] Check `mcp/chat-mcp/src/tools.rs`: does `login` work for stoat test server?
- [ ] Check `poly-cli` dynamic dispatch works end-to-end

### D2 — Easy-signin bypass (test server only)
- [ ] Add `test_signin` MCP tool to `mcp/chat-mcp/src/tools.rs`
  - Input: `{backend, url, username}` — no password
  - Guard: only allowed if `url` contains `localhost` or `127.0.0.1`
  - Internally creates a fake token / calls test server's `/test/auth/token` endpoint
- [ ] Add matching `/test/auth/token` endpoint to `servers/test-stoat/` (and test-lemmy, test-matrix)
  - Returns a valid session token for the given username without password check
  - Route guard: only available in test mode (always on in test servers by design)

### D3 — Full stoat CLI login flow
- [ ] `poly-cli login --backend stoat --url http://localhost:9101 --user stoat --pass testpass123`
- [ ] After login: MCP broadcasts to any connected poly-web UI via SSE/signal
- [ ] `poly-cli list-servers --account <id>`
- [ ] `poly-cli list-channels --account <id> --server <server-id>`
- [ ] `poly-cli list-dms --account <id>`
- [ ] `poly-cli list-friends --account <id>`
- [ ] `poly-cli get-messages --account <id> --channel <channel-id>`

### D4 — Test account easy-signin in UI
- [ ] Add "Quick Login" button to test accounts panel (no password needed for localhost)
- [ ] Calls test server `/test/auth/token` endpoint, bypasses OAuth/password flow

### D5 — Extend to other backends (post stoat)
- [ ] matrix test server easy-signin
- [ ] discord test server easy-signin
- [ ] teams test server easy-signin
- [ ] HN: no login needed
- [ ] lemmy: easy-signin on local test server

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
