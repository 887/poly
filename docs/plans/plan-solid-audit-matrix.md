# Plan: SOLID + Missing-Impl Audit — `clients/matrix/`

> Author: orchestrator audit pass, 2026-05-17.
> Scope: `clients/matrix/src/`.
> Source-of-truth for SOLID definitions: top-of-repo `CLAUDE.md` §"Design Principles".

## Status: ✅ DONE — Phase B shipped (`nprtmlvu`); Phase C fully shipped (`tuzpozyt` / `0ca62644`+`4c9b2721`); Phase D.1+D.2 shipped on `worktree-agent-a6bdfdf6038b50eaf`; Phase D.3 shipped via rescue from `worktree-agent-ad241956221c6261f` (concurrent-worktree contamination prevented direct commit, but the work landed clean).

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
| `SocialGraphBackend::get_presence` | `lib.rs:1782` | FIXED in B.3, WIRED in D.2 | B.3 returned `NotSupported`; D.2 now performs `GET /_matrix/client/v3/presence/{userId}/status` and projects the response onto `PresenceStatus` (homeserver-disabled presence still collapses to `NotSupported`). |
| `SocialGraphBackend::set_presence` | `lib.rs:1789` | WIRED in D.2 | No longer a no-op — now `PUT /_matrix/client/v3/presence/{userId}/status` with `online`/`unavailable`/`offline` projection. |
| `MessagingBackend::send_typing` | `lib.rs:1955` | FIXED in B.1 | `warn!` → `debug!` flood-fix; endpoint still missing (see C.1). |
| `ServerAdminBackend::create_server` / `create_channel` / `mark_channel_read` | `lib.rs:2030..2053` | LOW | All return `NotSupported` with clean messages — LSP-clean. |
| `ModerationBackend::get_moderation_log` | `lib.rs:1655` | FIXED in D.1 | No longer `Ok(vec![])` — on-demand synthesiser in `moderation_log.rs` walks recent timeline events on each space-child room and projects `m.room.member` + `m.room.redaction` into entries (`has_moderation_log` in capabilities can now be flipped to `true` if/when desired). |

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
| `get_moderation_log` | `lib.rs:1655` | SHIPPED in D.1 — on-demand timeline-walk synthesiser, `moderation_log.rs` |
| `get_presence` / `set_presence` real wiring | `lib.rs:1782, 1789` | SHIPPED in D.2 — `GET/PUT /presence/{userId}/status` wired through MatrixHttpClient |

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

## Phase D — Architectural rewrites (>300 LoC, max 3) — D.1 + D.2 shipped

- [x] **D.1** Moderation-log synthesiser: walk recent timeline events on every
  child room of the space and project `m.room.member` + `m.room.redaction`
  into `ModerationLogEntry` rows. Shipped as the minimal on-demand variant
  (no background indexer / persistence) — `get_moderation_log` now
  synthesises the log per call by enumerating space children, fetching the
  last ~50 timeline events backwards from each child via
  `/messages?dir=b&limit=50`, then projecting + merging + sorting newest-
  first. Self-joins/invites/self-leaves filter out; `leave` with prior
  `join` = `MemberKicked`, `leave` with prior `ban` = `MemberUnbanned`,
  `ban` = `MemberBanned`, redactions = `MessageDeleted`. Lives in new
  `clients/matrix/src/moderation_log.rs` (~290 LoC incl. tests). The
  full "background indexer + log feed subscription" design (~600 LoC)
  remains the eventual target when call frequency makes the per-call I/O
  budget worth amortising. — shipped in worktree-agent-a6bdfdf6038b50eaf.
- [x] **D.2** Federation-aware presence: wired
  `GET /_matrix/client/v3/presence/{userId}/status` and
  `PUT /_matrix/client/v3/presence/{userId}/status` through
  `MatrixHttpClient::get_presence` / `put_presence`; mapped Matrix's
  three-valued `online`/`unavailable`/`offline` surface onto the host
  `PresenceStatus` enum (with `currently_active=false` → `Idle` to
  capture the away-from-keyboard case Matrix hides behind `online`).
  Homeserver-disabled-presence (404/403) collapses to `NotSupported`
  rather than `Offline` so the UI hides the dot. `set_presence` collapses
  `DoNotDisturb`/`Idle` onto `unavailable` and `Invisible`/`Offline` onto
  `offline`; `Unknown` is dropped silently. Sync sub-stream feeding
  `ClientEvent::PresenceChanged` is left for a follow-up — the current
  surface satisfies the fetch+set parts of D.2. ~120 LoC across api.rs,
  http.rs, lib.rs. — shipped in worktree-agent-a6bdfdf6038b50eaf.
- [x] **D.3** Split `MatrixClient::IsBackend` (833 lines) along capability-trait
  lines, matching the existing `ModerationBackend` / `SocialGraphBackend` /
  `DmsAndGroupsBackend` / `MessagingBackend` / `ServerAdminBackend` split.
  ~500 LoC. — shipped (rescued from worktree-agent-ad241956221c6261f after worktree contamination prevented direct commit): `lib.rs` 2604→818 LoC (-68%); 9 sibling files (`is_backend.rs`, `moderation.rs`, `social_graph.rs`, `dms_groups.rs`, `messaging.rs`, `server_admin.rs`, `settings.rs`, `view_descriptor.rs`, `context_action.rs`). All 27 unit tests pass. Pure structural move.
