# Plan: SOLID + missing-impl audit — `clients/hackernews/`

## Status: ✅ DONE — A.1+A.2 shipped (`llvrkmlt`), A.3 skipped (already covered), B.1 (`6067edd2`), B.3+B.4 (prev change), B.2+B.5 (`xtkvrtox`). Phase C deferred: C.1/C.2/C.3 are >300-LoC architectural rewrites whose scope crosses crates (host-bridge KV wiring for C.1; auth/error redesign for C.2; native↔WASM HTML-parse strategy decision for C.3). They are intentionally left for follow-up plans rather than rolled into the SOLID-audit close-out — file separate plan-doc per item when ready to land.

Audit pass over `clients/hackernews/src/{api.rs,auth.rs,cache.rs,lib.rs,mapping.rs,signup.rs,types.rs}`
(2245 LoC). Identifies SOLID violations and missing implementations.

Scope: only `clients/hackernews/`. Do NOT touch other client crates.

---

## Phase A — Ship-now wins (≤50 LoC each, max 3)

- [x] **A.1** Drop "TODO: migrate to" boilerplate from in-memory
      settings storage doc comment (`lib.rs:59`). _≤5 LoC._ — shipped
- [x] **A.2** Dedup `NotSupported` allocation strings (`lib.rs:716-829`,
      ~22 sites all "Hacker News has no X") into module-level `const`
      slices. _≈30 LoC removed._ — shipped (10 const + 19 call sites updated)
- [~] **A.3** SKIPPED — `mapping.rs:473` already has `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`; the `.expect()` at `:548` is covered. No change needed.

## Phase B — Medium refactors (50-300 LoC, max 5)

- [x] **B.1** Split `lib.rs` (901 LoC, 64 fns) `IsBackend` impl from
      sibling trait impls (`SocialGraphBackend` line 701,
      `DmsAndGroupsBackend` line 774, plus 6 trait methods returning
      `NotSupported` each). Move trait impls to sibling modules. SRP.
      — shipped in change `6067edd2`; `lib.rs` reduced to ~370 LoC;
      new files: `social_graph.rs`, `dms_and_groups.rs`, `settings.rs`,
      `view_descriptor.rs`; also removed stale `auth.rs` duplicate
      (B.4 had created `auth/mod.rs` but left the old file).
- [x] **B.2** `mapping.rs` (591 LoC) has both production mapping fns
      AND 200+ lines of test fixtures (`:540+`). Split fixtures into
      `mapping/tests.rs`. — shipped in change `xtkvrtox`;
      `mapping.rs` → `mapping/mod.rs` (466 LoC, production-only) +
      `mapping/tests.rs` (123 LoC, fixtures + 7 tests). All 12 unit
      tests still pass.
- [x] **B.3** `IsBackend::send_message` (`lib.rs:240-307`) does
      channel-id dispatch via a string `match` AND constructs the
      typed payload AND calls auth-gated `submit` AND maps the
      response. Split into smaller helpers. The current shape forces
      future channel additions to edit one giant function (Open/Closed).
      — shipped: extracted `require_write_session`, `require_post_channel`,
      `require_text_content`, `build_pending_message`; body collapsed to ~10 lines.
- [x] **B.4** `auth.rs` (261 LoC) bundles cookie extraction, login
      form parsing, AND submit logic. Split cookie module out — it's
      reusable (the test at `:222` would migrate cleanly).
      — shipped: `auth.rs` → `auth/mod.rs` + `auth/cookies.rs`; cookie tests
      migrated to `cookies.rs`; `extract_user_cookie` is `pub(super)` in submodule.
- [x] **B.5** `IsBackend::get_view_rows` (`lib.rs:519+`) and
      `get_view_detail` (`:587+`) are large dispatchers on view-id
      patterns. Extract a `ViewKind` enum + `kind.fetch(...)` to make
      adding a new view kind a one-impl addition (Open/Closed).
      — shipped in change `xtkvrtox`; introduced private `HnViewKind`
      enum (`Overview | Feed(HnFeed)`) in
      `view_descriptor.rs` with `from_channel_id` / `feed` / `map_row`
      methods, plus `parse_offset` and `fetch_feed_page` helpers.
      `get_view_rows` collapsed from ~50 lines of inline dispatch to
      ~15 lines. Adding a new view kind (e.g. user profiles) now means
      adding one enum variant + one arm in `map_row`/`feed`.
      `get_view_detail` already had a single-shape body (no branching)
      so it was not touched — applying the enum there would have been
      cosmetic with no Open/Closed benefit.

## Phase C — Architectural rewrites (>300 LoC, max 3)

- [~] **C.1** DEFERRED — Polite cache (`cache.rs`, 112 LoC) is in-memory
      only and bypassed on every native restart. Move to host KV via
      `client.config.hackernews.cache.*` namespace. Architectural
      rewrite >300 LoC crossing crate boundaries (cache.rs + lib.rs +
      host-bridge wiring + KV-namespace contracts). Out of scope for
      the SOLID close-out; needs its own plan-doc that scopes the
      cross-crate KV-cache pattern (likely reusable across all backends
      with similar caches — Discord, Matrix). Filing as follow-up.
- [~] **C.2** DEFERRED — HN API client (`api.rs`, 216 LoC) mixes
      Firebase JSON + HTML-scrape. Splitting into `HnReadApi` +
      `HnWriteApi` is the right Open/Closed move but cascades into
      auth-flow and error-mapping redesigns (the scraper-side errors
      have different recovery semantics than Firebase 4xx/5xx).
      Architectural rewrite — file separate plan-doc.
      _Note 2026-05-24: orthogonal_ trait-side write isolation
      (lift `send_message` to `WritableMessagingBackend`) shipped via
      `plan-trait-split-readable-vs-writable.md` Phase D.9. The
      api-client split below is still outstanding and is the meatier
      piece.
- [~] **C.3** DEFERRED — WASM target: `clients/hackernews` builds for
      WASM but the `scraper`-using auth/post path is native-only via
      feature gate. Same as Reddit C.1 — needs a project-wide decision
      between "all HTML-scrape clients stay native-only" vs "adopt a
      WASM-compatible HTML parser". This is a policy decision, not a
      hackernews-local refactor; tracked separately.

---

## Findings index (file:line)

- Pack C P18 stub comment: `lib.rs:59`.
- Repeated `NotSupported` allocs: `lib.rs:716,720,724,732,736,740,744,748,764,784,788,792,796,800,804,812,816,820,829` (19 sites).
- Capability stubs (legitimate — HN has no DMs/friends/groups):
  `lib.rs:716-829`.
- Real capability gap not flagged "not yet implemented" but missing:
  none — HN coverage is honest about its limits.
- SRP violation: `lib.rs` 901 LoC / 64 fns; `mapping.rs` 591 LoC.
