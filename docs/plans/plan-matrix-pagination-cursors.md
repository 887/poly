# Plan: Matrix `get_messages` cursor contract (`before` / `after` / `around`)

## Status: 📋 PLANNED

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

## Phase A — Honesty first: stop returning the wrong page

Small, independently landable, and removes the silent-wrong-answer class before
the real implementation exists. Do this even if Phase B slips.

- [ ] **A.1** In `clients/matrix/src/is_backend.rs:268` `get_messages`, return
  `ClientError::NotSupported` naming the unhandled cursor when
  `query.around.is_some()`. (Precedent: the Teams backend does exactly this at
  `clients/teams/src/is_backend.rs:275` after the same review.)
- [ ] **A.2** Same treatment for `query.after.is_some()` while it is not a real
  cursor — a forward page from the sync token is not "messages after X".
- [ ] **A.3** Make the cold-cache `before` path explicit rather than silently
  malformed: if `resolve_pagination_token` returns the input unchanged **and**
  the input looks like an event ID (`starts_with('$')`), treat it as a miss and
  take the initial-sync branch instead of sending it as `from`. Add the
  event-ID-shape check as a named helper so Phase B can reuse it.
- [ ] **A.4** Unit tests in `clients/matrix/src/lib.rs` (next to the existing
  `pagination_token_round_trips_by_oldest_event_id` at `:773`): `around` →
  `NotSupported`; `after` → `NotSupported`; an unresolved `$`-prefixed `before`
  does not reach `fetch_messages` as `from`.

## Phase B — `/context/{eventId}` support in the HTTP layer

- [ ] **B.1** Add `ContextResponse` to `clients/matrix/src/api.rs` (alongside
  `MessagesResponse` at `:288`): `events_before`, `event`, `events_after`,
  `start`, `end`.
- [ ] **B.2** Add `fetch_context(&self, room_id, event_id, limit) -> ClientResult<ContextResponse>`
  to `clients/matrix/src/http.rs` (mirror the shape of `fetch_messages` at
  `:379`), calling
  `GET /_matrix/client/v3/rooms/{room}/context/{event}?limit=N`.
- [ ] **B.3** Deserialisation test against a captured Synapse fixture, in the
  same test module as `messages_response_keeps_the_end_pagination_token`
  (`clients/matrix/src/lib.rs:755`).

## Phase C — Wire all three cursors through `get_messages`

- [ ] **C.1** `around`: call `fetch_context(channel_id, id, limit)`, map
  `events_before` + `event` + `events_after` through `room_event_to_message`,
  and record **both** returned tokens — `start` against the newest decoded
  message and `end` against the oldest — via `record_pagination_token`. Replace
  the A.1 `NotSupported`.
- [ ] **C.2** `before` cold-cache path: on a token-map miss, call
  `fetch_context(channel_id, before, 1)` to obtain a real `start` token for that
  event, then continue the normal `dir=b` pagination from it. Replaces the A.3
  fallback-to-sync behaviour, which loses the user's scroll position.
- [ ] **C.3** `after`: same as C.2 but pagination continues from `end` with
  `dir=f`, and the recorded token is keyed on the *newest* decoded message
  (the current `record_pagination_token` call at
  `clients/matrix/src/is_backend.rs:325` deliberately keys on the oldest and is
  correct only for `dir=b` — do not reuse it unmodified). Replace the A.2
  `NotSupported`.
- [ ] **C.4** Review the `paging_backwards` heuristic at `:289`
  (`from.is_empty() || query.after.is_none()`) — once `after` is a real cursor
  the direction is a function of *which cursor was supplied*, not of emptiness.
  Rewrite as an explicit match over `(before, after, around)`.
- [ ] **C.5** Confirm the token map bound at `clients/matrix/src/lib.rs:140`
  still holds now that up to two tokens are recorded per page
  (`pagination_token_cache_is_bounded`, `:782`).

## Phase D — Cross-backend contract test (the LSP fix)

Without this, the two backends drift apart again on the next change.

- [ ] **D.1** Write a shared cursor-contract test helper in
  `clients/client/` (or a new `clients/client/tests/message_query_contract.rs`)
  that, given any `IsBackend` + a seeded channel, asserts: `before(id)` returns
  only messages older than `id`; `after(id)` only newer; `around(id)` contains
  `id`; and an unsupported cursor returns `NotSupported` rather than a wrong
  page.
- [ ] **D.2** Run it against the demo backend (`clients/demo/`) as the reference
  impl, plus Matrix and Stoat against their test servers (`test-matrix` :9100,
  `test-stoat` :9101 — see `CLAUDE.md` "Test-server Avatar URL Conventions" for
  the runner).
- [ ] **D.3** Document the contract in the `MessageQuery` doc comment
  (`clients/client/src/types/message.rs:160-176`): state that a backend which
  cannot honour a cursor MUST return `NotSupported` and MUST NOT substitute a
  different page. This is the sentence the LSP violation was missing.

## Phase E — Verify (QA gate — iterate until a clean round)

- [ ] **E.1** `cargo test -p poly-matrix -p poly-stoat -p poly-client` green.
- [ ] **E.2** `cargo clippy --workspace -- -D warnings` clean;
  `cargo check -p poly-lint-gate` rc=0, zero baseline entries added.
- [ ] **E.3** Manual walk against `test-matrix` (:9100): scroll up past three
  pages in a seeded room, then click a search hit and confirm the view lands on
  that message with context above and below.
- [ ] **E.4** Re-run E.1–E.3 after the final fix; tick DONE only off a round that
  surfaced nothing new.
