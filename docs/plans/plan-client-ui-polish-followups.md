# Plan — Client-UI Polish Follow-Ups

> **Created:** 2026-04-19
> **Status:** ✅ COMPLETE 2026-04-19 — every item shipped or superseded
> **Notes:** F1, F2, F4, F5, F6, F7, F8, F9, F10, F11, F12 all shipped. F3 (Playwright) dropped — superseded by `docs/plans/plan-blitz-component-tests.md` (deferred design plan for in-process Dioxus Blitz snapshots, since Playwright is too slow for the desired iteration loop).
> **Parent:** `plan-client-ui-polish.md` (✅ 10 packs shipped but with known gaps)
> **Why this plan exists:** the parent polish plan was marked ✅ COMPLETE based on `cargo check --workspace` + `cargo test --workspace --lib` — which skips integration & e2e tests. A full `cargo test --workspace` revealed **30 failures** in `poly-plugin-loader-tests`. F1 fixed that. This plan collects every remaining follow-up in one honest list.

---

## 0. Honest accounting

The previous status report claimed "363 passing." That count came from `--lib` only. Pre-F1 reality was `463 passed, 30 failed`. As of `main @ ea3ac2ee` (F1 shipped):

```
cargo test --workspace  →  780 passed, 0 failed
cargo check --workspace --all-targets  →  0 warnings
```

**F1 fix:** all 7 plugins (demo/discord/matrix/lemmy/stoat/teams/server-client) updated to the post-Pack-E WIT world (`BackendType` enum dropped → string slugs; `plugin-metadata.get-settings-schema` removed; six new exported interfaces stubbed). `crates/plugin-host-tests/src/lib.rs::load_plugin` now auto-runs `cargo component build` once per process when the artifact is missing, so `cargo test` works whether or not the WASM was pre-built. Blanket `#![allow(clippy::unwrap_used, expect_used, panic, unused_variables)]` removed from every harness file; ~70 sites refactored to `Result`-returning `HarnessResult` with `?`-propagation. Seven previously-orphaned harness helpers wired in as real `demo.rs` tests (no `#[allow(dead_code)]`).

---

## 1. Item inventory

### ✅ F1 — WASM plugin harness can't run without manual build step (DONE 2026-04-19)

**Resolution:** `main @ ea3ac2ee`. All 7 plugins updated to the post-Pack-E WIT world. `load_plugin` now auto-builds the missing WASM via `cargo component build -p <crate> --target wasm32-wasip2` (idempotent per process via `OnceLock<Mutex<HashSet>>`). e2e tests run clean: 37 passed in `client_e2e/main.rs` (30 original + 7 newly-wired orphan helpers). Total workspace: 780 passed, 0 failed.

---

### ✅ F2 — D12 `BackendCapabilities` flag removal (DONE 2026-04-19)

**Resolution:** all 7 `TODO(D12)` fields (`presence`, `typing_indicators`, `reactions`, `search_messages`, `attachments`, `create_server`, `create_channel`) deleted from `BackendCapabilities`. Five had zero production readers (test-only); two `create_server` readers (`crates/core/src/ui/routes.rs::CreateServerRoute`, `crates/core/src/ui/account/common/notifications.rs::NotificationMenuFilter::supported_by`) replaced with a slug-derived check. Per-plugin capability tests pruned to assert only on the surviving shape fields. `cargo test --workspace` still 0 failures (776 passed; -4 from removed assertion lines).

---

### ⏸ F3 — UI snapshot goldens infrastructure (DROPPED — superseded 2026-04-19)

**Resolution:** Playwright dropped — too slow for the iteration loop the user wants. Replaced by a deferred plan for **Dioxus Blitz–based component testing** (WGPU-native, in-process Dioxus rendering — already wired in `apps/desktop-blitz` as a shipping renderer). New plan file: `docs/plans/plan-blitz-component-tests.md`. Snapshot/regression coverage of visual-polish packs lives in that plan now; this followup is closed out without action here.

---

### ✅ F4 — Matrix `m.space.parent`/`child` tree nesting (DONE 2026-04-19)

**Resolution:** Matrix's `get_sidebar_declaration` (both native + WASM) switched from the stock `SpacesRoomsLayout` (depth-1 cap) to `SidebarLayoutKind::Custom` with a single `SidebarSection` containing a flat `Vec<SidebarItem>` carrying `parent_id` pointers reflecting the user's `m.space.child` graph. The host's existing `CustomSidebar` reconstructs the tree. Async `fetch_space_tree` walks `GET /_matrix/client/v3/joined_rooms` → `GET .../rooms/{id}/state` (to detect `m.room.create.type == "m.space"` and pull room name) → `GET .../rooms/{id}/hierarchy` per space, with cycle detection via a `visited_spaces` set. Pure helper `build_sidebar_items` extracted for unit testing; 5 new tests cover preserved/dropped parent_ids, space-vs-room route kinds, label resolution, and FTL regression.

---

### ✅ F5 — GitHub discussions tab (DONE 2026-04-19)

**Resolution:** added `GitHubHttpClient::graphql_query<T: DeserializeOwned>(query, variables)` (POSTs to `https://api.github.com/graphql` with the existing bearer auth) plus a `list_discussions(owner, repo, first, after) -> (Vec<GhDiscussion>, Option<String>)` helper using a fixed query that pulls number/title/body/url/timestamps/upvote_count/comments_count/author/category/answer_chosen_at/closed. New `GhDiscussion` + supporting wire-format types in `types.rs`. New `mapping::map_discussion_to_viewrow` exposes title as `primary_text`, category name as `secondary_text`, "👍 N · 💬 N" as `meta_text`, and "answered"/"closed" badges. `lib.rs::get_view_rows` discussions branch now calls `list_discussions` instead of returning the empty placeholder; cursor wired as `CursorKind::Opaque`. Edge cases handled: null `nodes` entries flatten away, deleted-account `author == null`, GraphQL `errors[]` joined into a single `GhError::Exit`. 5 unit tests on the mapping. Native build also routes through `gh api graphql` for parity with the REST helpers.

---

### ✅ F6 — HN recursive comments (DONE 2026-04-19)

**Resolution:** the recursive BFS fetcher (`get_comment_thread` in `clients/hackernews/src/lib.rs`) was already in place but two limits gated it: `get_view_detail` declared `max_depth: 1` (host's tree-body uses that as a row-cap multiplier) and used `take(50)` on the top-level kids; `get_messages` used the feed's `query.limit.unwrap_or(20)` for comment channels. Bumped to `TreeSpec { root_page_size: 30, max_depth: 8 }` (≤ 240 rows ceiling), comment-channel default to 300, BFS clamp to 1000. Real HN threads now render fully.

---

### ✅ F7 — Custom-block usage lint scanner (DONE 2026-04-19)

**Resolution:** added `crates/lint-gate/build/custom_block_usage.rs` (mirrored as `pub mod custom_block_usage` in `src/lib.rs` for unit tests). Scanner counts `CustomBlock {` literal sites per plugin (substring `MyCustomBlock` and type-position references are excluded by leading-ident-char + immediate-`{` heuristics). Threshold: 5 per plugin. Today's max is 1; the headroom catches a regression where typed surfaces get replaced by HTML blobs without flagging existing usage. Wired into `build.rs` alongside the other 8 rules. 6 unit tests cover the counting + path-extraction helpers.

---

### ✅ F8 — Custom-block compile-fail trybuild fixture (DROPPED + replaced 2026-04-19)

**Resolution:** trybuild idea is unworkable — Rust's type system can't inspect string contents at compile time. Replaced with a `debug_assert!` in `crates/core/src/ui/client_ui/custom_block.rs::sanitize_html` that fails any debug build whose sanitizer leaks a `<script` tag through. Defence-in-depth on top of the existing `script_tag_stripped` runtime test in the sanitizer's own test suite.

---

### ✅ F9 — Host-api KV for plugin settings (DONE 2026-04-19)

**Resolution:** all 7 plugin WASM guests (demo / discord / lemmy / matrix / stoat / teams / server-client) now route `ClientSettingsGuest::get_setting_value` / `set_setting_value` through `crate::wit_bindings::poly::messenger::host_api::storage_get` / `storage_set` instead of returning the previous `Ok("null")` / `Ok(())` stubs. Composite key format: `"settings:{scope-label}:{scope-id}:{user-key}"` where `scope-label` is one of `account-global` / `per-server` / `per-channel` / `per-user`. The host backs the KV with SQLite at `$XDG_DATA_HOME/poly/storage.sqlite3` (production) or in-memory (tests). Native ClientBackend impls retain the in-process `SettingsStorageCell` since they're test-only scaffolding — production paths always go through WASM + WIT.

---

### ✅ F10 — State-aware menu items for chat backends (DONE 2026-04-19)

**Resolution:** Discord, Stoat, Matrix, Teams each shipped state-aware `get_context_menu_items` in both native + WASM impls, with distinct ids per state (e.g. `mute-channel` / `unmute-channel`). State held in `std::sync::Mutex<HashSet<String>>` (or `RwLock`) on native for `Send + Sync`; `thread_local!` `RefCell<…>` on WASM (single-threaded). 50+ new FTL keys added in `en` plus stub copies in `de`/`es`/`fr` with `# TODO(translate)` markers.

---

### ✅ F11 — Non-English locale real translations (DONE 2026-04-19)

**Resolution:** all 33 stub locale files (`{de,es,fr}` × 11 bundles — `locales/<lang>/main.ftl` + `clients/*/locales/<lang>/plugin.ftl`) translated via three parallel sonnet subagents (one per language) with concrete style-guide examples baked into each prompt. ~500+ strings across 3 locales. `# TODO(translate)` markers reduced from 33 files to 0. Brand names (Discord, Matrix, Lemmy, Hacker News, MCP) and HN section names (Ask, Show) kept in English; common UI tech loanwords (Channel/Canal, Pull request) translated where natural per locale convention. `cargo check --workspace --all-targets` clean.

---

### ✅ F12 — Chat-view UTF-8 em-dash mojibake (DONE 2026-04-19)

**Resolution:** root cause was `strip_data_href_on_anchors` in `crates/core/src/ui/client_ui/custom_block.rs` — the byte-level state machine cast individual bytes to `char` (`bytes[i] as char`), reinterpreting multi-byte UTF-8 sequences as Latin-1. An em-dash (`U+2014`, bytes `0xE2 0x80 0x94`) became `â` (`U+00E2`) plus garbage. Fixed by iterating with `html[i..].chars().next()` and advancing `i += c.len_utf8()`. Six new regression tests cover em-dash, accented chars, CJK, and emoji through both `sanitize_html` and the markdown render path.

---

## 2. Proposed sequencing

1. **Ship F1 first** — stop lying about test counts. Every future pack's status is only trustworthy once the e2e suite actually runs.
2. **F2 (D12 flags)** — small cleanup, unblocked, measurable.
3. **F7 + F8** — small lint-gate follow-ups.
4. **F4 (Matrix nesting)** — use CustomSidebar pattern; meaningful UX win.
5. **F10 (state-aware menus)** — per-backend, small increments.
6. **F5 + F6 + F9** — bigger per-backend or architectural work.
7. **F3 (snapshot infra)** — dedicated day.
8. **F11 + F12** — ongoing / debug-driven.

## 3. Acceptance criteria for this plan's completion

Each F-item lands with its own test set (per §1.2 of the parent plan). The plan closes when:

- `cargo test --workspace` (no `--lib` filter) reports zero failures.
- `cargo test --workspace --all-features` same.
- The polish plan's `docs/plans/plan-client-ui-polish.md` status table is trustworthy again — specifically the "layer (d) E2E via WASM host" column shows real pass counts.
- All `TODO(D12)` markers in `clients/client/src/types.rs` are gone.

## 4. What went wrong

Calling out the meta-lesson so future sessions don't repeat it: **`cargo test --lib` is not enough for "tests pass" claims.** It skips integration tests (which includes `tests/*.rs` files), so backend round-trip tests and the plugin-host e2e suite get silently excluded from the pass count.

Mandatory going forward: any "tests pass" claim must come from `cargo test --workspace` with no filter, OR explicitly state scope ("--lib only; integration tests require the WASM build step in F1").
