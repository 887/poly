# Plan: SOLID + Missing-Impl Audit — `clients/stoat/`

> Author: orchestrator audit pass, 2026-05-17.
> Scope: `clients/stoat/src/` (excluding `voice_wasm*.rs` + `voice_common.rs` —
> just landed, hands-off).
> Source-of-truth for SOLID definitions: top-of-repo `CLAUDE.md` §"Design Principles".

## Status: IN PROGRESS — Phase B shipped in change `nprtmlvu`

Phase A (audit) complete. Phase B (ship-now wins) shipped. Phases C/D (medium /
architectural) recorded for future passes.

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

## Phase C — Medium refactors (50-300 LoC, max 5)

- [ ] **C.1** Wire `send_typing` to Bonfire WS write path. Requires exposing a
  WS-send handle on `StoatClient` (channel-side `ChannelStartTyping` /
  `ChannelStopTyping` JSON frames). ~100 LoC across `lib.rs` + a new
  `bonfire_ws.rs`.
- [ ] **C.2** Wire `search_messages` via Revolt `POST /channels/{id}/search`
  + `MessageSearchHit` mapping. ~150 LoC across `http.rs` + `api.rs` + `lib.rs`.
- [ ] **C.3** Wire `get_server_roles` via Revolt server config role table.
  ~80 LoC.
- [ ] **C.4** Implement `invite_user_to_server` via `POST /servers/{id}/invites`.
  ~60 LoC.
- [ ] **C.5** Wire `mute_conversation` / `unmute_conversation` once the
  notification-override schema is confirmed across instances. ~120 LoC.

## Phase D — Architectural rewrites (>300 LoC, max 3)

- [ ] **D.1** Bonfire WebSocket event parser for WIT-guest (`guest.rs:585`).
  WASM plugin currently no-ops `handle_ws_data`; native already parses via
  `parse_bonfire_event`. Extract the parser into a shared module callable from
  both. ~400 LoC.
- [ ] **D.2** Split `StoatClient::IsBackend` (943 lines) along the same
  capability-trait lines the rest of the codebase uses
  (`ModerationBackend` / `SocialGraphBackend` / `DmsAndGroupsBackend` /
  `MessagingBackend`). Re-export from a thin `IsBackend` facade. ~600 LoC.
- [ ] **D.3** Split `StoatHttpClient` (1148 lines) by domain — auth, channels,
  messages, social, moderation. ~500 LoC.
