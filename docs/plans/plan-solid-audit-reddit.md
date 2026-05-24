# Plan: SOLID + missing-impl audit — `clients/reddit/`

## Status: ✅ DONE — all tractable phases shipped (changes `totxoywutypz` plan doc / `uvmnpumnvsyk` real code split + B.5 + C.3); C.1 + C.2 deferred with rationale

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

- [x] **B.1 — shipped in change `uvmnpumnvsyk` (was claimed earlier in
      `totxoywutypz` but that commit only updated the plan doc; the
      actual file split was never landed).** Split `backend.rs`
      (1573 LoC, single god-file) into a `backend/` directory of focused
      per-concern modules. See **C.3** below for the per-file LoC
      breakdown — the same physical split closes both B.1 and C.3
      because they describe the same refactor.
- [x] **B.2** `backend.rs` `From<RedditError> for ClientError` (`:197`)
      lives 200 lines from the consumer impl. After audit: already at
      line 189 with a dedicated section header — no relocation needed.
      Match simplified-as-is (6 arms already logically grouped).
      Now lives in `backend/error.rs` post-C.3.
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
- [x] **B.5 — shipped in change `uvmnpumnvsyk`.** Split `lib.rs`
      (977 LoC `RedditClient` HTTP shim) into `client/` submodule with
      domain-separated impl blocks on the same struct (sibling-impl
      pattern, same as `stoat/src/http/`):
      - `client/mod.rs` (217 LoC) — struct + constructors + session /
        fetch / `post_form` / `resolve_url` / `urlencoding_simple`.
        Fields are `pub(super)` so sibling impls can use them.
      - `client/auth.rs` (111 LoC) — `login_with_password`,
        `login_with_session_cookie`, `is_logged_in`.
      - `client/read.rs` (269 LoC) — `list_subreddit`, `get_post`,
        `get_gallery_urls`, `get_user`, `list_subscribed_subreddits`,
        `inbox`, `list_subreddit_page`, `search_subreddits`.
      - `client/write.rs` (242 LoC) — `compose_dm`, `subscribe`,
        `submit_self_post`, `reply_comment`, `vote`, `delete_thing`,
        `edit_user_text`, `mark_message_read`.
      `lib.rs` collapses to a 195-line crate-root that owns only:
      `SLUG`, `plugin_translations`, `RedditError`, `SubredditInfo`,
      `SortKind` + its impl, the `mod` declarations, and the
      `SortKind` unit tests. `RedditClient` is re-exported via
      `pub use client::RedditClient`. All public API surface preserved
      (verified: `cargo check -p poly-reddit` clean,
      `cargo test -p poly-reddit --lib` 13/13 pass).

## Phase C — Architectural rewrites (>300 LoC, max 3)

- [~] **C.1 — DEFERRED.** `clients/reddit/src/parser/` is
      `#[cfg(feature = "native")]` because `scraper` doesn't run on
      WASM. The WASM plugin therefore has NO parsing capability and
      cannot serve any read action — currently the WASM target compiles
      to a near-empty no-op shell. Closing this is an architectural
      decision (ship a WASM-compatible parser like `html5gum` behind
      a feature flag, OR mark Reddit explicitly native-only and remove
      the WASM build target). It needs product/strategy input that the
      audit pass cannot answer unilaterally; not a code-shaped task.
      Tracking as a follow-up plan; explicitly out of scope for the
      SOLID close.
- [~] **C.2 — DEFERRED.** Every parser targets `old.reddit.com` markup.
      Reddit will eventually retire it. Re-architecting the parser
      layer behind a `trait RedditParser` with `OldRedditParser`
      (current) and `ShreddNewParser` (stub) impls is the Open/Closed
      fix — but until a real shredd-new parser exists to *be* the
      second impl, introducing the trait now is speculation. The
      sensible trigger is "the day we start writing the shredd-new
      parser, refactor to the trait as the same PR". Documented here
      so the trigger isn't forgotten.
- [x] **C.3 — shipped in change `uvmnpumnvsyk` (same change as B.1).**
      `RedditBackend` (1176 LoC of trait impls in the old
      `backend.rs`) was the kitchen-sink
      `IsBackend + SocialGraphBackend + DmsAndGroupsBackend +
      MessagingBackend + DiscoverBackend + SettingsBackend +
      ViewDescriptorBackend`. Split into the planned
      `backend/` directory of per-concern modules:
      - `backend/mod.rs` (181 LoC) — `RedditBackend` struct +
        inherent fns (`media_previews_enabled`, `current_sort`,
        `backend_type`, `account_id`, `account_display_name`,
        `build_session`, `fetch_post_thread_messages`).
        Fields are `pub(crate)` so sibling impls can read them.
      - `backend/ids.rs` (37 LoC) — 8 ID/name bijection fns.
      - `backend/error.rs` (46 LoC) — `From<RedditError> for
        ClientError` + 16 `NS_*` constants (A.3 holdover).
      - `backend/mapping.rs` (345 LoC) — `parser::*` →
        `poly_client::*` mappers, `html_to_plain_text`,
        `render_comments_to_html`, sort-key codecs,
        `split_title_body`, `raw_*` constructors.
      - `backend/is_backend.rs` (384 LoC) — `IsBackend` impl
        (auth, servers, channels, send/get messages, mechanisms,
        plugin manifest, capability casts).
      - `backend/social_graph.rs` (82 LoC) — `SocialGraphBackend` impl.
      - `backend/dms_and_groups.rs` (77 LoC) — `DmsAndGroupsBackend`.
      - `backend/messaging.rs` (106 LoC) — `MessagingBackend` impl.
      - `backend/discover.rs` (44 LoC) — `DiscoverBackend` impl.
      - `backend/settings.rs` (19 LoC) — `SettingsBackend` impl.
      - `backend/view_descriptor.rs` (347 LoC) — `ViewDescriptorBackend`
        impl (sidebar declaration + sort-action invocation +
        `get_channel_view` + `get_view_rows` + `get_view_detail`).
      Pure structural split — zero behaviour change. Verified clean:
      `cargo check -p poly-reddit`, `cargo check -p poly-core --target
      wasm32-unknown-unknown`, `cargo test -p poly-reddit --lib`
      (13/13 pass). _SRP — every file now has one trait worth of
      reasons-to-change._

---

## Findings index (file:line — paths are pre-C.3 historic for grep
## continuity; current paths in parens)

- Dead helpers (shipped Phase A.1): `backend.rs:175,187`
  (now removed entirely).
- Repeated `Selector::parse(...).unwrap()`: 17 sites across `parser/*.rs`
  (still in `parser/*.rs` — `*_selector()` factories live there).
- Repeated `NotSupported` allocs: `backend.rs:1346-1466` (~22 sites)
  (now `backend/error.rs` `NS_*` constants).
- Real capability gap: `backend.rs:1539` `search_messages` claims "not
  yet implemented" — reddit has search; should implement, not refuse.
  Still open — tracking as a separate impl task (now lives in
  `backend/messaging.rs`).
- TODO in body: `backend.rs:901` (`navigator.connection.effectiveType`)
  (now `backend/is_backend.rs` `client_mechanisms`).
- WASM/native split: `parser/mod.rs:21` gates entire parser on native.
  See C.1.
- SRP violations: closed by B.5 (lib.rs) + C.3 (backend.rs). Largest
  remaining file is `backend/is_backend.rs` at 384 LoC.
