# Test Backends — Developer Reference

Mock HTTP servers that stand in for real messaging platforms during
development and integration testing. All are plain Axum servers seeded
with deterministic fixture data. They boot in about a second and expose
a `/health` endpoint you can poll.

## poly-test-runner — all-in-one launcher

The easiest way to start every backend at once:

```bash
# Build and launch all 8 backends (compiles on first run)
cargo run -p poly-test-runner -- --seed

# Verbose logs
cargo run -p poly-test-runner -- --seed --verbose
```

`poly-test-runner` spawns each backend as a child process on a fixed
port, waits for `/health → 200`, then prints a summary table and keeps
them alive until Ctrl+C. Kill it and all children stop.

Source: `servers/test-runner/src/main.rs`

---

## Shared Helpers — `test-common`

All backends link against `servers/test-common`. Key exports:

| Symbol | Description |
|--------|-------------|
| `serve_animal(name: &str) -> Response` | Serve a bundled animal PNG/SVG by bare name (e.g. `"koala"`, `"axolotl"`). Returns 404 for unknown names. |
| `CliArgs` | CLI parser: `--port`, `--seed`, `--verbose`, `--reset` |
| `health_handler(backend)` | Standard `{"status":"ok","backend":"…"}` response |
| `EventBus<T>` | `tokio::broadcast`-based event bus for real-time SSE/WS delivery |
| `AuthState` | Opaque token store: `create_token`, `validate`, `wipe_persisted` |

Avatar asset list (source: `clients/demo/assets/`):
- **PNG:** koala, kangaroo, platypus, owl, raccoon, stoat, lemming, sheep, walrus, cat, dog, parrot, cockatoo
- **SVG:** axolotl, beaver, hedgehog, flamingo, otter

See `servers/test-common/src/avatars.rs` for the full match table.

### Common lifecycle endpoints (all backends except hackernews, forgejo, github, lemmy)

| Endpoint | Method | Effect |
|----------|--------|--------|
| `/seed` | POST | Populate demo data (idempotent) |
| `/reset` | POST | Wipe all state to empty |
| `/reseed` | POST | Reset + seed in one call (use between test runs) |

---

## test-matrix — Port 9100

Matrix C2S mock. Implements login, sync, room membership, messaging,
media, and a subset of moderation endpoints.

**Health:**
```bash
curl http://127.0.0.1:9100/health
# {"status":"ok","backend":"matrix"}
```

**Seeded users:** `@owl:localhost`, `@axolotl:localhost`, `@cat:localhost`, `@dog:localhost`

**Avatar curl (proves route serves bytes):**
```bash
# Media thumbnail — strip trailing _avatar to get the animal name
curl -v http://127.0.0.1:9100/_matrix/media/v3/thumbnail/localhost/owl_avatar \
  -o /tmp/owl.png && file /tmp/owl.png
```

**Fetch messages (sync):**
```bash
# 1. Login
TOKEN=$(curl -s -X POST http://127.0.0.1:9100/_matrix/client/v3/login \
  -H "Content-Type: application/json" \
  -d '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"owl"},"password":"testpass123"}' \
  | jq -r '.access_token')

# 2. Initial sync (returns joined rooms + timeline)
curl -s "http://127.0.0.1:9100/_matrix/client/v3/sync" \
  -H "Authorization: Bearer $TOKEN" | jq '.rooms.join | keys'
```

**Reset / reseed:**
```bash
curl -X POST http://127.0.0.1:9100/reseed
```

---

## test-stoat — Port 9101

Stoat (Revolt-compatible) mock. Implements login, server/channel list,
messaging, presence, DMs.

**Health:**
```bash
curl http://127.0.0.1:9101/health
# {"status":"ok","backend":"stoat"}
```

**Seeded users:** `STOAT01` (stoat), `RACCOON01` (raccoon), `LEMMING01` (lemming)

**Avatar curl:**
```bash
# Avatar ID is av_{USER_ID}; route strips the av_ prefix
curl -v http://127.0.0.1:9101/avatars/av_STOAT01 \
  -o /tmp/stoat.png && file /tmp/stoat.png
```

**Auth:** `x-session-token: TOKEN` header (not Bearer)

**Fetch messages:**
```bash
# 1. Login
TOKEN=$(curl -s -X POST http://127.0.0.1:9101/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"stoat@example.com","password":"testpass123"}' \
  | jq -r '.token')

# 2. List channels on first server
curl -s http://127.0.0.1:9101/servers \
  -H "x-session-token: $TOKEN" | jq '.[0].id'
```

**Reset / reseed:**
```bash
curl -X POST http://127.0.0.1:9101/reseed
```

---

## test-discord — Port 9102

Discord REST + Gateway mock. Implements login, guilds, channels, messages,
forum threads, moderation.

**Health:**
```bash
curl http://127.0.0.1:9102/health
# {"status":"ok","backend":"discord"}
```

**Seeded users:** `1` (koala), `2` (kangaroo), `3` (platypus/wallaby stand-in)

**Avatar curl:**
```bash
# Pattern: /avatars/{user_id}/{animal_name}.png
curl -v http://127.0.0.1:9102/avatars/1/koala.png \
  -o /tmp/koala.png && file /tmp/koala.png
```

**Auth:** `Authorization: Bot TOKEN` or `Authorization: Bearer TOKEN`

**Fetch messages:**
```bash
# 1. Login (test endpoint)
TOKEN=$(curl -s -X POST http://127.0.0.1:9102/test/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"koala","password":"testpass123"}' \
  | jq -r '.token')

# 2. Get messages from channel 200
curl -s http://127.0.0.1:9102/api/v10/channels/200/messages \
  -H "Authorization: Bearer $TOKEN" | jq '.[0].content'
```

**Reset / reseed:**
```bash
curl -X POST http://127.0.0.1:9102/reseed
```

---

## test-teams — Port 9103

Microsoft Teams / Graph API mock. Implements OAuth2 token exchange,
team/channel list, messaging, member roster, profile photos.

**Health:**
```bash
curl http://127.0.0.1:9103/health
# {"status":"ok","backend":"teams"}
```

**Seeded users:** `U001` (Sheep, sheep@contoso.com), `U002` (Walrus, walrus@contoso.com)

**Avatar curl (Graph profile-photo path):**
```bash
# Pattern: /v1.0/users/{user_id}/photo/$value
TOKEN=$(curl -s -X POST http://127.0.0.1:9103/test/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"sheep@contoso.com","password":"testpass123"}' \
  | jq -r '.token')

curl -v "http://127.0.0.1:9103/v1.0/users/U001/photo/\$value" \
  -H "Authorization: Bearer $TOKEN" \
  -o /tmp/sheep.png && file /tmp/sheep.png
```

**Fetch messages:**
```bash
# List teams/channels (Graph-style)
curl -s http://127.0.0.1:9103/v1.0/me/joinedTeams \
  -H "Authorization: Bearer $TOKEN" | jq '.[0].id'
```

**Reset / reseed:**
```bash
curl -X POST http://127.0.0.1:9103/reseed
```

---

## test-lemmy — Port 9104

Lemmy community/forum mock. Implements login, community list, post list/create,
comment list/create, private messages. Avatar routes follow Lemmy's pict-rs URL
convention.

**Note:** seed data in `state.rs` hardcodes `localhost:9108` in avatar URLs — this
is a known seed artefact. The server listens on port 9104 (set by `poly-test-runner`).
The avatar route itself works regardless; only the URLs embedded in API responses
contain the wrong port if the server is started independently without `--port 9104`.

**Health:**
```bash
curl http://127.0.0.1:9104/health
# {"status":"ok","backend":"lemmy"}
```

**Seeded users:** `testuser` (axolotl avatar), `beaver`, `hedgehog`

**Avatar curl (pict-rs style — extension included):**
```bash
# Pattern: /pictrs/image/{filename} (includes extension)
curl -v http://127.0.0.1:9104/pictrs/image/beaver.svg \
  -o /tmp/beaver.svg && file /tmp/beaver.svg
```

**Fetch posts:**
```bash
# 1. Login
TOKEN=$(curl -s -X POST http://127.0.0.1:9104/api/v3/user/login \
  -H "Content-Type: application/json" \
  -d '{"username_or_email":"beaver","password":"testpass123"}' \
  | jq -r '.jwt')

# 2. List posts in all communities
curl -s "http://127.0.0.1:9104/api/v3/post/list" \
  -H "Authorization: Bearer $TOKEN" | jq '.posts[0].post.name'
```

Lemmy does NOT expose `/seed`, `/reset`, or `/reseed` — state is seeded at startup
and reset by restarting the process (`poly-test-runner` Ctrl+C → restart with `--seed`).

---

## test-hackernews — Port 9105

Read-only HN mock. Serves a fixed set of stories and comments. No user accounts,
no avatars (HN has no avatar system; the UI falls back to coloured initials).

**Health:**
```bash
curl http://127.0.0.1:9105/health
# {"status":"ok","backend":"hackernews"}
```

**Fetch stories:**
```bash
curl http://127.0.0.1:9105/v0/topstories.json | jq '.[0:5]'
curl http://127.0.0.1:9105/v0/item/1.json | jq '.title'
```

No auth, no seed/reset endpoints. Purely read-only fixture data.

---

## test-forgejo — Port 9106

Forgejo / Gitea mock. Implements login, repo list, issues, comments, PR reviews.

**Health:**
```bash
curl http://127.0.0.1:9106/health
# {"status":"ok","backend":"forgejo"}
```

**Seeded users:** `otter` (id 1), `flamingo` (id 2), `testuser` (id 3, axolotl avatar)

**Avatar curl:**
```bash
# Pattern: /avatars/{name} — bare animal name, no extension
curl -v http://127.0.0.1:9106/avatars/otter \
  -o /tmp/otter.svg && file /tmp/otter.svg
```

**Auth:** `Authorization: token TOKEN` (Forgejo/Gitea convention, not Bearer)

**Fetch issues:**
```bash
# 1. Login
TOKEN=$(curl -s -X POST http://127.0.0.1:9106/api/v1/user/signin \
  -H "Content-Type: application/json" \
  -d '{"username":"otter","password":"testpass123"}' \
  | jq -r '.sha1')

# 2. List repos + issues
curl -s http://127.0.0.1:9106/api/v1/user/repos \
  -H "Authorization: token $TOKEN" | jq '.[0].full_name'
```

Forgejo does NOT expose `/seed`, `/reset`, `/reseed` — state is seeded at startup.

---

## test-github — Port 9107

GitHub REST API mock. Implements login, repo list, issues, comments, PR reviews.

**Health:**
```bash
curl http://127.0.0.1:9107/health
# {"status":"ok","backend":"github"}
```

**Seeded users:** `penguin` (login, avatar aliased to koala.png), `chameleon` (avatar aliased to parrot.png)

**Note:** No penguin/chameleon PNG assets exist in `clients/demo/assets/`; the route
aliases penguin → koala and chameleon → parrot. The URL shape is correct; only the
rendered image is an approximation.

**Avatar curl:**
```bash
# Pattern: /avatars/{login}.png
curl -v http://127.0.0.1:9107/avatars/penguin.png \
  -o /tmp/penguin.png && file /tmp/penguin.png
```

**Auth:** `Authorization: token TOKEN` or `Authorization: Bearer TOKEN`

**Fetch issues:**
```bash
# 1. Login
TOKEN=$(curl -s -X POST http://127.0.0.1:9107/test/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"penguin","password":"testpass123"}' \
  | jq -r '.token')

# 2. List repos + issues
curl -s http://127.0.0.1:9107/user/repos \
  -H "Authorization: token $TOKEN" | jq '.[0].full_name'
```

GitHub does NOT expose `/seed`, `/reset`, `/reseed` — state is seeded at startup.

---

## test-reddit — Port 9108

old.reddit.com HTML-scrape mock. Implements cookie-based login + modhash, subreddit
listings (Hot/New/Top/Controversial), post + comment tree, inbox / DMs, subscriptions,
votes, top-level submit, comment reply, edit / delete, mark-read. Real-shape HTML
fixtures captured from `old.reddit.com` live under `clients/reddit/tests/fixtures/`.

**Health:** uses the shared `poly-test-common` lifecycle helper; the canonical probe
is the front-page HTML:
```bash
curl -s http://127.0.0.1:9108/ | grep -c '<html'   # → 1
```

**Seeded users:** `cat` (avatar: cat), `dog` (avatar: dog). Password for both:
`testpass123`. Sessions are issued as `mock_session_<user>_<n>` cookies named
`reddit_session`.

**Avatar curl:**
```bash
# Pattern: /avatars/{animal} — bare name, no extension.
curl -v http://127.0.0.1:9108/avatars/cat -o /tmp/cat.png && file /tmp/cat.png
```

**Login + post-list:**
```bash
# 1. Login (mock — accepts the seeded password).
COOKIE=$(curl -s -i -X POST http://127.0.0.1:9108/api/login/cat \
  --data 'passwd=testpass123' | awk '/^set-cookie:/ {print $2}' | tr -d '\r;')

# 2. Fetch r/rust hot listing (HTML page).
curl -s -H "Cookie: $COOKIE" http://127.0.0.1:9108/r/rust/hot/ | head -c 200
```

**Write-side endpoints (POST, form-encoded, requires session cookie):**
| Path                  | Form fields                            | Effect                     |
|-----------------------|----------------------------------------|----------------------------|
| `/api/submit`         | `sr`, `kind=self`, `title`, `text`     | Top-level self-post        |
| `/api/comment`        | `thing_id` (t1_/t3_/t4_), `text`       | Reply / inbox-reply        |
| `/api/del`            | `id` (t1_/t3_)                         | Delete own thing           |
| `/api/editusertext`   | `thing_id` (t1_/t3_), `text`           | Edit own body              |
| `/api/read_message`   | `id` (t4_)                             | Mark DM read               |
| `/api/vote`           | `id`, `dir` (1/0/-1)                   | Vote                       |
| `/api/compose`        | `to`, `subject`, `text`                | Send DM                    |
| `/api/subscribe`      | `sr` (t5_), `action` (sub/unsub)       | Subscribe / unsubscribe    |

Anonymous calls to any write endpoint return `401 logged out`.

**Reset:**
```bash
curl -X POST http://127.0.0.1:9108/test/reset    # wipes sessions, DMs, votes, etc.
```
