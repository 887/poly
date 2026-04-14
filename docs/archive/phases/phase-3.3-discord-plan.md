# Phase 3.3 Plan — Discord & Spacebar Client

> **Created:** 2026-03-30
> **Last updated:** 2026-04-14
> **Status:** 🟡 In Progress (REST stub + test server exist; Gateway + Spacebar dual-mode TODO)
> **Crate:** `poly-discord`
> **Goal:** View and interact with Discord (official) **and** Spacebar/Fosscord self-hosted instances. Both are first-class backends.
> **Approach:** Direct REST + Gateway WebSocket with user tokens. `base_url` swappable so the same client drives both Discord and Spacebar.
> **Note:** Discord's ToS prohibits unofficial clients — UI must warn users. Spacebar self-hosted has no ToS concern.
> **Licensing:** MIT-compatible only. Reference `twilight-rs` (ISC) freely. Do NOT copy from AGPL Spacebar/Fosscord code — use their running server as a compatibility target, not as a source.

---

## 3.3.1 Research & Approach — ✅ DECIDED

- [x] **3.3.1.1** Approach chosen: **direct REST (v10) + Gateway WebSocket, user tokens**
- [x] **3.3.1.2** Reference library chosen: **twilight-rs** (ISC-licensed) — depend on or mine for patterns
- [x] **3.3.1.3** Bridge approach rejected — adds complexity without solving ToS
- [x] **3.3.1.4** Webview approach rejected — fragile, Electron/Wry-only, can't ship to mobile
- [x] **3.3.1.5** **Spacebar/Fosscord elevated to first-class target** — users opting out of Discord
      ToS should have a native path. Same wire protocol, swap `base_url`.
- [x] **3.3.1.6** ToS warning required in signup UI for the Discord path; no warning for Spacebar path
- [x] **3.3.1.7** Licensing boundary: MIT/ISC/Apache only. AGPL projects (Spacebar server, Fosscord)
      are reference-only — we do not copy or link against them.

---

## 3.3.2 Implementation

- [x] **3.3.2.1** Auth flow — user token via `GET /users/@me`; signup UI stores token
- [x] **3.3.2.2** Implement `ClientBackend` trait for `DiscordClient` (REST portion)
- [x] **3.3.2.3** Guild (server) retrieval via `/users/@me/guilds`
- [ ] **3.3.2.4** Channel list with categories (currently flat — `parent_id` ignored)
- [ ] **3.3.2.5** Message send/receive — text only, no embeds/attachments yet
- [ ] **3.3.2.6** User profiles, presence, avatars (avatars always `None` today)
- [ ] **3.3.2.7** DMs (list works) and group DMs (type 3 not handled)
- [ ] **3.3.2.8** Friend list and friend requests (`/users/@me/relationships`)
- [ ] **3.3.2.9** Server icons, banners, and channel info — CDN URL construction
- [ ] **3.3.2.10** Slash commands / application commands (stretch)
- [ ] **3.3.2.11** `global_name` display field preferred over `username`
- [ ] **3.3.2.12** Rate limit handling — parse `X-RateLimit-*` headers, auto-retry on 429

## 3.3.2-SB Spacebar / Fosscord Dual-Mode — MUST HAVE

- [ ] **3.3.2-SB.1** Signup UI: two paths — "Connect to Discord" (with ToS warning) and
      "Connect to Spacebar instance" (prompts for `base_url`, no warning)
- [ ] **3.3.2-SB.2** Store `base_url` on the session; all REST/Gateway calls respect it
- [ ] **3.3.2-SB.3** Gateway URL discovery for Spacebar — call `GET /api/v10/gateway`
      against the custom host rather than hardcoding `gateway.discord.gg`
- [ ] **3.3.2-SB.4** Lenient deserialization — `#[serde(default)]` + `Option<T>` on
      fields added by Discord but not yet in Spacebar
- [ ] **3.3.2-SB.5** Local Spacebar smoke test — `docker compose up` a Spacebar instance,
      run `poly-discord` against it, verify: login → list guilds → list channels →
      send message → receive via Gateway → logout
- [ ] **3.3.2-SB.6** Document Spacebar quirks as we find them (in `clients/discord/agents.md`)
- [ ] **3.3.2-SB.7** Once stable, submit listing to <https://spacebar-explorer.sovr.top/clients>

---

## 3.3.3 Real-Time Events

- [ ] **3.3.3.1** Gateway WebSocket connection (or bridge events, depending on approach)
- [ ] **3.3.3.2** Message events (new, edit, delete)
- [ ] **3.3.3.3** Presence updates
- [ ] **3.3.3.4** Typing indicators

---

## 3.3.4 Voice/Video

- [ ] **3.3.4.1** Discord voice gateway integration (or bridge-based audio)
- [ ] **3.3.4.2** Voice channel join/leave
- [ ] **3.3.4.3** Video and screen share (stretch)

---

## 3.3.5 WASM Guest Implementation

> Port native `DiscordClient` logic to `guest.rs` using `host_api::http_request()` for REST and `host_api::websocket_*()` for Gateway.

- [ ] **3.3.5.1** Auth in guest (token validation or OAuth2 token exchange)
- [ ] **3.3.5.2** Guild/channel/message methods via REST
- [ ] **3.3.5.3** `handle_ws_data()` — parse Gateway WebSocket payloads, call `emit-event` for MESSAGE_CREATE, TYPING_START, PRESENCE_UPDATE etc.
- [ ] **3.3.5.4** Guest E2E tests in `crates/plugin-host-tests/tests/client_e2e/discord.rs`

---

## 3.3.6 Mock Test Server & Manual UI Testing

> See Phase 4 plan (`docs/phase-4-test-servers-plan.md` §4.5) for full details. This section tracks Discord-specific test server integration.

**Test accounts:** Koala + Kangaroo (cartoony avatar PNGs matching Cat/Dog style)
**Crate:** `servers/test-discord/` (binary: `poly-test-discord`)

- [ ] **3.3.6.1** Build mock Discord API server implementing all REST endpoints the plugin calls (see §4.5 checklist)
- [ ] **3.3.6.2** Mock Gateway WebSocket — IDENTIFY, READY, dispatch MESSAGE_CREATE/TYPING_START/PRESENCE_UPDATE
- [ ] **3.3.6.3** `/reset` and `/seed` endpoints with demo data (2 users, 2 guilds with categories + channels, DM channel, messages)
- [ ] **3.3.6.4** Signup/token registration flow support
- [ ] **3.3.6.5** Integration test: `poly-discord` plugin authenticates → list guilds → list channels → send message → receive via Gateway → logout
- [ ] **3.3.6.6** Manual UI test: connect Poly app to `localhost` test server, verify sidebar/chat/DMs render correctly

---

## Completion Criteria

- [x] Approach decision documented with ToS risk assessment
- [ ] Can view Discord servers and channels (REST works, no real-time yet)
- [ ] Can send and receive messages in channels and DMs
- [ ] Friend list and DMs work
- [ ] User is shown appropriate ToS warning before connecting to discord.com
- [ ] Voice channels work (stretch — may not be feasible depending on approach)
- [ ] Mock test server passes full E2E smoke test (REST ✅, Gateway ❌)
- [ ] WASM guest has parity with native for all core chat features
- [ ] **Spacebar: signup UI offers custom `base_url` mode with no ToS warning**
- [ ] **Spacebar: local compatibility smoke test passes (guilds, channels, messages, Gateway)**
- [ ] **Twilight-rs either depended on or mined thoroughly for model types and Gateway logic**
- [ ] **No AGPL code in the final build — verified via license audit of the dependency tree**
