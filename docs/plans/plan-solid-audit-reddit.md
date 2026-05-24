# Plan: SOLID + missing-impl audit — `clients/reddit/`

## Status: IN PROGRESS — Phase A fully shipped

Audit pass over `clients/reddit/src/{backend.rs,lib.rs,signup.rs,parser/*.rs}`
(3610 LoC). Identifies SOLID violations and missing implementations.

Scope: only `clients/reddit/`. Do NOT touch other client crates.

---

## Phase A — Ship-now wins (≤50 LoC each, max 3)

- [x] **A.1 — shipped in this audit pass.** Delete dead helpers
      `_message_id_for_comment` (`backend.rs:175`) and
      `_dm_id_from_channel_id` (`backend.rs:187`). Both are prefixed
      with `_` to silence dead-code warnings — neither is called
      anywhere in-crate (verified via `grep -rn`). 8 LoC removed.
- [x] **A.2** Lift the repeated `Selector::parse(LITERAL).unwrap()`
      sites (`parser/inbox.rs:25,43,51,59`; `parser/post.rs:45,69,79,90,102`;
      `parser/subreddit.rs:71,105,115,163`; `parser/user.rs:30,46,59,96,105,114`)
      into named factory `fn post_selector() -> Selector` helpers per
      module. **Note: `scraper::Selector` is `!Sync` (holds `Rc`), so
      `LazyLock` is NOT viable — use a per-call factory or
      `thread_local!`.** Pure de-duplication, no allocation change for
      the per-call form. _≈40 LoC._ — shipped in this pass
- [x] **A.3** Tighten `NotSupported` error strings into module-level
      `const`s (`backend.rs:1346-1466` — 18 sites all allocating
      identical "Reddit has no X" strings). — shipped in this pass

## Phase B — Medium refactors (50-300 LoC, max 5)

- [ ] **B.1** Split `backend.rs` (1605 LoC, 103 fns). Top-level free
      fns (`html_to_plain_text`, `flatten_comments_into_messages`,
      `render_comments_to_html`, `split_title_body`, 8x ID-mapping
      helpers, `raw_post_to_message`, `raw_dm_to_dm_channel`,
      `user_profile_to_user`, `raw_post_to_viewrow`, `build_sub_server`,
      `build_sub_channel`, sort helpers) belong in `backend/mapping.rs`.
      The 8 ID helpers belong in `backend/ids.rs`. _SRP._
- [ ] **B.2** `backend.rs` `From<RedditError> for ClientError` (`:197`)
      lives 200 lines from the consumer impl. Move to top of file or
      its own `errors.rs`.
- [ ] **B.3** `RedditBackend::get_messages` (~line 600+) mixes post-
      fetching, comment-flattening, and ViewRow construction. Extract
      the comment-flatten pipeline.
- [ ] **B.4** `parser/mod.rs` (247 LoC) bundles `ParseError`, `RawPost`,
      `RawComment`, `RawDm`, `UserProfile`, `parse_timestamp_ms`, AND
      the LoggedOut detector. Split into `parser/types.rs`,
      `parser/error.rs`, `parser/time.rs`, `parser/auth.rs`.
- [ ] **B.5** `lib.rs` (977 LoC) houses `RedditClient` HTTP shim. The
      `with_base_url` constructor (`:962+`) is the only thing keeping
      `lib.rs` exposed — move it into `backend.rs` or a dedicated
      `client.rs` and let `lib.rs` be a 50-line re-export shim.

## Phase C — Architectural rewrites (>300 LoC, max 3)

- [ ] **C.1** WASM/native split: `clients/reddit/src/parser/` is
      `#[cfg(feature = "native")]` because `scraper` doesn't run on
      WASM. That means the WASM plugin has NO parsing capability and
      cannot serve any read action. Decide: ship a separate WASM-
      compatible HTML parser (e.g. `html5gum`) and feature-gate, OR
      mark Reddit explicitly native-only and remove the WASM build
      target. Currently the WASM target compiles to a near-empty
      no-op shell.
- [ ] **C.2** Old-Reddit-only coupling: every parser targets
      `old.reddit.com` markup. Reddit will eventually retire it.
      Re-architect parser layer behind a `trait RedditParser` with
      `OldRedditParser` (current) and stub `ShreddNewParser` impls.
      _Open/Closed — adding the new parser shouldn't require edits
      to call sites._
- [ ] **C.3** `RedditBackend` (`backend.rs:429-1605`, 1176 LoC of impls)
      is the kitchen-sink `IsBackend + SocialGraphBackend +
      DmsAndGroupsBackend + MessagingBackend + DiscoverBackend`. Split
      impls into sibling files per trait, mirroring proposed Lemmy B.1.

---

## Findings index (file:line)

- Dead helpers (shipped Phase A.1): `backend.rs:175,187`.
- Repeated `Selector::parse(...).unwrap()`: 17 sites across `parser/*.rs`.
- Repeated `NotSupported` allocs: `backend.rs:1346-1466` (~22 sites).
- Real capability gap: `backend.rs:1539` `search_messages` claims "not
  yet implemented" — reddit has search; should implement, not refuse.
- TODO in body: `backend.rs:901` (`navigator.connection.effectiveType`).
- WASM/native split: `parser/mod.rs:21` gates entire parser on native.
- SRP violations: `backend.rs` 1605 LoC / 103 fns; `lib.rs` 977 LoC.
