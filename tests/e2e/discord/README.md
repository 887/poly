# Discord E2E Tests

Playwright test suite for the Discord backend in Poly. Tests call
`poly-test-discord` directly over HTTP — no running WASM app required.

## Prerequisites

- Rust toolchain (for `poly-test-discord`)
- Node.js 18+ with npm
- `npx playwright install chromium` (only needed if you run browser-based specs)

## Quick start

```bash
# Terminal 1 — start the mock server
cargo run -p poly-test-discord -- --port 9200 --seed

# Terminal 2 — run all Discord specs
DISCORD_MOCK_URL=http://localhost:9200 \
  npx playwright test tests/e2e/discord/ --project discord-api
```

## Playwright config

The Discord specs use the `discord-api` project defined in `playwright.config.ts`.
That project uses `request` fixtures only (no browser), so `browserName` and
`viewport` are irrelevant.

To run with the default config (which targets port 3001):

```bash
npx playwright test tests/e2e/discord/
```

Or set `DISCORD_MOCK_URL` to override the base URL:

```bash
DISCORD_MOCK_URL=http://localhost:9200 npx playwright test tests/e2e/discord/
```

## Running against real Discord (CI skip)

Tests that require a real Discord token are unconditionally skipped unless:

```
DISCORD_TEST_WITH_REAL_OAUTH=1
DISCORD_TEST_TOKEN=<your-bot-token>
```

are set. Never set these in CI unless you have a dedicated test bot with a
sandbox guild.

## Mock server reference

The mock server lives at `servers/test-discord/`. It implements all Discord API
v10 endpoints called by `clients/discord/src/http.rs`.

| Endpoint | Notes |
|----------|-------|
| `POST /test/auth/token` | CI-only: get a token without a password |
| `POST /seed` | Seed demo data (idempotent) |
| `POST /reset` | Wipe all state |
| `POST /reseed` | Reset + seed (used in `beforeEach`) |

## Spec overview

| File | What it tests |
|------|--------------|
| `discord-auth.spec.ts` | Login, token validation, `GET /users/@me`, guild list |
| `discord-message.spec.ts` | Send/receive messages, gateway `MESSAGE_CREATE` event |
| `discord-context-menus.spec.ts` | Block, add/remove friend, set note, invite to server |
| `discord-group-dm.spec.ts` | Open DM, leave DM (`DELETE /channels/{id}`), add recipient |

## Adding new tests

1. Add a `test(...)` block to the relevant spec file.
2. If the Discord API call you need is not yet in the mock server, add a handler
   to `servers/test-discord/src/routes.rs` and register it in `src/lib.rs`.
3. Run `cargo check -p poly-test-discord` to verify the Rust side compiles.
