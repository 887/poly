# Phase 3.1 Plan — Stoat (Revolt) Client

> **Created:** 2026-03-30
> **Status:** 🟡 In Progress
> **Crate:** `poly-stoat`
> **Goal:** Full chat + real-time events + voice/video with Stoat servers. WebRTC infrastructure built here is reused by all other backends.

---

## 3.1.1 Research & Planning

- [x] **3.1.1.1** Deep-dive Stoat REST API (developer docs, `clients/stoat/api-1.json`, `stoatchat/javascript-client-api`)
- [x] **3.1.1.2** Document all REST endpoints needed: auth, servers, channels, messages, users, voice — captured in `clients/stoat/SPEC.md`
- [ ] **3.1.1.3** Document WebSocket (Bonfire) event protocol — heartbeat, event types, reconnect
- [x] **3.1.1.4** Document Stoat auth flow (email/password, token resume, MFA/disabled detection) — in `SPEC.md`
- [ ] **3.1.1.5** Document Stoat voice/video protocol (Vortex / WebRTC specifics)
- [ ] **3.1.1.6** Test against official Stoat server and a self-hosted instance (end-to-end smoke test)
- [x] **3.1.1.7** Update `clients/stoat/agents.md` with all findings (updated 2026-03-17)

---

## 3.1.2 Core Stoat Client (Native)

- [x] **3.1.2.1** HTTP client setup (`reqwest`) with base URL normalization, instance ID derivation, token-header scaffolding
  - `clients/stoat/src/config.rs` + `src/http.rs`
- [x] **3.1.2.2** Authentication: email/password login, token resume, logout, MFA/disabled detection
  - `clients/stoat/src/api.rs`, `src/http.rs`, `src/lib.rs`
  - Mock-backed integration tests: `stoat_authenticate_*`, `stoat_logout_*`
- [~] **3.1.2.3** `ClientBackend` trait fully implemented for native `StoatClient`
  - ✅ Auth, channels, messages, send, users, presence, friends, DMs, groups, member mutations
  - ❌ `get_servers()` — intentionally `NotSupported` (no REST endpoint in Stoat; see 3.1.2.4)
  - ❌ `send_message(WithAttachments)` — `NotSupported` (blocked on `poly_client` attachment model)
  - ❌ `event_stream()` — returns empty stream (WebSocket not yet implemented)
  - ❌ Reactions add/remove/delete, message edit/delete, pin/unpin, search, block/unblock
- [ ] **3.1.2.4** Server list retrieval
  - Blocked: No documented `GET /servers` or `GET /users/@me/servers` endpoint in Stoat REST
  - Path forward: Bonfire WebSocket ready-state event OR confirmed REST endpoint from Stoat team
  - Currently returns `NotSupported`
- [x] **3.1.2.5** Channel list per server (with categories, unread/mention enrichment)
  - `get_server(id)`, `get_channels(server_id)`, `get_channel(id)`, `get_channel_members(channel_id)`
  - Unread enrichment from `GET /sync/unreads`
  - Mock tests: `stoat_get_server_*`, `stoat_get_channels_*`, `stoat_get_channel_*`
- [x] **3.1.2.6** Message retrieval (paginated, before/after/around, reply hydration, reactions, attachments)
  - `get_messages(channel_id, query)` → `GET /channels/{target}/messages`
  - ULID timestamp derivation, bundled user/member mapping, Autumn attachment URLs
  - Mock tests: `stoat_get_messages_*`
- [~] **3.1.2.7** Send messages (text + reply done; attachments blocked)
  - ✅ `send_message(Text)`, `send_reply_message(Text)` — nonce generation, reply-intent, preview hydration
  - ❌ `send_message(WithAttachments)` — blocked on shared `poly_client` attachment upload model
- [~] **3.1.2.8** User profiles and presence
  - ✅ `get_user(id)`, `get_presence(user_id)` — Autumn avatar, status → presence mapping
  - ❌ Rich profile surface (`/users/{id}/profile`) not yet reviewed/implemented
  - Mock tests: `stoat_get_user_*`, `stoat_get_presence_*`
- [~] **3.1.2.9** Friend list and friend requests
  - ✅ `get_friends()`, `get_notifications()` (incoming friend requests), `respond_to_friend_request()`
  - ✅ `send_friend_request(username)` helper
  - ❌ Full notification surface (beyond friend requests) not implemented
  - Mock tests: `stoat_get_friends_*`, `stoat_get_notifications_*`, `stoat_respond_to_friend_request_*`
- [~] **3.1.2.10** Group DMs / multi-user chats
  - ✅ `get_dm_channels()`, `get_groups()`, `open_direct_message_channel()`, `open_saved_messages_channel()`
  - ✅ `add_group_member()`, `remove_group_member()`, `get_channel_members()` for groups
  - ✅ SavedMessages surfaced as self-DM; DM list filters self-DM row
  - ❌ Unread/last-message parity in open-DM flows (guest-side enrichment)
  - Mock tests: `stoat_get_dm_channels_*`, `stoat_get_groups_*`, `stoat_open_*`, `stoat_add/remove_group_member_*`
- [x] **3.1.2.11** Self-hosted instance support (configurable base URL, normalized instance ID)

---

## 3.1.3 WASM Guest Parity

The guest in `clients/stoat/src/guest.rs` has **partial** real implementation:

- [x] Auth (email/password + token resume + logout) — via `host-api.http-request`
- [x] `open_direct_message_channel()`, `open_saved_messages_channel()`
- [x] `add_group_member()`, `remove_group_member()`
- [x] Guest E2E tests in `crates/plugin-host-tests/tests/client_e2e/stoat.rs`
- [ ] `get_server()`, `get_channels()`, `get_channel()` — still stubbed
- [ ] `get_messages()`, `send_message()`, `send_reply_message()` — still stubbed
- [ ] `get_user()`, `get_presence()` — still stubbed
- [ ] `get_friends()`, `get_notifications()`, `respond_to_friend_request()` — still stubbed
- [ ] `get_dm_channels()`, `get_groups()` — still stubbed
- [ ] Reaction, pin, search methods — still stubbed

---

## 3.1.4 Missing Native Features

- [ ] **Reactions** — add/remove/delete emoji reactions on messages
- [ ] **Message edit/delete** — `POST /channels/{target}/messages/{id}` (edit), `DELETE` (delete)
- [ ] **Message pins** — pin/unpin messages via Stoat API
- [ ] **Message search** — `GET /channels/{target}/search`
- [ ] **Block/unblock users** — user relationship mutations
- [ ] **Server invites** — generate/accept invite codes
- [ ] **Attachment send** — blocked on `poly_client` model; needs upload lifecycle (upload → ID → attach)
- [ ] **Rich profile surface** — `/users/{id}/profile` for richer profile UI data

---

## 3.1.5 Real-Time Events (WebSocket / Bonfire)

- [ ] **3.1.5.1** WebSocket connection management (connect to Bonfire WS URL from `GET /` config, reconnect, heartbeat)
- [ ] **3.1.5.2** Message events: new (`Message`), edit (`MessageUpdate`), delete (`MessageDelete`)
- [ ] **3.1.5.3** Presence updates (`UserUpdate`)
- [ ] **3.1.5.4** Typing indicators (`ChannelStartTyping`)
- [ ] **3.1.5.5** Notification events (friend request received, mention)
- [ ] **3.1.5.6** Channel/server update events (`ChannelUpdate`, `ServerUpdate`)
- [ ] **3.1.5.7** Map Stoat Bonfire events → `ClientEvent` enum in `event_stream()`
- [ ] **3.1.5.8** Native reconnect with exponential backoff
- [ ] **3.1.5.9** Guest: poll-based event bridge (WASM WebSockets or poll via HTTP long-poll)

---

## 3.1.6 WebRTC Voice Infrastructure (SHARED — built here, reused by Matrix/Discord/Teams)

- [ ] **3.1.6.1** Set up `webrtc = "0.17.1"` (or current stable) in workspace
- [ ] **3.1.6.2** ICE candidate gathering (STUN/TURN)
- [ ] **3.1.6.3** DTLS handshake
- [ ] **3.1.6.4** Audio capture (ALSA/PulseAudio on Linux, CoreAudio on Mac, WASAPI on Windows)
- [ ] **3.1.6.5** Audio encoding/decoding (Opus codec)
- [ ] **3.1.6.6** RTP/SRTP audio streaming
- [ ] **3.1.6.7** Voice channel join/leave (Stoat Vortex token handshake)
- [ ] **3.1.6.8** Mute/unmute, deafen controls
- [ ] **3.1.6.9** Voice activity detection (VAD)
- [ ] **3.1.6.10** Platform bridge for mobile (iOS/Android mic access)

---

## 3.1.7 WebRTC Video Infrastructure (SHARED)

- [ ] **3.1.7.1** Camera capture (platform-specific bridges)
- [ ] **3.1.7.2** Video encoding (VP8/VP9/H264)
- [ ] **3.1.7.3** Video decoding and rendering
- [ ] **3.1.7.4** Screen sharing capture
- [ ] **3.1.7.5** Video channel join/leave
- [ ] **3.1.7.6** Camera on/off controls
- [ ] **3.1.7.7** Platform bridge for mobile (iOS/Android camera access)

---

## 3.1.8 Integration Testing (Real Server)

- [ ] **3.1.8.1** Test auth flow against live `stoat.chat`
- [ ] **3.1.8.2** Test message send/receive in real channels (text, reply)
- [ ] **3.1.8.3** Test friend management (add, accept, reject)
- [ ] **3.1.8.4** Test DM open and group DM
- [ ] **3.1.8.5** Test voice call with another Stoat user
- [ ] **3.1.8.6** Test video call with another Stoat user
- [ ] **3.1.8.7** Test against self-hosted Revolt instance
- [ ] **3.1.8.8** Test adding Stoat server to Poly favorites sidebar

---

## Completion Criteria

- [ ] Can log into a Stoat account through Poly settings (native + WASM plugin)
- [ ] Can view joined Stoat servers and channels
- [ ] Can send and receive text messages in real-time (via WebSocket events)
- [ ] Can make voice calls to other Stoat users
- [ ] Can make video calls to other Stoat users
- [ ] Friend list, DMs, group DMs, notifications all work
- [ ] Self-hosted instances work with custom base URL
- [ ] WASM guest has parity with native for all core chat features

---

## Session Notes

### 2026-03-30
- Split from monolithic `phase-3-plan.md` into per-client files.
- Native `StoatClient` is substantially complete for REST-based chat (auth, channels, messages, send, users, friends, DMs, groups — 27 passing mock tests).
- WASM guest has real auth + DM/group mutations; everything else still stubbed.
- Next priorities in rough order:
  1. WebSocket real-time events (3.1.5) — needed for live messaging UX
  2. Guest parity for messages/channels/users (3.1.3) — needed for plugin path
  3. Reactions, edit/delete, attachment send (3.1.4)
  4. Server list discovery (3.1.2.4) — may unblock after Bonfire WS work
  5. Voice/video (3.1.6/3.1.7) — large, dedicated effort
