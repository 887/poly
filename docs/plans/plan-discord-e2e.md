# Discord End-to-End Test Plan

## Status: ✅ DONE — HTTP-only suite green (35 passed / 1 intentional skip)

> Closed out 2026-04-30. The four spec files cover the full HTTP surface
> from section 1 against `poly-test-discord`; gateway-WS, auth, message
> CRUD, group-DM, context menus, and moderation all pass. UI-level
> Playwright tests, real-OAuth coverage, and reaction/thread HTTP wiring
> were always scoped out — see "Punted to follow-up plans" below.

Close-out fixes shipped today:
- **`servers/test-discord/src/main.rs`** — set `gateway_url` dynamically
  from the bound address with `/gateway/ws` path so tests on any port
  see a working ws:// URL (was hardcoded `ws://localhost:9102` with no
  path → `Unexpected server response: 404` in gateway-ws test).
- **`playwright.config.ts`** — `discord-api` project now runs with
  `fullyParallel: false, workers: 1`. Specs share one mock server and
  call `/reseed` in `beforeEach`; parallel workers raced and produced
  401-vs-404 flakes on auth-bound assertions.

## Punted to follow-up plans

- **UI-level Playwright tests** (full WASM `poly-web` driving) — defer
  until `plan-meta-personalities.md` Phase D ships the persona UI; both
  surfaces can share one Playwright session.
- **Real-OAuth E2E** — needs `DISCORD_CLIENT_SECRET` + sandbox bot;
  spec stub at `discord-auth.spec.ts:151` is unconditionally skipped.
- **Reactions / thread creation HTTP tests** — add when
  `clients/discord/src/http.rs` grows the corresponding methods (still
  `wit_bindings.rs`-only today).

> Created: 2026-04-27
> Scope: Playwright + mock-server E2E coverage for the poly-discord backend.

---

## 1. Feature matrix

Each row is one feature wired in `clients/discord/src/`. "Mock-safe" means the
test can run against `poly-test-discord` on CI without real OAuth credentials.

| Feature | HTTP client method | Discord API endpoint | Mock-safe? | Notes |
|---|---|---|---|---|
| Password login (Spacebar-compat) | `login` | `POST /api/v10/auth/login` | Yes | Real Discord needs browser OAuth; skip on CI |
| Real OAuth | `signup.rs` | browser redirect | No | CI skip — requires `DISCORD_CLIENT_SECRET` |
| Fetch self user | `get_me` | `GET /api/v10/users/@me` | Yes | Auth probe after login |
| List guilds | `get_guilds` | `GET /api/v10/users/@me/guilds` | Yes | Server list population |
| Get single guild | `get_guild` | `GET /api/v10/guilds/{id}` | Yes | |
| List guild channels | `get_guild_channels` | `GET /api/v10/guilds/{id}/channels` | Yes | Channel sidebar |
| Get channel | `get_channel` | `GET /api/v10/channels/{id}` | Yes | |
| List messages | `get_messages` | `GET /api/v10/channels/{id}/messages` | Yes | History load |
| Send message | `send_message` | `POST /api/v10/channels/{id}/messages` | Yes | Core send path |
| Open DM | `open_dm` | `POST /api/v10/users/@me/channels` | Yes | |
| List DMs | `get_dm_channels` | `GET /api/v10/users/@me/channels` | Yes | |
| Delete channel (close DM / leave group) | `delete_channel` | `DELETE /api/v10/channels/{id}` | Yes | Group DM leave too |
| Add friend / block | `put_relationship` | `PUT /api/v10/users/@me/relationships/{id}` | Yes | type=1 friend, type=2 block |
| Remove friend / unblock | `delete_relationship` | `DELETE /api/v10/users/@me/relationships/{id}` | Yes | |
| Set user note | `put_user_note` | `PUT /api/v10/users/@me/notes/{id}` | Yes | |
| Add user to group DM | `add_group_dm_recipient` | `PUT /api/v10/channels/{id}/recipients/{uid}` | Yes | |
| Create invite | `create_invite` | `POST /api/v10/channels/{id}/invites` | Yes | |
| Get user | `get_user` | `GET /api/v10/users/{id}` | Yes | Profile lookup |
| Trigger typing | `trigger_typing` | `POST /api/v10/channels/{id}/typing` | Yes (no-op) | Mock returns 204 |
| Kick member | `kick_member` | `DELETE /api/v10/guilds/{id}/members/{uid}` | Yes | Moderation |
| Ban member | `ban_member` | `PUT /api/v10/guilds/{id}/bans/{uid}` | Yes | Moderation |
| Unban member | `unban_member` | `DELETE /api/v10/guilds/{id}/bans/{uid}` | Yes | Moderation |
| Get bans | `get_bans` | `GET /api/v10/guilds/{id}/bans` | Yes | Moderation |
| Set member timeout | `set_member_timeout` | `PATCH /api/v10/guilds/{id}/members/{uid}` | Yes | Moderation |
| Delete message | `delete_message` | `DELETE /api/v10/channels/{id}/messages/{mid}` | Yes | Moderation |
| Patch channel | `patch_channel` | `PATCH /api/v10/channels/{id}` | Yes | Moderation |
| Reorder channels | `reorder_channels` | `PATCH /api/v10/guilds/{id}/channels` | Yes | Moderation |
| Audit log | `get_audit_log` | `GET /api/v10/guilds/{id}/audit-logs` | Yes | Moderation |
| My guild member | `get_guild_member_me` | `GET /api/v10/guilds/{id}/members/@me` | Yes | Permission check |
| Guild roles | `get_guild_roles` | `GET /api/v10/guilds/{id}/roles` | Yes | Permission check |
| Patch guild | `patch_guild` | `PATCH /api/v10/guilds/{id}` | Yes | Server settings |
| Active threads | `get_active_threads` | `GET /api/v10/guilds/{id}/threads/active` | Yes | Forum / thread list |
| Archived threads | `get_archived_threads_public` | `GET /api/v10/channels/{id}/threads/archived/public` | Yes | Forum archive |
| Gateway WebSocket | (gateway client) | `wss://…/gateway/ws` | Yes | Real-time events |

---

## 2. What Playwright tests cover

The Playwright specs in `tests/e2e/discord/` are **HTTP-intercept + mock-server
tests**. They do not render the full Poly WASM app. Instead they call the
`poly-test-discord` REST API directly to verify request shapes the client
would send, then optionally load the UI in a plain page with `page.route()`
intercepts to verify that UI actions produce the correct HTTP requests.

### 2.1 What requires real OAuth (skip on CI)

The following tests are marked `test.skip` unless the env var
`DISCORD_TEST_WITH_REAL_OAUTH=1` is set:

- Any test that requires a live `discord.com` token
- Signup flow via `oauth2/authorize`

All other tests run against the local `poly-test-discord` server.

---

## 3. Mock server (`servers/test-discord/`)

The server is a pre-existing Rust/axum crate (`poly-test-discord`). It
implements the full endpoint surface listed in section 1.

### Seed data

| ID | Type | Name |
|----|------|------|
| User 1 | User | koala (`testpass123`) |
| User 2 | User | kangaroo (`testpass123`) |
| User 3 | User | wallaby (`testpass123`) |
| Guild 100 | Guild | Australiana (owner: koala) |
| Guild 101 | Guild | Wildlife Chat (owner: kangaroo) |
| Channel 200 | GuildText | #general (guild 100) |
| Channel 201 | GuildText | #random (guild 100) |
| Channel 300 | Private (DM) | — |
| Channel 500 | GuildForum | #general-discussion (guild 100) |

### Auth

`POST /test/auth/token` with `{ "username": "koala" }` returns a bearer token
without password check — use this in CI instead of the login endpoint.

### Endpoints new in this plan

The following endpoints were added in this plan (all were already called by
`DiscordHttpClient` but not yet routed in the test server):

- `PUT /api/v10/users/@me/relationships/{user_id}` — returns 204
- `DELETE /api/v10/users/@me/relationships/{user_id}` — returns 204
- `PUT /api/v10/users/@me/notes/{user_id}` — returns 204
- `DELETE /api/v10/channels/{channel_id}` — closes DM, returns 204
- `PUT /api/v10/channels/{channel_id}/recipients/{user_id}` — returns 204
- `POST /api/v10/channels/{channel_id}/invites` — returns synthetic invite JSON

---

## 4. Playwright spec inventory

| File | Tests |
|------|-------|
| `tests/e2e/discord/discord-auth.spec.ts` | Token login → `GET /users/@me` returns user, `GET /users/@me/guilds` returns guild list |
| `tests/e2e/discord/discord-message.spec.ts` | Send message → persisted in `GET /channels/{id}/messages`, gateway event emitted |
| `tests/e2e/discord/discord-context-menus.spec.ts` | Block user → `PUT /relationships/2` with `{type:2}`, ignore (block type) |
| `tests/e2e/discord/discord-group-dm.spec.ts` | Leave group DM → `DELETE /channels/300`, channel removed from list |

---

## 5. Running

See `tests/e2e/discord/README.md` for full instructions.

Quick start:

```bash
# Terminal 1 — start mock server on port 9200
cargo run -p poly-test-discord -- --port 9200 --seed

# Terminal 2 — run specs
DISCORD_MOCK_URL=http://localhost:9200 npx playwright test tests/e2e/discord/
```

---

## 6. TODOs / punted items

- **Real OAuth E2E**: Requires `DISCORD_CLIENT_SECRET` + a Discord test application.
  The spec stubs are present but unconditionally skipped on CI via `test.condition`.
- **UI-level Playwright tests**: The specs in this plan drive the HTTP API
  directly. Full WASM UI tests (click sidebar, see messages) require a running
  `poly-web` instance and are left to a follow-up plan.
- **Reactions / thread creation via HTTP**: The Discord client has reaction
  endpoints in `wit_bindings.rs` but they are not yet in `DiscordHttpClient`.
  Add them when the client surface is extended.
- **Gateway real-time in Playwright**: The gateway WebSocket tests in
  `discord-message.spec.ts` use `ws://` connections directly; full UI push
  requires loading the WASM bundle.
