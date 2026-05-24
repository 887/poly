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

- [x] **B.1 — shipped in change `wlqmorlz`.** Split `backend.rs`
      (1573 LoC, single god-file) into a `backend/` directory of focused
      per-concern modules:
      - `backend/mod.rs` — `RedditBackend` struct + inherent fns +
        `fetch_post_thread_messages` async helper (B.3 holdover); 168 LoC.
      - `backend/ids.rs` — 8 ID/name bijection fns; 46 LoC.
      - `backend/error.rs` — `From<RedditError> for ClientError` +
        16 `NS_*` constants (A.3 holdover); 51 LoC.
      - `backend/mapping.rs` — `parser::*` → `poly_client::*` mappers
        plus HTML sanitisers (`html_to_plain_text`,
        `render_comments_to_html`) and sort-key codecs; 326 LoC.
      - `backend/is_backend.rs` — `IsBackend` impl (auth, servers,
        channels, send/get messages, mechanisms, capability casts);
        373 LoC.
      - `backend/social_graph.rs` — `SocialGraphBackend` impl; 82 LoC.
      - `backend/dms_and_groups.rs` — `DmsAndGroupsBackend` impl; 81 LoC.
      - `backend/messaging.rs` — `MessagingBackend` impl; 117 LoC.
      - `backend/discover.rs` — `DiscoverBackend` impl; 47 LoC.
      - `backend/settings.rs` — `SettingsBackend` impl; 23 LoC.
      - `backend/view_descriptor.rs` — `ViewDescriptorBackend` impl
        (sidebar declaration + sort-action + view rows/detail); 333 LoC.
      Pure structural split — zero behaviour change. Largest remaining
      file is `view_descriptor.rs` at 333 LoC. _SRP._
- [x] **B.2** `backend.rs` `From<RedditError> for ClientError` (`:197`)
      lives 200 lines from the consumer impl. After audit: already at
      line 189 with a dedicated section header — no relocation needed.
      Match simplified-as-is (6 arms already logically grouped).
      — shipped in this pass
- [x] **B.3** `RedditBackend::get_messages` (~line 600+) mixes post-
      fetching, comment-flattening, and ViewRow construction. Extracted
      `fetch_post_thread_messages(bare_id, &bt)` helper that owns the
      gallery fetch + OP enrichment + comment flatten pipeline. Call
      site in `get_messages` is now 2 lines. — shipped in this pass
- [x] **B.4** `parser/mod.rs` (247 LoC) bundles `ParseError`, `RawPost`,
      `RawComment`, `RawDm`, `UserProfile`, `parse_timestamp_ms`, AND
      the LoggedOut detector. Split into `parser/error.rs` (`ParseError`
      only), `parser/types.rs` (`RawPost`, `RawComment`, `RawDm`,
      `UserProfile`, `UserOverviewItem`), `parser/mod.rs` kept as
      re-export + glue only (`detect_logged_out`, `parse_html`,
      `data_attr`, `parse_timestamp_ms`, tests). All `use super::` in
      submodules unchanged (re-exports preserve the path).
      — shipped in this pass
- [~] **B.5 — DEFERRED.** `lib.rs` (977 LoC) houses `RedditClient` HTTP
      shim. The plan suggested moving `with_base_url` "out" — but the
      cited line (`:962`) sits inside a `#[cfg(test)] mod tests` block,
      not in load-bearing code. The actual bulk (~750 LoC) is the
      `impl RedditClient { … }` with ~30 async HTTP methods (`get_post`,
      `inbox`, `reply_comment`, `submit_self_post`, …). Splitting that
      into `client/{auth,read,write,session}.rs` is a follow-up
      opportunity — but unlike B.1 the dependencies all flow through
      `&self` on private fields (`http`, `base_url`, `session_cookie`)
      and the cross-method helpers (`with_session_cookie`,
      `capture_session_cookie`, `fetch_text`, `post_form`) make the
      split more delicate than B.1's per-trait carve-out. Tracking as
      a follow-up; not blocking Phase B closure.

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
