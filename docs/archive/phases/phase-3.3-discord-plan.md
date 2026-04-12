# Phase 3.3 Plan — Discord Client

> **Created:** 2026-03-30
> **Status:** ⬜ Not Started
> **Crate:** `poly-discord`
> **Goal:** View and interact with Discord servers/channels/DMs. Approach TBD.
> **Note:** Discord's ToS prohibits unauthorized API clients — approach decision must weigh this carefully.

---

## 3.3.1 Research & Approach Decision

- [ ] **3.3.1.1** Re-evaluate Discord client landscape (new crates? policy changes since last review?)
- [ ] **3.3.1.2** Test `discord_client_gateway` / `discord_client_rest` crates (if still maintained)
- [ ] **3.3.1.3** Evaluate bridge approach (Matrix bridge via `mautrix-discord` — user runs bridge server)
- [ ] **3.3.1.4** Evaluate webview approach (hidden webview running Discord web — Electron/Wry only)
- [ ] **3.3.1.5** Evaluate hybrid approach (background official client + data extraction via IPC/logs)
- [ ] **3.3.1.6** **DECISION: Choose implementation approach** — document in `clients/discord/agents.md`
- [ ] **3.3.1.7** Document ToS implications and required user warnings/disclaimers in UI

---

## 3.3.2 Implementation (approach-dependent)

- [ ] **3.3.2.1** Auth flow (token, OAuth2 PKCE, or webview-based — depends on chosen approach)
- [ ] **3.3.2.2** Implement `ClientBackend` trait for `DiscordClient`
- [ ] **3.3.2.3** Guild (server) retrieval and display
- [ ] **3.3.2.4** Channel list with categories (text, voice, forum, announcement channels)
- [ ] **3.3.2.5** Message send/receive (text, embeds, attachments)
- [ ] **3.3.2.6** User profiles, presence, avatars
- [ ] **3.3.2.7** DMs and group DMs (up to 10 users)
- [ ] **3.3.2.8** Friend list and friend requests
- [ ] **3.3.2.9** Server icons, banners, and channel info
- [ ] **3.3.2.10** Slash commands / application commands (if feasible)

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

- [ ] Approach decision documented with ToS risk assessment
- [ ] Can view Discord servers and channels
- [ ] Can send and receive messages in channels and DMs
- [ ] Friend list and DMs work
- [ ] User is shown appropriate ToS warning before connecting
- [ ] Voice channels work (stretch — may not be feasible depending on approach)
- [ ] Mock test server passes full E2E smoke test
- [ ] WASM guest has parity with native for all core chat features
