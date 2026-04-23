# Plan — Server Banners

> **Created:** 2026-04-21
> **Status:** 🟧 IN PROGRESS
> **Plan-author:** agent-a882b844

---

## §0 Status header

This plan tracks end-to-end server banner support (read + write) across three
backends: **poly-server**, **discord**, and **lemmy**.

Phases:
- **A** — `ClientBackend` trait (add `update_server_banner`)
- **B-poly** — poly-server: DB + API + wire client
- **B-discord** — Discord: API field + HTTP PATCH
- **B-lemmy** — Lemmy: API field + HTTP PUT
- **C** — Host UI wires `BannerPanel` to new trait method
- **D** — Unit tests per plugin

---

## §1 Backend research summary

### Discord

Source: Discord Developer Documentation (https://docs.discord.com/developers/resources/guild)

- **Read:** `GET /api/v10/guilds/{guild_id}` returns `banner: Option<String>` (hash
  string). URL formed as `https://cdn.discordapp.com/banners/{guild_id}/{hash}.png`.
- **Write:** `PATCH /api/v10/guilds/{guild_id}` accepts `banner` as a **base64 data URI**
  (format: `data:image/png;base64,<b64>` or jpeg). The Discord API describes this as
  "base64 16:9 png/jpeg image for the guild banner".
- **Dimensions:** Recommended 960×540 px (16:9). Maximum 1920×1080 for high-res.
- **Premium gating:** The `BANNER` guild feature is required; this is granted at
  Server Boost Level 2 (Tier 2). The test server (Spacebar-compatible mock) does NOT
  enforce premium tiers — all PATCH fields are accepted. Real Discord requires Tier 2.
  Animated banners (GIF) additionally require the `ANIMATED_BANNER` feature (Tier 3).
- **Scope guard:** Animated banners and premium-tier gating are noted as **out-of-scope**
  for this plan. The implementation sends the data URI; if Discord returns 403 (no banner
  feature), the error is surfaced as `ClientError::PermissionDenied`.
- **Test server (9102):** The Spacebar-style mock in `servers/test-discord/` accepts
  `PATCH /api/v10/guilds/{id}` but currently only handles `name`. We add `banner` field
  storage and serialization.

### Lemmy

Sources:
- lemmy-js-client v0.17 source at https://github.com/LemmyNet/lemmy-js-client
- Lemmy docs at https://join-lemmy.org/docs/contributors/04-api.html

- **Read:** `GET /api/v3/community/list` and `GET /api/v3/community` responses include
  `community.banner: Option<String>`. The value is a full URL string pointing to a
  pictrs-hosted image (e.g. `https://instance/pictrs/image/{filename}`).
- **Write:** `PUT /api/v3/community` (EditCommunity) accepts `banner: Option<String>`.
  The value must be a **URL string** — specifically a previously-uploaded pictrs URL.
  Upload flow: `POST /pictrs/image` (multipart) → get back the image URL → pass URL
  to `PUT /api/v3/community`. Our plan scope is **URL-only** (no file upload); the
  BannerPanel URL input already accepts a URL string.
- **Auth:** JWT required in `Authorization: Bearer <jwt>` header for PUT.
- **Test server (9104):** The mock in `servers/test-lemmy/` currently handles
  `GET /api/v3/community` but does NOT handle `PUT /api/v3/community`. We add a
  `PUT /api/v3/community` route that stores the banner URL in the community state.
  `Community` struct needs a `banner: Option<String>` field.

### poly-server

Source: `servers/server/src/` in this repo.

- **Current state:** The SQLite schema for the `server` table has `icon_url TEXT` but
  NO `banner_url` column. The SurrealDB schema likewise has `icon_url` but no
  `banner_url`. The REST API `PATCH /servers/{id}` accepts `name` and `icon_url` but
  not `banner_url`. The WS `ServerUpdated` event carries `icon_url` but not
  `banner_url`. The client-side `WireServer` model has no `banner_url` field.
- **Plan:** Add `banner_url TEXT` column to SQLite schema (migration-safe with
  `ALTER TABLE IF NOT EXISTS` / `ADD COLUMN IF NOT EXISTS`), add `banner_url` to
  `WireServer`, extend `update_server` on both DB backends, extend the REST API
  request body, and the HTTP client helper.

---

## §2 Current state matrix

| Aspect | poly-server | discord | lemmy |
|---|---|---|---|
| Backend stores banner_url | **N** (no DB column) | Y (Discord API has it) | Y (Lemmy API has it) |
| Plugin reads banner from API | **N** (hardcoded `None`) | **N** (field missing from `DiscordGuild`) | **Y** (`community.banner` read) |
| Plugin exposes banner via WIT/trait to host | partial (Server struct has `banner_url` but always `None`) | partial (same) | **Y** (mapped in `map_community_to_server`) |
| Host renders banner | Y (in channel list header `draft_banner.rs`) | Y (renders `server.banner_url` if set) | Y |
| Host has UI to change banner (local-only) | Y (`BannerPanel` in overview.rs) | Y | Y |
| Plugin implements `update_server_banner` | **N** (not yet in trait) | **N** | **N** |

Gaps:
1. The `ClientBackend` trait has no `update_server_banner` method.
2. poly-server DB/API does not store or expose `banner_url`.
3. Discord plugin does not read the `banner` hash from the guild object.
4. Discord plugin has no PATCH method that can send a banner data URI.
5. Lemmy plugin does not implement a PUT community endpoint call for banner updates.
6. The `BannerPanel` host UI saves only to `AppSettings` (local override), not to
   backend API.

---

## §3 Implementation plan

### Phase A — `ClientBackend` trait extension

- [x] A1. Add `update_server_banner(server_id: &str, banner_url: Option<&str>) -> ClientResult<()>` to `ClientBackend` in `clients/client/src/lib.rs` with default impl returning `ClientError::NotSupported`. This is a URL-only method (no binary upload in scope).

### Phase B-poly — poly-server backend

- [x] B-P1. Add `banner_url TEXT` column to SQLite schema in `servers/server/src/db/sqlite.rs` (add `ALTER TABLE … ADD COLUMN IF NOT EXISTS` migration call after schema CREATE). Also add to `WireServer` model.
- [x] B-P2. Extend `update_server` in sqlite.rs to accept and apply `banner_url: Option<String>`.
- [x] B-P3. Same for surreal.rs `update_server` + SurrealDB field definition.
- [x] B-P4. Extend `UpdateServerRequest` and `update_server` handler in `servers/server/src/api/servers.rs` to accept `banner_url`.
- [x] B-P5. Add `banner_url` to `WireServer` in `clients/server-client/src/models.rs` and update `parse_wire_server` helper.
- [x] B-P6. Add `update_server_banner` HTTP helper in `clients/server-client/src/http.rs`.
- [x] B-P7. Implement `update_server_banner` on `PolyServerBackend` in `clients/server-client/src/backend.rs`. Also read `banner_url` from the wire model in `map_server`.

### Phase B-discord — Discord backend

- [x] B-D1. Add `banner: Option<String>` to `DiscordGuild` in `clients/discord/src/api.rs`.
- [x] B-D2. Construct `banner_url` in `get_servers` / `get_server` in `clients/discord/src/lib.rs` using `https://cdn.discordapp.com/banners/{id}/{hash}.png`.
- [x] B-D3. Add `patch_guild_banner(guild_id: &str, banner_data_uri: Option<&str>)` to `DiscordHttpClient` in `clients/discord/src/http.rs`.
- [x] B-D4. Implement `update_server_banner` on `DiscordClient` in `clients/discord/src/lib.rs` — converts URL input to `ClientError::NotSupported` with hint (Discord requires base64 data URI, not a URL). For the test server path, pass the URL as-is in the `banner` field.
- [x] B-D5. Add `banner` field to test-discord state/routes: `Guild.banner`, `guild_to_json`, `PATCH /api/v10/guilds/{id}`.

### Phase B-lemmy — Lemmy backend

- [x] B-L1. Add `EditCommunityRequest` struct to `clients/lemmy/src/api.rs` with `community_id`, `banner: Option<String>`, `auth: String`.
- [x] B-L2. Add `put_community(community_id: i64, banner: Option<&str>) -> ClientResult<CommunityView>` to `LemmyHttpClient` in `clients/lemmy/src/api.rs`.
- [x] B-L3. Implement `update_server_banner` on `LemmyClient` in `clients/lemmy/src/lib.rs` — parses `server_id` back to community `i64`, calls `put_community`.
- [x] B-L4. Add `banner` field to `Community` in test-lemmy state.rs. Add `PUT /api/v3/community` route to test-lemmy lib.rs + routes.rs.

### Phase C — Host UI

- [x] C1. In `crates/core/src/ui/account/server/settings/overview.rs`, change `BannerPanel` to call `update_server_banner` via the `client_manager` after saving to `AppSettings`. Wire backend save into the save button `onclick` handler using `spawn(async move { … })`. Show error toast on failure, success badge on ok. Pass `backend_slug` so the panel can skip the API call for backends that don't support it (returns `NotSupported` anyway, but avoids confusing error toasts).

### Phase D — Tests

- [x] D1. `clients/discord/tests/banner.rs` — unit test that constructs a `DiscordGuild` JSON with a `banner` hash and verifies `get_servers()` returns the correct CDN URL. Uses the mock test-discord server.
- [x] D2. `clients/lemmy/tests/banner.rs` — test that `update_server_banner` calls `PUT /api/v3/community` with the correct `banner` field and that `get_servers()` returns the updated URL afterwards. Uses the mock test-lemmy server.
- [x] D3. `clients/server-client/tests/banner.rs` — test that `update_server_banner` calls `PATCH /servers/{id}` and that a subsequent `get_server()` returns the updated `banner_url`. Uses the in-process poly-server.

---

## §4 Out-of-scope

- **Animated GIF banners** (Discord ANIMATED_BANNER feature, Boost Tier 3). Noted for future.
- **Premium-tier gating** (Discord requires BANNER feature at Tier 2). The implementation
  surfaces `PermissionDenied` if the API rejects; no UI enforcement.
- **File picker / binary upload.** The `BannerPanel` URL input is a text field. File upload
  (multipart to Lemmy pictrs, base64 data URI conversion for Discord) is future work.
- **Image cropping/resizing UI.** Out of scope.
- **WASM plugin WIT surface.** The WIT `messenger-client` interface does not yet expose
  `update-server-banner`. The in-tree native backends implement the Rust trait directly.
  WIT extension is future work (separate plan).
- **E2 image proxy** (E2 in plan-ui-polish-round-2.md). This plan only handles
  banner-set / banner-render via URL. Proxy fetching is E2's concern.

---

## §5 Test matrix

| Test | File | What it verifies |
|---|---|---|
| `discord_banner_read` | `clients/discord/tests/banner.rs` | `get_servers()` maps `banner` hash → CDN URL |
| `discord_banner_not_in_response` | `clients/discord/tests/banner.rs` | `get_servers()` returns `None` when `banner` is absent |
| `lemmy_update_server_banner` | `clients/lemmy/tests/banner.rs` | `update_server_banner` calls PUT and banner round-trips |
| `lemmy_get_servers_banner_url` | `clients/lemmy/tests/banner.rs` | `get_servers()` returns `banner_url` from community |
| `poly_server_update_banner` | `clients/server-client/tests/banner.rs` | `update_server_banner` PATCH → re-read `get_server()` has banner |
