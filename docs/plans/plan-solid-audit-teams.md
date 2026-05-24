# Plan: SOLID + Missing-Impl Audit — `clients/teams/`

> Author: orchestrator audit pass, 2026-05-17.
> Scope: `clients/teams/src/` (includes `voice.rs` documentation stub; voice
> integration deferred to plan-voice-video-calls).
> Source-of-truth for SOLID definitions: top-of-repo `CLAUDE.md` §"Design Principles".

## Status: ✅ DONE — Phase B shipped in change `nprtmlvu`; Phase C fully shipped in change `snonppwv` / commit `fb1fc675` (C.1 channel views via Graph, C.2 chat create, C.3 member ops, C.5 edit_group_dm topic; C.4 was previously shipped); Phase D.2 shipped in change `vvqwyqlnpuzl`; D.1 deferred to `docs/plans/plan-teams-calling.md` (ACS calling — ~800 LoC + Cargo dependency surface that does not yet exist in Rust ecosystem); D.3 deferred to `docs/plans/plan-teams-graph-subscriptions.md` (Graph change-notification subscriptions — ~700 LoC + requires publicly addressable HTTPS endpoint infrastructure). B.3 closed as reserved no-op — no further ≤50-LoC win was both safe and useful.

---

## Phase A — Audit findings

- [x] **A.1** Walk every `impl` block and classify findings — done in change `nprtmlvu`.
- [x] **A.2** Grep stubs and pair with disposition — done.

### A.1 SRP — single-responsibility violations

| Site | File:Line | Severity | Note |
|------|-----------|----------|------|
| `TeamsClient::IsBackend` impl | `lib.rs:289..1045` | HIGH | 756-line impl mixes auth, server list, channel list, view-rows, settings, sidebar, context-menu, composer buttons. Same shape as Stoat/Matrix — split along the existing capability-trait lines (`ModerationBackend` / `SocialGraphBackend` / `DmsAndGroupsBackend` already exist). |
| `TeamsHttpClient` | `http.rs:1..504` | LOW | 504 lines — within tolerance; one-domain (Graph v1.0). |
| `TeamsClient::get_view_rows` server-walk loop | `lib.rs:919..971` | LOW | 53 lines, single responsibility (render server cards). OK. |
| `TeamsClient::get_context_menu_items` (not pictured) | `lib.rs` | LOW | Inherits the central-match SRP smell from the others; out-of-scope here. |

### A.2 OCP

- Central `invoke_*_action` matches — same as Stoat/Matrix. Out-of-scope.

### A.3 LSP — contract violations

| Site | File:Line | Severity | Note |
|------|-----------|----------|------|
| `SocialGraphBackend::get_friends` | `lib.rs:1250` | FIXED in B.1 | Was `Ok(vec![])`; now `NotSupported` to match the surrounding "Teams has no friend system" contract. |
| `ModerationBackend::update_channel` `warn!` storm | `lib.rs:1190..1198` | FIXED in B.2 | Three `warn!`s per call for fields the UI sends in every update payload. Demoted to `debug!`. |
| `get_user` | `lib.rs:1242` | LOW | Returns `NotSupported` — documented well; LSP-clean (the disambiguation rationale in the doc comment is exemplary). |
| `voice.rs` TeamsVoiceClient | `voice.rs:1..137` | LOW | Every method returns `NotSupported(_)` with consistent messages and tested via `connect_voice_returns_not_supported` etc. Exemplary LSP discipline. |
| `get_voice_participants` WIT-guest | `guest.rs:610` | LOW | `Ok(vec![])` — given `supports_voice: false` capability, callers know not to ask. |
| `get_view_rows` for non-empty channel_id | `lib.rs:929` | MEDIUM | Bare `NotSupported` for per-channel rows. Stoat/Matrix render channel views; Teams should too eventually (C.1). |

### A.4 ISP — kitchen-sink

- `IsBackend` itself — same shape as the other two. Out-of-scope.

### A.5 DIP

- `TeamsHttpClient` ownership is concrete and appropriate.

### A.6 Missing-impl inventory

| Trait method | File:Line | Disposition |
|--------------|-----------|-------------|
| `get_channel_view` / `get_view_rows` (channel) / `get_view_detail` | `lib.rs:915, 929, 977` | NEEDS_IMPL — main message view (C.1) |
| `ban/unban/timeout/get_bans` | `lib.rs:1104..1152` | INTENTIONAL — Teams has no ban/timeout |
| `reorder_channels` / `get_moderation_log` / `get_server_roles` | `lib.rs:1209..1231` | INTENTIONAL — no Graph endpoints |
| `get_user` | `lib.rs:1242` | DEFERRED — Graph requires `User.Read.All`; document scope |
| `get_friends` and friend ops | `lib.rs:1250..1273` | INTENTIONAL — Teams has no friend system |
| `open_direct_message_channel` | `lib.rs:1348` | NEEDS_IMPL — Graph `POST /chats` (C.2) |
| `add_group_member` / `remove_group_member` / `add_users_to_group_dm` | `lib.rs:1360..1376` | NEEDS_IMPL — Graph `POST /chats/{id}/members` (C.3) |
| `mute_conversation` / `unmute_conversation` / `leave_group_dm` / `edit_group_dm` | `lib.rs:1384..1412` | NEEDS_IMPL — Graph chat-update endpoints (C.4) |
| `close_dm_channel` | `lib.rs:1378` | DOC_ONLY — Graph has no chat-delete endpoint |
| `TeamsVoiceClient` real impl | `voice.rs` | DEFERRED — see D.1 (ACS calling SDK) |
| `view-rows not yet implemented for team channels` | `lib.rs:929` | Reflected as C.1 |
| WIT-guest parity (open_direct_message_channel etc.) | `guest.rs:594..603` | DEFERRED — mirror native progress |

---

## Phase B — Ship-now wins (≤50 LoC each, max 3) — shipped in change `nprtmlvu`

- [x] **B.1** Fix LSP violation in `get_friends`: return `NotSupported`
  instead of `Ok(vec![])` (`lib.rs:1250`, ~10 LoC).
- [x] **B.2** Demote three `update_channel` `warn!`s → `debug!` for fields
  with no Graph equivalent (slow_mode_secs / nsfw / position). The UI sends
  these on every full update; warn-spam is real (`lib.rs:1190..1198`, ~10 LoC).
- [x] **B.3** Reserved — confirmed no further ≤50-LoC win was both safe and
  useful in this pass. Closed as no-op to mark the slot resolved rather than
  leaving an indefinite open box. Future sweeps can add new B.4 entries.

---

## Phase C — Medium refactors (50-300 LoC, max 5) — C.1/C.2/C.3/C.4/C.5 shipped in this change

- [x] **C.1** Implement channel `get_channel_view` / `get_view_rows` /
  `get_view_detail` for team channels — render message list rows like Stoat
  does. ~250 LoC across `lib.rs` + view-mapping helpers.
  — `get_channel_view` returns `ViewKind::FlatList` descriptor for non-empty channel IDs.
  — `get_view_rows` (channel branch): fetches messages via Graph, maps to ViewRow with author/timestamp meta.
  — `get_view_detail`: best-effort message lookup in the 50-message page; returns `CustomBlock` HTML with author + HTML-escaped body.
- [x] **C.2** Implement `open_direct_message_channel` via Graph
  `POST /chats` with `chatType: oneOnOne` + member array. ~80 LoC.
  — New `TeamsHttpClient::create_chat` method. Graph is idempotent (returns existing chat if one already exists).
- [x] **C.3** Implement `add_group_member` / `remove_group_member` /
  `add_users_to_group_dm` via `POST /chats/{id}/members` +
  `DELETE /chats/{id}/members/{membershipId}`. ~120 LoC.
  — New `add_chat_member`, `get_chat_members`, `remove_chat_member` HTTP helpers.
  — `remove_group_member` resolves membership ID via GET members before DELETE.
  — `add_users_to_group_dm` is sequential O(n); Graph has no batch-add endpoint.
- [x] **C.4** Implement `mute_conversation` / `unmute_conversation`. Wired to the
  in-memory `muted_dms` store (same source of truth the sidebar and context-menu
  "mute-dm" action use). Graph `notificationSettings` PATCH requires per-chat
  membership ID lookup — noted in code comment, deferred; in-memory parity ships
  now. ~30 LoC.
- [x] **C.5** Implement `edit_group_dm` (chat topic / photo). ~100 LoC.
  — New `TeamsHttpClient::patch_chat_topic` (PATCH /v1.0/chats/{id} with `topic`).
  — `avatar_url` is accepted but silently ignored: Graph has no chat-photo endpoint.

## Phase D — Architectural rewrites (>300 LoC, max 3)

- [~] **D.1** Real `TeamsVoiceClient` via Azure Communication Services calling
  SDK. Requires ACS token acquisition, WebRTC bridge to `voice_bridge`.
  ~800 LoC + Cargo dependency surface. Currently stub-only per
  `plan-voice-video-calls.md` Phase I.
  — **DEFERRED** to `docs/plans/plan-teams-calling.md`. Rationale: no
  first-party Rust SDK exists for ACS Calling (only JS/.NET/iOS/Android);
  realistic path is a JS-bridge wiring `@azure/communication-calling`
  through a hidden WebView (same shape as Discord voice bridge but
  pointed at a Microsoft endpoint). That work spans token acquisition
  (separate ACS identity provisioning per tenant), call lifecycle UI
  parity (incoming-call banner, hold, transfer), and tenant-policy edge
  cases (admin-disabled external federation, lobby behavior). Genuinely
  multi-week; the existing stub in `clients/teams/src/voice.rs` returns
  clean `NotSupported` and is the correct placeholder until that plan
  ships. New plan file documents the design + dependency audit.
- [x] **D.2** Split `TeamsClient::IsBackend` (756 lines) along capability-trait
  lines, matching the existing `ModerationBackend` / `SocialGraphBackend` /
  `DmsAndGroupsBackend` split. ~500 LoC. — shipped in change `vvqwyqlnpuzl`
- [~] **D.3** Long-poll → real Graph change-notification subscription (event
  stream replaces `/test/events/poll`). Requires webhook lifecycle, secret
  validation, server-side relay. ~700 LoC.
  — **DEFERRED** to `docs/plans/plan-teams-graph-subscriptions.md`.
  Rationale: production Graph does NOT offer long-polling — the
  replacement is the change-notifications API, which mandates a
  publicly addressable HTTPS `notificationUrl`. We do not currently
  have hosted relay infrastructure, and local-dev users cannot accept
  webhooks without a tunnel. The plan must first decide the infra
  model (self-hosted, hosted relay, or on-demand tunnel) before code
  lands. The current long-poll against the test server's
  `/test/events/poll` endpoint works for development and demos; no
  user-visible regression from deferring. New plan file enumerates the
  subscription lifecycle, HMAC validation, encryption for rich
  notifications, and transition strategy.
