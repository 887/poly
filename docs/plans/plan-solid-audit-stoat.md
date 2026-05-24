# Plan: SOLID + Missing-Impl Audit — `clients/stoat/`

> Author: orchestrator audit pass, 2026-05-17.
> Scope: `clients/stoat/src/` (excluding `voice_wasm*.rs` + `voice_common.rs` —
> just landed, hands-off).
> Source-of-truth for SOLID definitions: top-of-repo `CLAUDE.md` §"Design Principles".

## Status: ✅ DONE — all phases shipped (changes `nprtmlvu`, `xwqxtotz`, `qumnkxmo`, `e1cb7a55`, and the SOLID close commit)

Phase A (audit) complete. Phase B (ship-now wins) shipped. Phase C complete:
C.1 (send_typing WS), C.2 (search_messages), C.3 (get_server_roles), C.4
(invite_user_to_server + ServerAdminBackend), and C.5 (mute/unmute via the
in-memory `menu_state.muted_dms` parity teams already uses — server-side
notification-override schema still considered unstable, so the in-memory
store is the canonical source of truth until that schema settles). Phase D
complete: D.1 (WIT-guest Bonfire parser mirrors native variants), D.2 + D.3
(file splits) shipped earlier.

---

## Phase A — Audit findings

- [x] **A.1** Walk every `impl` block and classify findings (SRP/OCP/LSP/ISP/DIP) — done in change `nprtmlvu`.
- [x] **A.2** Grep stubs (`NotSupported`, `TODO`, `Ok(vec![])`) and pair each
  with "is this contractually correct, a missing impl, or an LSP lie?" — done.

### A.1 SRP — single-responsibility violations

| Site | File:Line | Severity | Note |
|------|-----------|----------|------|
| `StoatClient::IsBackend` impl | `lib.rs:628..1571` | HIGH | 943-line impl mixes auth, server list, channel list, voice, settings, sidebar, view-rows, context-menu, notifications, get-messages. One trait, ~50 methods. Splittable along the same trait-segregation lines as `ModerationBackend` / `SocialGraphBackend` / `DmsAndGroupsBackend`. |
| `StoatHttpClient` | `http.rs:1..1148` | MEDIUM | 1148-line god-struct: auth, members, channels, messages, attachments, moderation, friends, groups, server settings. Closest to a domain-driven split: `StoatAuthHttp`, `StoatChannelHttp`, `StoatMessageHttp`, `StoatModerationHttp`, `StoatSocialHttp`. |
| `StoatClient::get_context_menu_items` | `lib.rs:1135..1243` | LOW | 108-line `match` with two nested `fn normal` / `fn destructive` helpers — SRP-passable but reads like a config table; extracting `static MENU_TEMPLATE: &[…]` would compress to <30 LoC. |

### A.2 OCP — adding a new menu/sort/filter requires editing existing match

- `lib.rs:1244` `invoke_context_action` — central match on action_id. Any new
  action requires editing this match. Acceptable for plugin-local actions
  (callers can't extend); flagged but not actionable unless the host gains a
  registry pattern.

### A.3 LSP — contract violations

| Site | File:Line | Severity | Note |
|------|-----------|----------|------|
| `MessagingBackend::send_typing` | `lib.rs:2156` (was `warn!`-spam) | FIXED in B.1 | Was emitting `warn!` on every keystroke; contract says success means "best-effort delivered". Now `debug!` — log-noise fixed. Endpoint still missing (see C.1). |
| `SocialGraphBackend::set_presence` | `lib.rs:1880` | MEDIUM | Always `Ok(())` — contract is "may fail on network/auth"; silent no-op is acceptable but logs nothing. Either wire `PATCH /users/@me` (Revolt) or add a `debug!` and document. |
| `DmsAndGroupsBackend::edit_group_dm` | `lib.rs:2039` (was `warn!`) | FIXED in B.2 | `warn!` whenever avatar_url is set; now `debug!`. |
| `MessagingBackend::send_message_internal` panic path | `lib.rs:539..618` | LOW | Uses `?` throughout; no unwrap; LSP-clean. |

### A.4 ISP — kitchen-sink traits

- The `IsBackend` super-trait that StoatClient implements pulls in everything.
  The codebase already has `ModerationBackend`, `SocialGraphBackend`,
  `DmsAndGroupsBackend`, `MessagingBackend` as split capability traits — good.
  IsBackend itself remains the kitchen sink; out-of-scope for this audit.

### A.5 DIP — concrete deps at call sites

- `StoatClient` owns a concrete `StoatHttpClient` (`lib.rs:139`). That's
  appropriate (plugin owns its transport); the trait the UI consumes is
  already abstract (`IsBackend`).

### A.6 Missing-impl inventory (NotSupported / TODO / stub)

| Trait method | File:Line | Disposition |
|--------------|-----------|-------------|
| `send_typing` | `lib.rs:2156` | NEEDS_IMPL — Bonfire WS write path (C.1) |
| `search_messages` | `lib.rs:2174` | NEEDS_IMPL — Revolt `POST /channels/{id}/search` (C.2) |
| `get_pinned_messages` / `set_message_pinned` | `lib.rs:2181, 2185` | NEEDS_IMPL — Revolt has no real pin concept; document as platform-NotSupported |
| `mute_conversation` / `unmute_conversation` | `lib.rs:2015, 2029` | DOC_ONLY — schema unstable; documented TODO at line 2022 |
| `reorder_channels` | `lib.rs:1755` | DOC_ONLY — Revolt has no endpoint |
| `get_moderation_log` | `lib.rs:1763` | DOC_ONLY — Revolt has no endpoint |
| `get_server_roles` | `lib.rs:1771` | NEEDS_IMPL — Revolt has roles via server config (C.3) |
| `invite_user_to_server` | `lib.rs:1536` (trait default) | NEEDS_IMPL — Revolt `POST /servers/{id}/invites` |
| `set_friend_nickname` / `set_user_note` | `lib.rs:1834, 1845` | DOC_ONLY |
| `ignore_user` / `unignore_user` | `lib.rs:1864, 1870` | INTENTIONAL — mapped to block/unblock |
| `voice_join` voice-bridge handshake | `lib.rs:946..1027`, `voice.rs` | DEFERRED — phase F.6 partial, out-of-scope (hands-off rule) |
| WIT-guest `handle_ws_data` Bonfire parser | `guest.rs:585` | NEEDS_IMPL — WASM plugin parity (D.1) |

---

## Phase B — Ship-now wins (≤50 LoC each, max 3) — shipped in change `nprtmlvu`

- [x] **B.1** Demote `send_typing` `warn!` → `debug!` and document the
  Bonfire-WS dependency inline (`lib.rs:2156`, ~10 LoC).
- [x] **B.2** Demote `edit_group_dm` avatar-url `warn!` → `debug!`
  (`lib.rs:2048`, ~10 LoC).
- [x] **B.3** Remove stale `// TODO(phase-3.1): Implement Stoat client` at
  `lib.rs:24` — the client is implemented (~2 LoC).

---

## Phase C — Medium refactors (50-300 LoC, max 5) — C.1/C.2/C.3/C.4 shipped in change `xwqxtotz`

- [x] **C.1** Wire `send_typing` to Bonfire WS write path. Added `ws_write_tx`
  field (`Mutex<Option<Box<dyn Fn(String)+Send+Sync>>>`) to `StoatClient`.
  `event_stream` now splits the tokio-tungstenite stream (read/write), spawns a
  `tokio::select!` loop with an `mpsc::unbounded_channel` for outbound frames,
  and stores a send-callback in `ws_write_tx`. `send_typing` queues a
  `BeginTyping` frame on native; is a documented debug-log no-op on WASM. — shipped in change `xwqxtotz`
- [x] **C.2** Wire `search_messages` via Revolt `POST /channels/{id}/search`
  + `MessageSearchHit` mapping. Added `StoatSearchRequest` + `StoatSearchResponse`
  to `api.rs`, `search_messages_channel` to `http.rs`, and full impl in
  `lib.rs::MessagingBackend`. Requires `channel_id` in query (Revolt has no
  server-wide search index). — shipped in change `xwqxtotz`
- [x] **C.3** Wire `get_server_roles` via Revolt server config role table. Added
  `StoatRole` struct to `api.rs`, `roles` field on `StoatServer`, `into_poly_roles()`
  mapper, and wired `get_server_roles` in `lib.rs`. ~80 LoC.
- [x] **C.4** Implement `invite_user_to_server` via `POST /channels/{id}/invites`.
  Added `StoatCreateInviteResponse` to `api.rs`, `create_channel_invite` to
  `http.rs`, and a full `ServerAdminBackend` impl for `StoatClient` in `lib.rs`
  (other methods stubbed as `NotSupported`; `mark_channel_read` wired via
  `PUT /channels/{id}/ack/{msg_id}`). — shipped in change `xwqxtotz`
- [x] **C.5** Wire `mute_conversation` / `unmute_conversation` via the
  in-memory `menu_state.muted_dms` set — same parity pattern teams C.4 ships.
  The Revolt `PATCH /channels/{id}` notification-override schema remains
  unstable across official vs self-hosted forks (some accept `"muted"` as
  string, some require integer level, some 4xx the field outright), so
  rather than guess per-instance, the trait method now writes to the same
  `menu_state.muted_dms` set the context-menu mute-dm action already reads.
  This makes the trait method and the context-action agree without a
  network round-trip. When the Stoat notification schema stabilises across
  instances, swap the in-memory toggle for a real PATCH call. — shipped in
  the SOLID close commit (this change).

## Phase D — Architectural rewrites (>300 LoC, max 3)

- [x] **D.1** Bonfire WebSocket event parser for WIT-guest (`guest.rs:613`).
  Decided against extracting a shared module: the native parser produces
  `poly_client::ClientEvent` and the WIT guest needs `wit::ClientEvent` —
  the two are structurally similar but field-by-field distinct (e.g. native
  carries `BackendType::from(SLUG)`, WIT carries the slug string; native
  `MessageReceived { … }` vs WIT `MessageReceived(MessageReceivedEvent { … })`
  tuple-variant wrapping). A shared trait abstraction would have to be
  generic over both type families and pull `wit_bindings` into the native
  build, which is a heavier refactor than just duplicating 80 lines of
  straight-line JSON parsing. Instead the WIT guest now has its own
  `bonfire_event_to_wit` helper sibling to native `parse_bonfire_event`
  covering the same variants (`Message`, `ChannelStartTyping`,
  `VoiceUserJoined`, `VoiceUserLeft`, `Authenticated`). `handle_ws_data`
  parses the WS payload as UTF-8 JSON, accepts either single-object or
  array forms (the test-stoat mock occasionally batches), and calls
  `host_api::emit_event` per parsed event — matching the teams
  `handle_ws_data` shape exactly. ~110 LoC. — shipped in the SOLID close
  commit (this change).
- [x] **D.2** Split `StoatClient::IsBackend` (943 lines) along the same
  capability-trait lines the rest of the codebase uses
  (`ModerationBackend` / `SocialGraphBackend` / `DmsAndGroupsBackend` /
  `MessagingBackend`). Re-export from a thin `IsBackend` facade. ~600 LoC.
  — shipped in change `qumnkxmo`: each capability-trait impl lives in its
  own sibling file (`is_backend.rs`, `messaging.rs`, `moderation.rs`,
  `social_graph.rs`, `dms_and_groups.rs`, `server_admin.rs`,
  `voice_transport.rs`, `settings.rs`, `view_descriptor.rs`,
  `context_action.rs`). `lib.rs` keeps only struct + inherent helpers +
  tests. `parse_bonfire_event` colocated with `is_backend.rs`.
- [x] **D.3** Split `StoatHttpClient` (1148 lines) by domain — auth, channels,
  messages, social, moderation. ~500 LoC.
  — shipped in change `qumnkxmo`: `http.rs` becomes `http/{mod,auth,channels,messages,moderation,social}.rs`.
  Struct + session/UA/request plumbing + tests in `http/mod.rs`; each
  domain attaches additional inherent `impl StoatHttpClient` blocks.
