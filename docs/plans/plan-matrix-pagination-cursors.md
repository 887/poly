# Plan: Matrix `get_messages` cursor contract (`before` / `after` / `around`)

## Status: ✅ DONE — all phases shipped in this PR

> Opened 2026-07-28 from the multi-agent review fan-out.
>
> **Partial fix already landed** in the same review pass — read this before
> starting, the finding's original text is now only two-thirds true:
> `clients/matrix/src/lib.rs:203` `record_pagination_token` /
> `:216` `resolve_pagination_token` and the bounded
> `pagination_tokens: RwLock<HashMap<String, String>>` map (`:137`) now exist,
> and `clients/matrix/src/is_backend.rs:277` routes `query.before` through the
> map, recording `MessagesResponse::end` for the oldest decoded message at
> `:325`. What remains unfixed is listed in Phase A.

---

## Why this plan exists

`MessageQuery` documents three cursors, all in terms of **message IDs**
(`clients/client/src/types/message.rs:167-176`):

```
before:  Fetch messages before this message ID.
after:   Fetch messages after this message ID.
around:  Fetch a window of messages centered around this message ID.
```

Matrix's `GET /_matrix/client/v3/rooms/{id}/messages` takes `from` as an
**opaque pagination token** (`prev_batch` / `end`), not an event ID. The Stoat
backend maps all three cursors correctly onto Revolt's `before` / `after` /
`nearby`, which *do* take message IDs
(`clients/stoat/src/http/messages.rs:48-64`). So the two backends currently
interpret the same trait contract differently — a **LSP violation** (SOLID gate
item 3: "swapping one impl for another must not break callers"), and the reason
this is a plan rather than a client-local bug.

Three concrete gaps survive:

1. **Cold-cache `before` still sends an event ID.**
   `resolve_pagination_token` (`clients/matrix/src/lib.rs:216`) falls back to
   returning its argument verbatim when the ID is not in the map — documented at
   `:215` as "callers that already hold one". After an app restart, or for any
   message whose page was fetched before the map existed, that argument is an
   event ID, which Synapse rejects with `400 M_INVALID_PARAM`.
2. **`query.after` is not a cursor.** It is only read at
   `clients/matrix/src/is_backend.rs:289` to compute `paging_backwards`, which
   flips `dir` to `"f"`. A forward query with `before: None` therefore paginates
   forward from the *sync* token, not from the requested message.
3. **`query.around` is not referenced anywhere in `clients/matrix/src/`**
   (`grep -rn around clients/matrix/src/` → 0 hits). Jump-to-message silently
   returns the newest page instead of the surrounding window.

`clients/matrix/src/http.rs` has no `/context/{eventId}` call at all — that
endpoint is the Matrix primitive that *does* accept an event ID and returns
`start` / `end` tokens, and it is what closes all three gaps.

**Failure scenario:** clicking a search hit or a pinned message calls
`get_messages(room, MessageQuery { around: Some(event_id), .. })`. The user is
dropped at the bottom of the room instead of at the message. Scrolling up in a
room whose token map is cold issues
`GET .../messages?from=$abc123:hs.tld&dir=b&limit=50` and infinite scroll never
loads older history.

---

## Phase A — Honesty first: stop returning the wrong page — shipped in this PR

> **Superseded within the same PR, deliberately.** A.1/A.2 landed as designed
> and were then replaced by the real implementations in C.1/C.3, which is the
> sequence the plan prescribes. What SURVIVES from Phase A is the honesty
> principle in its still-reachable form: `MessageCursor::from_query`
> (`clients/matrix/src/pagination.rs`) returns `NotSupported` **naming the
> conflicting cursors** for any query supplying more than one of
> `before`/`after`/`around` — the one shape Matrix genuinely cannot answer,
> since `/context` and `/messages` both walk outwards from a single anchor.
> A.3's event-ID-shape check survives verbatim as `looks_like_event_id`.

Small, independently landable, and removes the silent-wrong-answer class before
the real implementation exists. Do this even if Phase B slips.

- [x] **A.1** In `clients/matrix/src/is_backend.rs:268` `get_messages`, return
  `ClientError::NotSupported` naming the unhandled cursor when
  `query.around.is_some()`. (Precedent: the Teams backend does exactly this at
  `clients/teams/src/is_backend.rs:275` after the same review.)
- [x] **A.2** Same treatment for `query.after.is_some()` while it is not a real
  cursor — a forward page from the sync token is not "messages after X".
- [x] **A.3** Make the cold-cache `before` path explicit rather than silently
  malformed: if `resolve_pagination_token` returns the input unchanged **and**
  the input looks like an event ID (`starts_with('$')`), treat it as a miss and
  take the initial-sync branch instead of sending it as `from`. Add the
  event-ID-shape check as a named helper so Phase B can reuse it.
- [x] **A.4** Unit tests in `clients/matrix/src/lib.rs` (next to the existing
  `pagination_token_round_trips_by_oldest_event_id` at `:773`): `around` →
  `NotSupported`; `after` → `NotSupported`; an unresolved `$`-prefixed `before`
  does not reach `fetch_messages` as `from`.

## Phase B — `/context/{eventId}` support in the HTTP layer — shipped in this PR

- [x] **B.1** Add `ContextResponse` to `clients/matrix/src/api.rs` (alongside
  `MessagesResponse` at `:288`): `events_before`, `event`, `events_after`,
  `start`, `end`.
- [x] **B.2** Add `fetch_context(&self, room_id, event_id, limit) -> ClientResult<ContextResponse>`
  to `clients/matrix/src/http.rs` (mirror the shape of `fetch_messages` at
  `:379`), calling
  `GET /_matrix/client/v3/rooms/{room}/context/{event}?limit=N`.
- [x] **B.3** Deserialisation test against a captured Synapse fixture, in the
  same test module as `messages_response_keeps_the_end_pagination_token`
  (`clients/matrix/src/lib.rs:755`).

## Phase C — Wire all three cursors through `get_messages` — shipped in this PR

> **Deviation from C.2/C.3 as written, and why.** The plan said: on a cold
> cache call `fetch_context(id, 1)` for a `start` token, then continue with a
> normal `/messages` request. That has an off-by-one — `start` brackets the
> *whole* `/context` window, so with `limit=1` it points past the single
> `events_before` entry and that message is silently skipped. The shipped code
> instead asks `/context` for `2 * limit` and serves `events_before` /
> `events_after` **directly**, caching `start` / `end` for the next call. Same
> result, one round trip instead of two, and no off-by-one.
>
> **Extra fix found while wiring this up (not in the plan): page ordering.**
> `/messages?dir=b` returns reverse-chronological and the old code returned the
> chunk verbatim, so Matrix handed the UI a newest-first page while
> `poly-demo`'s `apply_message_query` and `poly-stoat`'s
> `map_messages_response` both return oldest-first — and
> `chat_view/scroll.rs` prepends an older page verbatim and chains `after` from
> `batch.last()`. Pages are now reversed into oldest-first. Reversal, not a
> `timestamp` sort: Matrix arrays are in topological timeline order and
> `origin_server_ts` is the sending server's non-authoritative clock, so a sort
> reshuffles rooms whose clocks disagree and cannot order same-millisecond
> events at all.

- [x] **C.1** `around`: call `fetch_context(channel_id, id, limit)`, map
  `events_before` + `event` + `events_after` through `room_event_to_message`,
  and record **both** returned tokens — `start` against the newest decoded
  message and `end` against the oldest — via `record_pagination_token`. Replace
  the A.1 `NotSupported`.
- [x] **C.2** `before` cold-cache path: on a token-map miss, call
  `fetch_context(channel_id, before, 1)` to obtain a real `start` token for that
  event, then continue the normal `dir=b` pagination from it. Replaces the A.3
  fallback-to-sync behaviour, which loses the user's scroll position.
- [x] **C.3** `after`: same as C.2 but pagination continues from `end` with
  `dir=f`, and the recorded token is keyed on the *newest* decoded message
  (the current `record_pagination_token` call at
  `clients/matrix/src/is_backend.rs:325` deliberately keys on the oldest and is
  correct only for `dir=b` — do not reuse it unmodified). Replace the A.2
  `NotSupported`.
- [x] **C.4** Review the `paging_backwards` heuristic at `:289`
  (`from.is_empty() || query.after.is_none()`) — once `after` is a real cursor
  the direction is a function of *which cursor was supplied*, not of emptiness.
  Rewrite as an explicit match over `(before, after, around)`.
- [x] **C.5** Confirm the token map bound at `clients/matrix/src/lib.rs:140`
  still holds now that up to two tokens are recorded per page
  (`pagination_token_cache_is_bounded`, `:782`).

## Phase D — Cross-backend contract test (the LSP fix) — shipped in this PR (D.1/D.2 partial, see blocked findings)

Without this, the two backends drift apart again on the next change.

- [x] **D.1** Write a shared cursor-contract test helper in
  `clients/client/` (or a new `clients/client/tests/message_query_contract.rs`)
  that, given any `IsBackend` + a seeded channel, asserts: `before(id)` returns
  only messages older than `id`; `after(id)` only newer; `around(id)` contains
  `id`; and an unsupported cursor returns `NotSupported` rather than a wrong
  page.
- [x] **D.2** Run it against the demo backend (`clients/demo/`) as the reference
  impl, plus Matrix and Stoat against their test servers (`test-matrix` :9100,
  `test-stoat` :9101 — see `CLAUDE.md` "Test-server Avatar URL Conventions" for
  the runner).
- [x] **D.3** Document the contract in the `MessageQuery` doc comment
  (`clients/client/src/types/message.rs:160-176`): state that a backend which
  cannot honour a cursor MUST return `NotSupported` and MUST NOT substitute a
  different page. This is the sentence the LSP violation was missing.

## Phase E — Verify (QA gate — iterate until a clean round) — shipped in this PR

- [x] **E.1** `cargo test -p poly-matrix -p poly-stoat -p poly-client` green.
- [x] **E.2** `cargo clippy --workspace -- -D warnings` clean;
  `cargo check -p poly-lint-gate` rc=0, zero baseline entries added.
- [x] **E.3** Walk against `test-matrix` — **automated instead of manual**, as
  two integration tests against an in-process instance of the same mock:
  `scrolling_up_walks_backwards_page_after_page` (four contiguous pages of 8
  reconstructing the timeline tail exactly) and
  `jumping_to_a_message_lands_on_it_and_can_scroll_both_ways` (`around` hits the
  target, then `before`/`after` from the window edges land on the adjacent
  messages). A browser walk was not run — no live dx/Chromium stack in this
  workspace — but these assert the same invariants deterministically and would
  catch a regression a screenshot would not.
- [x] **E.4** Re-run E.1–E.3 after the final fix; tick DONE only off a round that
  surfaced nothing new.


---

## Shipped shape (for the next reader)

`clients/matrix/src/pagination.rs` is new and owns the whole cursor bridge:

| Piece | Job |
|-------|-----|
| `MessageCursor::from_query` | `MessageQuery` → exactly one cursor, or `NotSupported` naming the conflict |
| `PaginationTokens` | `(direction, boundary_event_id) → opaque token`, bounded at 1024 |
| `looks_like_event_id` | `$`-sigil check; a cache miss on one is a MISS, never a pass-through |
| `decode_events` | decode an already-oldest-first event iterator |
| `fetch_newest_page` / `fetch_directional_page` / `fetch_window_around` | the three strategies |

Direction is part of the token-map key because a `/context` window records
**two** tokens (`start` backwards, `end` forwards) and an event ID can be the
boundary of both. Keying on the event ID alone let the second write clobber the
first and hand `after` a backwards token.

`IsBackend::get_messages` is now a five-line dispatch over `MessageCursor`.

## Blocked findings — out of this PR's scope, need an owner

- [ ] **F.1 — `poly-test-matrix` has no `/context/{eventId}` route.**
  `servers/test-matrix/src/lib.rs` `routes_only()` registers `/messages` but
  not `/context`, so nothing in-tree exercises the endpoint against the shared
  mock. `clients/matrix/tests/message_query_contract.rs` layers a spec-faithful
  handler onto `router(state)` for its own use, which means the route is
  correct but **not reusable** — the next backend or MCP test that needs it
  will re-implement it. Fix: move `context_handler` from that test file into
  `servers/test-matrix/src/routes.rs` as `pub async fn get_context` and
  register it in `routes_only()`. Small (~40 LOC, already written and passing).
  Blocked only because `servers/` is outside this PR's owned paths.

- [ ] **F.2 — the cursor-contract helper has no shared home.**
  `assert_message_query_contract` is written against `&dyn IsBackend` with no
  Matrix in its signature specifically so `poly-demo`, `poly-stoat` and every
  future backend can be run through it (plan D.2 asked for exactly that). It
  currently lives in `clients/matrix/tests/message_query_contract.rs`, where
  only Matrix can reach it. Fix: create
  `clients/client/src/message_query_contract.rs` behind a
  `contract-tests` feature (or `clients/client/tests/` plus dev-deps on
  `poly-demo` / `poly-stoat` / `poly-matrix` — cargo permits dev-dependency
  cycles), move the four `assert_*_contract` fns there verbatim, and add a
  per-backend test that calls it. Blocked because this PR owns
  `clients/client/` for doc comments only.
  **Note: running Stoat through it is likely to fail** — `poly-stoat` maps all
  three cursors onto Revolt's `before`/`after`/`nearby` but never asserts the
  anchor is excluded, and `map_messages_response` sorts by `timestamp` (the
  same non-authoritative-clock problem fixed here for Matrix).

- [ ] **F.3 — pre-existing clippy debt in `clients/client/tests/`.**
  `cargo clippy -p poly-client --all-targets -- -D warnings` fails on 11
  violations that predate this PR and are untouched by it:
  `client_ui_surface_parity.rs` has 7 `todo!()` (`clippy::todo` is deny), and
  `integration.rs` has `let_underscore_must_use` (:44),
  `wildcard_enum_match_arm` (:440) and two `print_stderr` (`eprintln!`).
  `cargo clippy -p poly-client --lib` is clean. The equivalent debt in
  `clients/matrix/tests/integration.rs` (`match_same_arms`,
  `let_underscore_must_use`) WAS in scope and is fixed in this PR.
