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

## Completion Criteria

- [ ] Approach decision documented with ToS risk assessment
- [ ] Can view Discord servers and channels
- [ ] Can send and receive messages in channels and DMs
- [ ] Friend list and DMs work
- [ ] User is shown appropriate ToS warning before connecting
- [ ] Voice channels work (stretch — may not be feasible depending on approach)
