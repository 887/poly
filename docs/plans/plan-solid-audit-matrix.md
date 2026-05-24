# Plan: SOLID + Missing-Impl Audit — `clients/matrix/`

> Author: orchestrator audit pass, 2026-05-17.
> Scope: `clients/matrix/src/`.
> Source-of-truth for SOLID definitions: top-of-repo `CLAUDE.md` §"Design Principles".

## Status: IN PROGRESS — Phase B shipped in change `nprtmlvu`; Phase C fully shipped in change `tuzpozyt` / commits `0ca62644` + `4c9b2721` (C.2 search, C.3 pinned, C.4 createRoom). Phase D queued.

---

## Phase A — Audit findings

- [x] **A.1** Walk every `impl` block and classify findings — done in change `nprtmlvu`.
- [x] **A.2** Grep stubs and pair with disposition — done.

### A.1 SRP — single-responsibility violations

| Site | File:Line | Severity | Note |
|------|-----------|----------|------|
| `MatrixClient::IsBackend` impl | `lib.rs:628..1461` | HIGH | 833-line impl mixes sync/event-stream, server list, channel list, members, settings, sidebar, view-rows, context-menu. Same shape as Stoat — split along existing capability traits. |
| `MatrixHttpClient` | `http.rs:1..1025` | MEDIUM | 1025-line god-struct: auth, sync, rooms, members, messages, account-data, push-rules, ignored-users. Splittable along auth / sync / rooms / messages / social. |
| `build_sidebar_items` + `SpaceTreeEntry` | `lib.rs:423..490` | LOW | Free function operating on `Vec<SpaceTreeEntry>` — SRP-clean but lives in `lib.rs`; would be cleaner in `sidebar.rs`. |
| `MatrixClient` `impl` blocks at `lib.rs:142`, `:491`, `:605` | `lib.rs` | LOW | Three separate `impl MatrixClient` blocks before the trait impls. Either consolidate or split into modules (`messages_mapper.rs`, `rooms.rs`). |

### A.2 OCP

- `invoke_context_action` central match — same shape as Stoat. Same disposition: acceptable.

### A.3 LSP — contract violations

| Site | File:Line | Severity | Note |
|------|-----------|----------|------|
| `SocialGraphBackend::get_friends` | `lib.rs:1701` | FIXED in B.2 | Was `Ok(vec![])` ("you have no friends"); now returns `NotSupported` ("Matrix has no friend concept") so callers can disambiguate empty-list vs not-supported. |
| `SocialGraphBackend::get_presence` | `lib.rs:1782` | FIXED in B.3 | Was always `Ok(Offline)` — lied to callers. Now `NotSupported`; presence-dot UI can hide. |
| `SocialGraphBackend::set_presence` | `lib.rs:1789` | DOCUMENTED in B.3 | No-op `Ok(())` acceptable per trait contract; comment now explains why. |
| `MessagingBackend::send_typing` | `lib.rs:1955` | FIXED in B.1 | `warn!` → `debug!` flood-fix; endpoint still missing (see C.1). |
| `ServerAdminBackend::create_server` / `create_channel` / `mark_channel_read` | `lib.rs:2030..2053` | LOW | All return `NotSupported` with clean messages — LSP-clean. |
| `ModerationBackend::get_moderation_log` | `lib.rs:1655` | LOW | Returns `Ok(vec![])` not `NotSupported`. Documented as "synthesise from m.room events" — pragmatically acceptable: `has_moderation_log = false` in capabilities hides the UI tab so callers don't see the empty list. |

### A.4 ISP — kitchen-sink

- Same shape as Stoat; capability-trait split already exists.

### A.5 DIP

- Concrete `MatrixHttpClient` ownership is acceptable.

### A.6 Missing-impl inventory

| Trait method | File:Line | Disposition |
|--------------|-----------|-------------|
| `send_typing` | `lib.rs:1955` | NEEDS_IMPL — `PUT /_matrix/client/v3/rooms/{roomId}/typing/{userId}` (C.1) |
| `search_messages` | `lib.rs:1992` | NEEDS_IMPL — `POST /_matrix/client/v3/search` (C.2) |
| `get_pinned_messages` / `set_message_pinned` | `lib.rs:1999, 2003` | NEEDS_IMPL — `m.room.pinned_events` state event (C.3) |
| `get_friends` and friend ops | `lib.rs:1701..1734` | INTENTIONAL — Matrix has no friend concept |
| `set_friend_nickname` / `set_user_note` | `lib.rs:1725, 1736` | INTENTIONAL — could be backed by account_data, deferred |
| `create_server` / `create_channel` | `lib.rs:2030, 2034` | NEEDS_IMPL — `POST /_matrix/client/v3/createRoom` (C.4) |
| `mark_channel_read` | `lib.rs:2051` | NEEDS_IMPL — `POST /_matrix/client/v3/rooms/{roomId}/read_markers` (C.5) |
| `update_server_banner` | `lib.rs:2043` | DOC_ONLY — Matrix has no banner concept |
| `get_moderation_log` | `lib.rs:1655` | DEFERRED — see D.1 (event-walk synthesis) |
| `get_presence` / `set_presence` real wiring | `lib.rs:1782, 1789` | DEFERRED — see D.2 (federation-aware presence) |

---

## Phase B — Ship-now wins (≤50 LoC each, max 3) — shipped in change `nprtmlvu`

- [x] **B.1** Demote `send_typing` `warn!` → `debug!`; document the missing
  `put_room_typing` http path inline (`lib.rs:1955`, ~10 LoC).
- [x] **B.2** Fix LSP violation in `get_friends`: return `NotSupported`
  instead of `Ok(vec![])` (`lib.rs:1701`, ~10 LoC).
- [x] **B.3** Fix LSP violation in `get_presence`: return `NotSupported`
  instead of `Ok(Offline)`; document `set_presence` no-op acceptable-by-contract
  (`lib.rs:1782..1789`, ~15 LoC).

---

## Phase C — Medium refactors (50-300 LoC, max 5) — C.1 + C.2 + C.3 + C.4 + C.5 shipped

- [x] **C.1** Wire `send_typing` to `PUT /_matrix/client/v3/rooms/{roomId}/typing/{userId}`
  with a 4-second `timeout` on the body `{ typing: true, timeout: 4000 }`. Added
  `put_room_typing` to `http.rs` and wired `send_typing` in `lib.rs` (reads user_id
  from session, best-effort errors). ~60 LoC.
- [x] **C.2** Implement `search_messages` via `POST /_matrix/client/v3/search`.
  Search categories: `room_events`. API types `SearchRequest/Response/Categories/Filter`
  added to `api.rs`; `post_search` in `http.rs`; `search_messages` in `lib.rs` maps
  `SearchResult` → `MessageSearchHit`. ~150 LoC. — shipped in change `c489a4619898`
- [x] **C.3** Implement `get_pinned_messages` / `set_message_pinned` via
  `m.room.pinned_events` state event. Added `PinnedEventsContent` to `api.rs`;
  `get_room_pinned_event_ids`, `get_room_event`, `put_room_pinned_events` in `http.rs`;
  both methods wired in `lib.rs` (read-modify-write for set_message_pinned,
  parallel event fetch + hydration for get_pinned_messages). ~150 LoC.
  — shipped in change `c489a4619898`
- [x] **C.4** Implement `create_server` (`POST /createRoom` with `preset: public_chat`
  + `room_type: m.space`) and `create_channel` (room with `m.space.parent` initial state
  + best-effort `m.space.child` write). Added `CreateRoomRequest/Response/InitialStateEvent`
  to `api.rs`; `create_room` + `put_space_child` in `http.rs`; both methods wired in
  `lib.rs`. ~220 LoC. — shipped in change `c489a4619898`
- [x] **C.5** Implement `mark_channel_read` via `POST /rooms/{id}/read_markers`.
  Added `post_read_markers` to `http.rs`; `mark_channel_read` in `lib.rs` fetches
  the latest event ID via `/messages?limit=1` then advances the marker. ~50 LoC.

## Phase D — Architectural rewrites (>300 LoC, max 3)

- [ ] **D.1** Moderation-log synthesiser: walk `/sync` events for `m.room.member`
  + `m.room.redaction` and project into `ModerationLogEntry` rows. Requires a
  background indexer task + persistence; the existing one-shot
  `get_moderation_log` stays NotSupported and a new "log feed" subscription
  emits entries. ~600 LoC.
- [ ] **D.2** Federation-aware presence: wire `GET /presence/{userId}/status` +
  `PUT /presence/{userId}/status`, plus the presence sub-stream in `/sync`
  feeding `ClientEvent::PresenceChanged`. ~400 LoC.
- [ ] **D.3** Split `MatrixClient::IsBackend` (833 lines) along capability-trait
  lines, matching the existing `ModerationBackend` / `SocialGraphBackend` /
  `DmsAndGroupsBackend` / `MessagingBackend` / `ServerAdminBackend` split.
  ~500 LoC.
