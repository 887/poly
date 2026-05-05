# Plan: SOLID refactor survey + ranked wins

> Owner: alexander.stuermer@aareon.com
> Created: 2026-05-03
> Status: 🟡 IN PROGRESS — Phase G fully closed (G.6g `a81777cc`, G.6h `55a7821a`, G.6i `c68b8d91`, G.6j `873cb1af`, G.6k `ae8d96b3`+`080e246b`, G.6l `c6b67d22`); phases H-J pending
>
> Source shards (raw findings, do not delete — referenced throughout):
> - `docs/plans/.solid-survey-shards/A.md` — Single Responsibility (oversize)
> - `docs/plans/.solid-survey-shards/B.md` — Open/Closed + Liskov
> - `docs/plans/.solid-survey-shards/C.md` — Interface Segregation + DI
> - `docs/plans/.solid-survey-shards/D.md` — State + reactive architecture
> - `docs/plans/.solid-survey-shards/E.md` — Test infra + tooling

## Executive summary

The codebase is mostly SOLID where it matters most — DI at the **crate
boundary** is clean (zero `use poly_<backend>::` imports inside
`crates/core/`), `BatchedSignal` already encodes hang-class
countermeasures, and the WIT contract makes some of the kitchen-sinky
shapes load-bearing.

The **cross-cutting** rot lives in three places:

1. **Three god structs** (`AppState` 100 fields, `ChatData` 30 fields,
   `ChatViewMarkupCtx` 70 fields) and **one god component** (`chat_view.rs`
   6809 lines) own most of what makes adding features hard.
2. **One god trait** (`ClientBackend` 88 methods, 52 of them implemented
   by ≤4 of 11 backends) forces every backend to stub or default 60 % of
   the surface and forces every UI consumer to ask "is this supported"
   via runtime capability flags.
3. **Three sources of truth for backend capabilities** (slug-keyed
   match arms in `types.rs`, per-backend trait override, per-backend
   parity test) drift independently and are kept in sync only by
   bespoke regression tests.

A small number of **cheap mechanical wins** (slug constants,
`baked_locales` per-locale split, test-harness extraction) can ship
immediately for measurable PR-friction improvements. The bigger
god-struct + kitchen-sink-trait splits need phased rollout but
unblock the oversize-component refactor (`plan-component-lints.md`).

The plan below is ordered to **start with the cheapest, lowest-risk
wins that pay back immediately**, then tackle the structural items in
dependency order. Each phase notes whether it can be delegated to a
sonnet worktree agent (mostly yes for the mechanical phases) or needs
orchestrator-led design (mostly the trait-split phases).

---

## Goals

1. Reduce the **edit-N-files burden** when adding a backend, route,
   view kind, or capability — measure by counting parallel match-arm
   edits and grep counts.
2. Reduce the **re-render fan-out** of the three god signals so that
   touching `voice_connection` doesn't re-render the chat list, and
   touching `notifications` doesn't re-render the call banner. Measure
   by `app_state` / `chat_data` subscribe-site count and per-write
   re-render count under the existing hang-class lints.
3. Cut the **trait stub burden** from ~52 NotSupported defaults to ~10
   genuinely shared methods + capability sub-traits opt-in.
4. Eliminate **duplicated lint surfaces** (bash + dylint + lint-gate
   express same intent) by funneling through a single rules library.
5. Shrink **the worst single-file blowups** (chat_view.rs,
   tools.rs, types.rs) to navigable sub-modules.

## Non-Goals

- No "rewrite Dioxus into another reactive framework" or "pull WASM
  out" structural moves.
- No changes to the WIT contract surface — the trait splits we propose
  must mirror existing WIT interface boundaries.
- No demo-data restructuring (`clients/demo/src/data.rs` is fixture
  data and stays as-is per shard A.3).

---

## Top 10 wins, ranked by ROI

ROI = (impact across surfaces) / (effort + risk). Each row links to the
detailed shard write-up.

| # | Win | Effort | Impact | Risk | Shards |
|---|-----|--------|--------|------|--------|
| 1 | Per-backend `pub const SLUG: &str` (199 string literals → 11 consts) | S | M | Low | B.1.3 |
| 2 | Split `baked_locales.rs` per locale (PR-conflict relief) | S | M | Low | E.1#2 |
| 3 | Extract `BackendHarness` shim from `servers/test-*/` (~270 LOC dup) | S/M | M | Low | E.1#1, E.3 |
| 4 | `clients/client/src/types.rs` file split (1891 lines → 10 modules, `pub use` re-exports) | S/M | M | Low | A.1#4 |
| 5 | Demo backend triplication → `DemoFlavour` parametric (~1000 LOC removed) | M | M | Med | B.1.2, A.1#2 |
| 6 | `ClientManager::with_backend(...)` helper (collapses 48 sites) + LSP fixes | M | H | Low | D.1.1, B.2.* |
| 7 | Kill `capabilities_for_slug` table — single source of truth via trait | M | H | Med | B.1.1 |
| 8 | Split `chat_view.rs` (6809 lines) per `chat_view/` sub-module | L | H | Med | A.1#1 |
| 9 | Slice `BatchedSignal<ChatData>` — extract `VoiceState`, `DragState`, `ChatLists` first | L | H | Med | C.3.1, D.3 |
| 10 | Capability sub-traits — split `ClientBackend` mirroring WIT interfaces | XL | H | High | C.2, B.1.5 |

**Wins #1-4 are cheap mechanical mops; do them first as warm-up + immediate PR-friction relief.**
**Wins #5-7 set up the larger structural moves; do them in parallel where files don't overlap.**
**Wins #8-9 are the tent-pole structural refactors; sequence carefully.**
**Win #10 is the long-horizon trait split; gated on #6+#7 landing.**

---

## Phased delivery

### Phase A — Mechanical wins (warm-up; ~2 days total)

Parallelisable across 4 sonnet worktree agents, all touching
disjoint files.

- [x] **A.1** Per-backend `pub const SLUG: &str` exported from each
      `clients/<name>/src/lib.rs`; flip the 199 in-crate
      `BackendType::from("…")` literals to `BackendType::from(crate::SLUG)`.
      Sites: see B.1.3 table. Effort S. — shipped in commit `c844f314`.
- [x] **A.2** Split `crates/core/src/i18n/baked_locales.rs` (4487
      lines) per-locale. Patch site: `crates/core/build.rs:84-109`. Net
      patch ≤15 lines. Effort S. Source: E.1#2. — shipped in commit `274f92ef`.
- [x] **A.3** Extract `BackendHarness` trait + `run::<H>()` helper
      into `servers/test-common`. Collapse all 9 `main.rs` shells to
      4 lines each, 9 `seed/reset/reseed` triples → harness default,
      lifecycle HTTP handlers → harness, layer chain → harness.
      Reddit landed in a follow-up (added inspect buffer + idempotent
      seed/reset on RedditState; the auth-state arg is intentionally
      ignored since Reddit's sessions live inline in the state).
      Effort S/M. Source: E.1#1. — shipped in commits `17fd9d63`
      (8 backends) + Reddit follow-up.
- [x] **A.4** File-split `clients/client/src/types.rs` (1891 lines)
      into `types/{backend, auth, server, file, message, user,
      notification, moderation, voice, command}.rs` with a `mod.rs`
      re-export shim. Zero behaviour change. Effort S/M. Source: A.1#4.
      — shipped in commit `5647a26f`.

### Phase B — LSP fixes + capability single-source-of-truth (~3 days)

- [x] **B.1** Land the four LSP fixes from B.2:
  - `send_reply_message` default → `Err(NotSupported)` (silent data-loss bug). HIGH.
  - `get_pinned_messages` default → `Err(NotSupported)` (mirror `set_message_pinned`). MED.
  - HN `get_presence` `Ok(Offline)` → `Ok(PresenceStatus::Unknown)` + new variant. MED.
  - Teams `get_user` `NotFound` → `NotSupported`. LOW.
  — shipped in commit `1034d3e2`.
- [x] **B.2** Killed `capabilities_for_slug` table — single source of
      truth via runtime `ClientManager::backend_capabilities` registry.
      Deleted `pack_f_capability_gates` parity test + 8 per-backend
      `tests/capabilities.rs` parity tests. 22 UI consumer sites
      migrated to `client_manager.peek().capabilities_for_slug(slug)`.
      — shipped in commit `68ee5243`.
- [x] **B.3** Closed as moot — the `forbid_backend_slug_match` lint
      matches `match X.as_str() { ... }` ladders, NOT the 6
      `if backend == "<slug>"` quirk gates the survey flagged. No
      inline-allowlist needed; nothing to migrate.

### Phase C — Demo triplication + chat_view virtualization split (~5 days)

These can land in parallel; they touch disjoint trees.

- [x] **C.1** Extract `DemoFlavour` trait/struct so `DemoClient`,
      `DemoClient2`, `DemoClient3` collapse into one parametric
      `DemoClientGeneric<F>` with type aliases. `lib.rs` 1867→514 LOC (73%),
      single `ClientBackend` impl body, all 7 tests pass. Effort M.
      Source: B.1.2. — shipped in commit `fc621b7b`.
- [x] **C.2** Pulled `crates/core/src/ui/account/common/chat_view.rs`'s
      virtualization engine into `chat_view/virtualization.rs` (~290 LOC,
      10 pure functions + 2 data structs + ~5 constants). chat_view.rs
      file→directory transition is now in place, de-risking Phase F.
      Effort M. Source: A.1#1. — shipped in commit `74adc8e4`.

### Phase D — `ClientManager::with_backend` + `use_view_resource` (~3 days)

- [x] **D.1** Implemented `ClientManager::with_backend(account_id, async fn)`,
      `with_backend_timeout`, and `with_backend_for_server(server_id, async fn)`
      that resolve the backend, apply `read_with_timeout(BACKEND_TIMEOUT)`,
      and return `ClientResult<R>` so callers can `?`. 43 duplicate
      sites migrated across `crates/core/src/ui/`. Sites that couldn't
      cleanly migrate (multi-call write-lock bodies) had their `read()`
      flipped to `peek()` to close hang-class #7. Source: D.1.1. —
      shipped in commit `c23d41a3`.
- [x] **D.2** `ViewQuery` trait + `use_view_resource<Q>(query)` hook
      shipped in `crates/core/src/ui/client_ui/use_view_resource.rs`,
      bundling backend-resolve + 5s timeout via D.1's `with_backend`.
      **11 of 27 sites migrated** to typed query structs (channel
      view, account overview, list-body first page + row detail,
      split-body rows, server roles/bans/modlog, context menu items,
      composer buttons, message actions). 16 remained on raw
      `use_resource` because they have structural reasons not to
      migrate: Signal reads inside the closure (reactive dep tracking),
      `try_consume_context` patterns, `Option<String>` account_id with
      `Ok(vec![])` fallback (treating "no account" as success not
      error), multi-call closure bodies with side-effects on ChatData,
      `get_backend_for_server` resolution path, intentional
      `client_manager.read()` subscriptions. Source: D.1.4. — shipped
      in commit `c1b6e031`.
- [x] **D.3** Trait-default approach (cleaner than a derive macro):
      `ClientBackend` gained `fn settings_storage(&self)` with a
      static-empty default, and `get_setting_value`/`set_setting_value`
      now have working defaults that delegate to it. Each of the 11
      plugin crates' identical 12-line {get,set} pair collapsed to a
      single 3-line override returning `&self.settings_storage`. Net
      -270 LOC. Source: D.4.3. — shipped in commit `cd1809bc`.

### Phase E — Lint-gate consolidation (~3 days) — shipped in commits `ec586e02` (E.3-E.5) + `6eb5e73b` (E.1+E.2)

- [x] **E.1** Created `crates/lint-gate-rules` lib crate. Moved every
      `crates/lint-gate/build/<rule>.rs` into
      `lint-gate-rules/src/<rule>.rs`; `build.rs` is now a ~120-line
      thin driver calling `poly_lint_gate_rules`. Tests live next to
      rules (29 unit tests passing). Dropped the 1416-line mirror in
      `lint-gate/src/lib.rs` (now 7 lines). Source: E.2 ("recommended
      consolidation"). — shipped in commit `6eb5e73b`.
- [x] **E.2** Ported 9 of 10 `tools/scripts/forbid-*.sh` scripts into
      `lint-gate-rules` modules using a shared `allowlist::Loader`.
      Bash scripts deleted after parity. CI workflow now runs a single
      `cargo check -p poly-lint-gate` step in place of 9 bash invocations.
      791 pre-existing violations grandfathered in `baseline.json`.
      `forbid-ui-only-persona-action.sh` (Q.3 stub) handled in E.5.
      — shipped in commit `6eb5e73b`.
- [x] **E.3** Retired the dylint duplication. Deleted
      `tools/lints/poly-lints/` entirely + workspace exclude entry +
      CI dylint job + `dylint.toml`. The lint-gate-rules Rust scanners
      cover the same rules with allowlist semantics. — shipped in
      commit `ec586e02`.
- [x] **E.4** Documented `crates/lint-gate/baseline.json` via
      `crates/lint-gate/baseline.md` (when it would be the right tool
      and how to populate it). File itself untouched. — shipped in
      commit `ec586e02`.
- [x] **E.5** Deleted the 16-line stub `forbid-ui-only-persona-action.sh`
      and removed the corresponding Q.3 step from
      `.github/workflows/lint-test.yml`. Phase Q.3 stays open in the
      persona-quality-gates plan; will land as a real Rust scanner once
      the UI surface ships. — shipped in commit `ec586e02`.

### Phase F — `chat_view.rs` full split (~1 week, parallelisable) — shipped in commits `059eaf55`, `f8ca9e1f`, `d11abf25`, this commit

Phase F unlocks all the per-effect work (`use_history_state_effect` etc.)
that hang-class plans already name as the canonical examples but couldn't
isolate.

- [x] **F.1** Move sub-components (`ChatHeaderActions` 347 lines,
      `ChatUtilityRail` 205 lines) to `chat_view/{header,utility_rail}.rs`. — shipped in commit `059eaf55`
- [x] **F.2** Move composer + search-filter helpers to
      `chat_view/{composer_helpers,search_filter}.rs`. — shipped in commit `f8ca9e1f`
- [x] **F.3** Move all 12 `use_*_effect` hooks to
      `chat_view/effects/<name>.rs` (one file per effect — they ARE the
      "reasons to change" that SRP cares about). Source: A.1#1. — shipped in commit `d11abf25`
- [x] **F.4** Pull `ChatViewSignals` (35 fields) and
      `ChatViewMarkupCtx` (70 fields) into `chat_view/signals.rs` and
      `chat_view/markup_ctx.rs` respectively. Constructor
      `build_chat_view_markup_ctx` moved to markup_ctx.rs as the
      "from_signals" builder. — shipped in this commit
- [x] **F.5** Orchestrator finalized: `mod.rs` re-exports removed;
      effects now import from `super::super::signals::ChatViewSignals`
      and `super::super::markup_ctx::ChatViewMarkupCtx`. Baseline
      regenerated twice. Both WASM + native builds pass. mod.rs final
      size: 3996 LOC (vs 4396 before F.4+F.5). Note: mod.rs exceeds
      the ≤500 LOC orchestrator goal — the remaining ~3500 LOC are
      render helpers (`render_message_list`, `render_message_row`,
      `render_message_input_area`, `MsgContextMenuOverlay`, etc.).
      Proposed follow-up: extract render helpers to
      `chat_view/message_list.rs`, `chat_view/composer.rs`,
      `chat_view/context_menu_overlay.rs` (separate task). — shipped in this commit

### Phase G — `AppState` + `ChatData` slice signals (~2 weeks, phased) — ALL shipped (G.1–G.6f)

The biggest structural move. Land per-slice, not big-bang. Each
sub-step is independently shippable; later steps benefit from earlier
ones (smaller signal subscriptions = less re-render churn).

- [x] **G.1** Land the in-flight `context_menu_stack` migration that
      `state.rs:585-590` already calls out. Delete the 8 scalar
      `*_context_menu` fields once every site uses the stack. Source:
      D.4.5. — shipped (commit ID pending jj describe)
- [x] **G.2** Extract `BatchedSignal<VoiceState>` from `ChatData`
      (80 sites, self-contained). Voice writes stop re-rendering chat
      list. Source: D.3. — shipped (commit ID: 1057ee20)
- [x] **G.3** Extract `BatchedSignal<DragState>` from `ChatData`
      (61 sites, transient, very write-heavy during drag). Source: D.3.
      — shipped in commit `a89999f4`
- [x] **G.4** Add `ChatAction` enum + `ChatData::apply()`. Migrate the
      23 manual-clear sites (`cd.channels.clear(); cd.messages.clear();
      cd.members.clear()`) to typed actions. Source: D.1.3.
      — shipped in commit (see below)
- [x] **G.5** Split remaining `AppState` into `NavState`, `UiLayout`,
      `UiOverlays`, `UserPrefs`. Source: D.2 split table. — shipped in commit (see jj describe below)
- [x] **G.6** Split remaining `ChatData` into `ChatLists`,
      `ChatViewState`, `AccountSessions`. Add by-id `HashMap` shadows
      so the 9 linear `iter().find` lookups become O(1). Source: D.3.
      Broken into sub-steps below — original G.6 scope was understated;
      first two passes shipped infrastructure + ~30% of consumer
      migration but left `ChatData` intact with all 21 fields and the
      new sub-signals partially-empty at runtime.
  - [x] **G.6a** Define `ChatLists` / `ChatViewState` / `AccountSessions`
        structs in `crates/core/src/state/{chat_lists,chat_view_state,account_sessions}.rs`.
        Add by-id `HashMap` shadows + invariant-preserving setters
        (`set_servers` / `push_server` / `server_by_id` etc.) on
        `ChatLists`. Wire 3 `provide_context` calls in `ui.rs`. Move
        `ChatAction::apply` to `ChatViewState::apply`; keep
        `ChatData::apply` as backward-compat shim. Migrated single-bucket
        consumers (content_social, account_bar, discord_forum_view,
        dm_user_sidebar, user_sidebar, voice_view, code_explorer,
        electron_titlebar, context_menu, profile, notifications,
        overview_subpages, create_server, direct_call, direct_call_overlay,
        dm_context_menu, user_profile_modal, routes). — shipped in
        commit `efee96e5`.
  - [x] **G.6b** Fix multi-bucket consumer cascade
        (`direct_call.rs` reads both `dm_channels` from `ChatLists`
        AND `account_sessions` from `AccountSessions`; callers updated).
        Both targets build clean. — shipped in commit `2928682e`.
  - [x] **G.6c** Writer-completeness migration. Every
        `chat_data.X = ...` / `chat_data.batch(|cd| cd.X.push(...))`
        site reroutes to the matching sub-signal. **MUST** use
        invariant-preserving setters when writing through `ChatLists`
        (otherwise `_by_id` shadows desync from canonical Vecs).
        Greppable proof: zero `chat_data\.batch` and zero `cd\.<field>
        =` patterns for any field that lives in a sub-state.
        — shipped in earlier session (commits span G.6c-G.6d work)
  - [x] **G.6d** Reader migration. ~128 remaining
        `chat_data: BatchedSignal<ChatData>` parameters or
        `use_context` sites reroute to the actual sub-signals each
        consumer needs. Drop the `chat_data` prop from ~30 component
        signatures. — shipped across multiple commits in previous session
  - [x] **G.6e** Eliminate `ChatData` as provided context. Drop
        `provide_context(BatchedSignal::new(ChatData::default()))` from
        `ui.rs`, remove `ChatData` from all UI component imports, remove
        from run_reset_flow. ChatData struct kept in state/chat_data.rs
        for test infra + user_color helper; `pub use` kept accordingly.
        — shipped in commit `862fb8745f7b`
  - [x] **G.6f** By-id audit. Migrated 3 key `iter().find()` linear
        lookups to `*_by_id` helpers: `favorites_sidebar.rs` servers
        lookup (now `server_by_id`), `CategorySection` channels lookup
        (now `channel_by_id` via context), `current_channel_unread_count`
        dm_channels lookup (now `dm_channel_by_id`). Remaining 4 lookups
        in `load_server_data_internal` operate on pre-commit freshly-loaded
        backend data (not yet in ChatLists) — cannot use by-id helpers.
        `account_server_bar.rs` operates on a local `&[Server]` slice.
        — shipped in commit (see G.6f commit below)

### Phase G.6g — vestigial `ChatData` type deletion — shipped in commit `a81777cc`

### Phase G.6h — action stub migration — shipped in commit `55a7821a`

- [x] **G.6h.1** `VoiceBannerAction::ToggleMute` — toggle `voice_connection.is_muted` via `try_consume_context::<BatchedSignal<VoiceState>>()`.
- [x] **G.6h.2** `VoiceBannerAction::ToggleDeafen` — toggle `voice_connection.is_deafened` via VoiceState context.
- [x] **G.6h.3** `VoiceBannerAction::Disconnect` — call `disconnect_active_call(voice_state)` via VoiceState context.
- [x] **G.6h.4** `VoiceBannerAction::GoToChannel` — read `VoiceState.voice_connection` via `.peek()`, push `Route::DmChat` or `Route::ServerChat` via `cx.navigator`, remember scroll position via `NavState` context.
- [x] **G.6h.5** `VoiceBannerAction::SwapHeldCall` — call `swap_to_first_held_call(voice_state)` via VoiceState context.
- [x] **G.6h.6** `NotificationsViewAction::SetFilter` — component-local `Signal<NotificationMenuFilter>` is unreachable from the action system without providing it as context; implemented as documented no-op; inline `onclick` handler remains authoritative.
- [x] **G.6h.7** `NotificationsViewAction::MarkAllRead` — clear all notifications for the active account via `ChatLists` context; cannot honour current filter (component-local).
- [x] **G.6h.8** `NotificationsViewAction::AcceptFriendRequest` / `DenyFriendRequest` — derive `account_id` from `ChatLists.notifications` lookup by notif_id; optimistically remove notification; spawn `handle_friend_request_action` async task. Both use `ChatLists` + `ClientManager` context.
- [x] **G.6h.9** `NotificationsViewAction::AcceptServerInvite` / `Dismiss` — remove notification from `ChatLists` via context; deeper server-join flow remains in inline component handlers.
- [x] **G.6h.10** `ActionCx` NOT extended (no new fields) — `try_consume_context` is the right boundary since VoiceState and ChatLists are not universally available at all `apply` call sites.
- [x] **G.6h.11** Regenerated `baseline.json` twice (after voice_banner.rs and notifications.rs edits shifted pre-existing lint violations to new line numbers). Final count: 867 entries.
- [x] **G.6h.12** Verify: `cargo check -p poly-core` clean; `cargo check --target wasm32-unknown-unknown` clean; `cargo test -p poly-core` 257 passed, 0 failed. Zero `todo!("phase-E:")` in `voice_banner.rs` and `notifications.rs`.

- [x] **G.6g.1** Audit `\bChatData\b` across `crates/` — confirmed live sites were `state.rs` re-export, `state/chat_data.rs` itself, and test helper in `account_restore.rs`. All comment-only references confirmed harmless.
- [x] **G.6g.2** Migrate test fixture: dropped `BatchedSignal<ChatData>` from `make_signals_in_runtime` return tuple in `account_restore.rs`; updated all 4 test call sites to pass 3-tuple `(cm, cl, as_)` matching the public `restore_native_accounts` signature.
- [x] **G.6g.3** Delete `ChatData` struct + `impl ChatData` from `state/chat_data.rs`; updated module-level doc. Removed `pub use chat_data::ChatData` from `state.rs`. `mod chat_data` retained for `user_color`, `backend_badge`, `format_file_size`, `VoiceMediaSettings` helpers still consumed by ~15 call sites.
- [x] **G.6g.4** Verify: `cargo check -p poly-core` clean (106 warnings, 0 errors); `cargo check --target wasm32-unknown-unknown` clean; `cargo test -p poly-core` 257 passed, 0 failed. Zero `\bChatData\b` type references in `crates/` (all remaining occurrences are in comments or `todo!` string literals fixed in G.6h).

### Phase G.6i — orphaned use_context preamble cleanup — shipped in commit `c68b8d91`

Deleted 102 orphaned `let X = use_context()` (and equivalent) bindings left by the G.6a–G.6h ChatData → sub-signal migrations. Files touched span 46 files across `crates/core/src/ui/`. For function parameters (e.g. `app_state` in `load_older_messages`, `chat_lists` in `activate_existing_or_new_call`), the parameter was removed from both the function signature and all call sites.

- [x] **G.6i.1** Delete orphaned `use_context` preambles in `thread_view.rs` (13 bindings across 5 functions: `ViewThreadButton`, `ActiveThreadsBar`, `ThreadList`, `ThreadSummaryPanel`, `ThreadsView`).
- [x] **G.6i.2** Delete orphaned binding in `header.rs` (chat_view): remove `app_state` param from `render_search_tab_button` + fix 2 call sites in `header.rs` and `mod.rs`.
- [x] **G.6i.3** Delete orphaned bindings in `context_menu/menus.rs` and `context_menu/host.rs` (3 lines).
- [x] **G.6i.4** Delete orphaned bindings in `chat_view/mod.rs` (7 sites: unused param in `load_older_messages`, `load_newer_messages`, `maybe_send_real_typing`; local bindings in `render_mobile_chat_header_right_toggle`, `TypingModeButton`, emoji closure, line 2307).
- [x] **G.6i.5** Delete orphaned bindings in `main_layout.rs` (4 bindings: `app_state`, `nav`, `ui_layout`, `user_prefs`).
- [x] **G.6i.6** Delete orphaned bindings in `channel_list.rs` (26 bindings across 8 functions).
- [x] **G.6i.7** Delete orphaned bindings in remaining 40 files (account_server_bar, account_switcher, avatar_context_menu, channel_context_menu, dm_context_menu, utility_rail, effects/mobile_side_column, effects/composer_focus, effects/search_messages, effects/pinned_messages, effects/command_preload, conversation_search_view, direct_call_overlay, discord_forum_view, dm_user_sidebar, forum_view, friends_panel, media_viewer, new_conversation_view, overview_sidebar, user_sidebar, voice_view, account/server/context_menu, account/server/settings, client_ui/sidebar/communities, client_ui/sidebar/feed, client_ui/sidebar/repo_tree, client_ui/view/card_body, client_ui/view/list_body, client_ui/view/split_body, dialogs/ban_member, dialogs/edit_channel, dialogs/kick_member, dialogs/timeout_member, routes.rs, search.rs, settings/general.rs, settings.rs, voice_banner.rs, direct_call.rs).
- [x] **G.6i.8** Verify: zero `unused variable: \`(app_state|nav|ui_overlays|ui_layout|user_prefs)\`` warnings; zero Rust compile errors; zero new unused import warnings.

### Phase G.6j — flip VoiceBanner + Notifications inline handlers through action system — shipped in commit `873cb1af`

All inline onclick handlers in `voice_banner.rs` and `notifications.rs` now route through
`dispatch_action!`, making `VoiceBannerAction::apply` / `NotificationsViewAction::apply` the
single authoritative execution path. Also fixes `dispatch_action!` macro to use `.batch()`
instead of the broken `.write()`.

- [x] **G.6j.1** Fix `dispatch_action!` macro in `actions.rs`: replace `$state.write()` (deprecated, panics on `BatchedSignal`) with `$state.batch(move |state| { ... })`.
- [x] **G.6j.2** Update `SetFilter::apply` from no-op to real: consume `Signal<NotificationMenuFilter>` from context and call `.set(filter)`.
- [x] **G.6j.3** Add `use_context_provider(|| kind_filter)` in `NotificationsView` to make the `kind_filter` signal reachable from `SetFilter::apply`.
- [x] **G.6j.4** Flip `VoiceBannerControls` (ToggleMute, ToggleDeafen, Disconnect, SwapHeldCall): remove `voice_state` prop, add `app_state`/`nav_state` via `use_context()`, replace 4 inline handlers with `dispatch_action!`.
- [x] **G.6j.5** Flip `VoiceBannerChannelLink` (GoToChannel): remove `app_state` prop + 6 props now derived inside `apply`, add `app_state`/`nav_state` via `use_context()`, replace inline onclick with `dispatch_action!(VoiceBannerAction::GoToChannel, ...)`.
- [x] **G.6j.6** Update `VoiceBanner` call sites: no longer passes `voice_state` to `VoiceBannerControls`; `VoiceBannerChannelLink` now only receives `channel_name`, `server_name`, `connection_kind`.
- [x] **G.6j.7** Flip `NotificationsView::MarkAllRead` onclick to `dispatch_action!`; add `app_state`/`nav_state` use_context bindings.
- [x] **G.6j.8** Flip all `NotificationItemContent` inline handlers (AcceptFriendRequest, DenyFriendRequest, AcceptServerInvite, Dismiss, Reauth): remove `chat_lists`/`client_manager` props, add `app_state`/`nav_state` use_context, replace inline bodies with `dispatch_action!`.
- [x] **G.6j.9** Update `NotificationList`: remove `chat_lists`/`client_manager` use_context bindings and stop passing them as props to `NotificationItemContent`.
- [x] **G.6j.10** Verify: 0 Rust compiler errors, 0 Rust warnings; no new poly-lint-gate violations beyond pre-existing baseline; all 5 `VoiceBannerAction` variants and 7 `NotificationsViewAction` variants route exclusively through `apply()`.

### Phase G.6l — `ForumViewAction` `todo!()` panic fix — shipped in commit `c6b67d22`

Surfaced after G.6k shipped: navigating to a github (or any forum-layout) channel route triggered `RuntimeError: unreachable` in WASM. Root cause was a `todo!("phase-G.5: …")` stub in `ForumViewAction::apply()` at `forum_view.rs:153` that G.6h's sweep missed (it only matched on `phase-E:` labels, not `phase-G.5:`).

- [x] **G.6l.1** Implement `ForumViewAction::ShowPosts` / `ShowComments` `apply()` body via `try_consume_context::<BatchedSignal<UserPrefs>>()` + `user_prefs.batch(|s| s.view_filter = next)` — same shape as G.6h's voice/notifications fixes.
- [x] **G.6l.2** Cfg-gate `account_sessions` in `direct_call_overlay.rs:34` to `#[cfg(not(target_arch = "wasm32"))]` — last residual G.6k unused-var on wasm, missed by an earlier sed when nav_state's cfg-gate insert shifted line numbers.
- [x] **G.6l.3** Verify live in poly-web: navigate to `/github/github.com/gh-github.com-887/channels/gh-63975513` — forum view renders cleanly, Open/Closed tab clicks succeed, 0 console messages.

### Phase H — `ClientBackend` capability sub-traits (~1 month, long-horizon) — design locked, ready for code

Gated on Phase B (capabilities single-source) + Phase D (with_backend)
landing. Trait split must mirror WIT interface boundaries (C.1.3
caveat).

#### Design decisions (locked 2026-05-05)

- **Dispatch shape (was H.4):** parent trait exposes capability accessors `fn as_forum(&self) -> Option<&dyn ForumBackend> { None }`, etc. Backends opt-in by overriding to `Some(self)`. `BackendCapabilities` bitflags remain the runtime hint for "is this accessor going to return Some" — accessors are the type-system enforcement. No `Any` downcasts, no enum-of-backends (would break `plugin-host`'s runtime-loaded WIT plugins).
- **Trait granularity:** WIT 1:1 mapping — `poly:client/{content-policy, code-repo, forum, threads, moderation, social, dms}` → Rust traits with the same names. Plugin-host's existing `from_wit_backend_capabilities` translation stays a single source of truth; no Rust↔WIT drift layer.
- **Fate of `ClientBackend` itself:** **delete entirely.** UI storage becomes `Box<dyn IsBackend>` where `IsBackend` is a thin parent trait — `Send + Sync`, slug/version/capabilities, basic auth (login/logout/get_account). All other ~85 methods live exclusively on capability sub-traits, reachable only through accessors. Big upfront churn (every `with_backend` call site in `crates/core/src/ui/` — ~58 sites — has to capability-check), but eliminates the kitchen-sink trait completely. No deprecation half-life.
- **`Err(NotSupported)` replacement:** sub-traits have **no default impls.** Capability is opt-in by impl-presence. Backend doesn't `impl ForumBackend` → `as_forum()` returns `None` → UI hits the type-system check, can't even *call* the methods. Eliminates all 52 `NotSupported` stubs as part of the migration.
- **Migration order:** bottom-up — H.1 → H.2 → H.3. H.1 is the proof-of-pattern (zero implementers = pure deletion); H.2 validates dispatch on real plugins; H.3 only after the pattern is hardened.

#### Sub-steps

- [x] **H.0** Carve out `IsBackend` parent trait (`Send + Sync`, slug, version, capabilities, login/logout/get_account, the capability accessors). Move it to `clients/client/src/lib.rs` alongside the soon-to-be-deleted `ClientBackend`. Effort S. shipped in commit `b757a081554f`
- [ ] **H.1** Carve out `ContentPolicy` (3 methods, 0 implementers).
      Pure deletion — no migration burden. Defines the dispatch pattern. Effort S. Source: C.2.1.
- [ ] **H.2** Carve out `CodeRepoBackend` + `ForumBackend` +
      `ThreadsBackend` (7 methods total, 4 implementers). Update
      `code_explorer.rs` + forum routes to take the narrower trait
      via `as_forum()` / `as_code_repo()` / `as_threads()`. Validates
      dispatch on real plugins. Effort M. Source: C.2.2.
- [ ] **H.3** Carve out `Moderation` + `SocialGraph` + `DmsAndGroups`
      (38 methods, ~43% of trait). Touches every backend; do
      one-trait-at-a-time. Effort L per trait. Source: C.2.3.
- [ ] **H.4** Delete `ClientBackend` trait — all method moved out by H.1-H.3. Migrate all `Box<dyn ClientBackend>` storage sites to `Box<dyn IsBackend>`. Migrate all `~58` `with_backend` UI call sites to capability-gate via accessors. Final cleanup; ratchets the pattern.

### Phase I — Routes.rs decomposition (~1 week, after Phase H starts)

- [ ] **I.1** Macro-derive `route_account_id` + `route_variant_name`
      from `#[connected(...)]` metadata already on each Route variant.
      `sync_route_to_app_state` stays hand-written (intentionally
      divergent bodies — see B.1.4 caveats). Effort L. Source: B.1.4.
- [ ] **I.2** Split `routes.rs` (2515 lines) per-domain — DM routes,
      server routes, forum routes, settings routes, agent routes,
      moderation routes — each module taking only the capability traits
      it needs (DIP win). Effort L. Source: C.3.3.

### Phase J — `mcp/chat-mcp/src/{tools,memory}.rs` split (~3 days)

- [ ] **J.1** Split `tools.rs` (4081 lines, 80+ handlers) into
      `tools/` sub-modules per CLAUDE.md's persona-handler family.
      Source: A.1#3.
- [ ] **J.2** Split `memory.rs` (2695 lines, 8 SQLite schemas, 57
      methods) into `memory/{facts, chat_notes, drafts, persona/, …}`.
      Convert the 9-11 param functions (`update_persona`,
      `query_persona_audit`) to builder structs. Source: A.1#5.

---

## Don't bother (explicit "leave alone" list)

Surveyed and intentionally rejected so future work doesn't re-litigate:

- **`clients/demo/src/data.rs` (5806 lines)** — fixture data; the
  function-per-fixture granularity is correct. Source: A.3.
- **`mcp/chat-mcp/src/tools.rs` lines 213-1399 — JSON tool schema** —
  one document by design (single grep verifies parity). Move *file*
  per J.1 but don't fragment further. Source: A.3.
- **`crates/core/src/ui/routes.rs` per-`#[component]` route adapters** —
  none over the 150-line cap; splitting per-route would multiply file
  count without breaking responsibilities. Source: A.3.
- **`View`, `ConnectionStatus`, `ContainerLabelForm`,
  `SidebarLayoutKind`, `ActionOutcome` enums** — closed sets by design
  with single-purpose dispatch. Replacing with traits would obscure
  the closed-set intent and lose compile-time exhaustiveness checks.
  Source: B.3.1-B.3.4.
- **`container_label_key` slug fallback table** — i18n fallback chain
  that should stay centralized until plugins ship FTL bundles.
  Source: B.3.5.
- **6 `if backend == "<slug>"` UI quirk gates** — single-line
  single-backend special cases; promoting each to a capability bit is
  worse than the literal. Source: B.3.6.
- **`UiSurface` 16-method block** — D9 design says "every backend
  required to implement, return empty for nothing-to-contribute." Keep
  in core trait. Source: C.4.1.
- **`MenuItem` / `SidebarItem` flat-with-`parent_id`** — WIT forbids
  recursive records; flat shape is required by the wire format.
  Source: C.4.2.
- **`event_stream` single-stream design** — host has unified dispatch
  loop; per-capability streams would multiply consumer threads for no
  win. Source: C.4.3.
- **`BatchedSignal` itself** — closed by hang-class lints; do NOT
  wrap further. Source: D.4.4 + C.4.8.
- **`RouteSynced<T>` per-field** — locks writes to the router via
  compile-time gating; current shape is already tighter than freeform.
  Source: D.4.2.
- **`SettingsStorageCell`** — already shared in `clients/client`; the
  per-backend-instance ownership is intentional for cross-backend
  isolation. Only the get/set 12-line shim is duplicated; address via
  a derive macro (D.3 above) if bandwidth. Source: D.4.3.
- **Per-row settings `Signal<bool>`** — independent toggles, NOT a
  state machine. Bundling would over-broaden subscribers. Source: D.4.1.
- **`clients/reddit/tests/fixtures/` cross-included by
  `servers/test-reddit/`** — intentional ground-truth sharing. Add a
  README noting the dual-include but don't restructure. Source: E.4.
- **Per-client `[lib] crate-type = ["cdylib", "rlib"]` + cfg-gated
  WASM deps** — load-bearing dual-build infrastructure. Source: E.4.
- **`TEST_HARNESS.md` per-crate `cargo test` listing** — workaround
  for native/WASM dep conflict, not bloat. Source: E.4.
- **`route_graph.rs` "never-grandfather" rule** — orphan routes are
  dead code; baseline support would defeat the lint. Source: E.4.

---

## Cross-references to existing in-flight plans

This survey *augments*, not replaces, these existing plans:

- `plan-component-lints.md` — covers the rsx!-cap that drives Phase F.
  This plan's Phase F is the missing existing-offender migration.
- `plan-batched-signal.md` — Phase 4 ("other hot-path signals") aligns
  with this plan's Phase G.
- `plan-peek-vs-read.md` — `with_backend` helper (Phase D.1) migrates
  `client_manager.read()` to `.peek()` automatically.
- `plan-backend-read-timeout.md` — `with_backend` closes the remaining
  raw-`client_manager.read().get_backend()` surface the existing lint
  cannot reach.
- `plan-use-spawn-once.md` — `use_view_resource` (Phase D.2) bundles
  spawn-once + reactive-effect + timeout + backend-resolve into one hook.
- `plan-context-menu-quality-control.md` (DONE) — Phase G.1 finishes
  the in-flight `context_menu_stack` migration the existing plan
  started.

---

## Risks + mitigations

- **God-signal split (Phase G) is the highest-blast-radius work.**
  Land per-slice, not big-bang. Each slice is independently shippable
  via a per-backend / per-component migration. Hang-class lints
  remain green throughout.
- **Trait split (Phase H) is constrained by WIT.** The Rust split
  must mirror the existing `messenger-plugin.wit` interface
  boundaries. Touching this without a matching WIT change breaks the
  WASM plugin contract. Keep `Box<dyn ClientBackend>` as the storage
  type; capability traits are dispatch-time downcasts.
- **`Signal<crate::path::T>` blind-spot regression** — see memory
  `feedback_signal_migration_namespace_blind`. Any signal-type
  migration must catch fully-qualified Signal types via grep before
  declaring a phase done.
- **Demo backend triplication (Phase C.1)** — the existing tests
  instantiate concrete `DemoClient2` / `DemoClient3` types directly.
  Migration must update test sites in lockstep.

---

## Suggested execution model

- **Phases A + E** can be assigned to sonnet worktree agents in
  parallel — disjoint files, mechanical work, low-risk.
- **Phases B + C + D** orchestrator-led with sonnet helpers for the
  sed-style migrations. Risk on `with_backend` is real (touches 48
  sites that vary in subtle ways).
- **Phases F + G + H** orchestrator-led with sonnet workers per
  sub-step. Each phase has a clear "land one sub-step, prove the shape,
  then parallelise" structure baked in.
- **Phase I + J** can ride along after H starts — they unblock once
  the trait splits exist.

Estimated total effort if everything ships: **~2-3 months elapsed**
with one engineer + sonnet agents, much less with multiple worktrees.
The mechanical wins (Phase A) ship in a couple of days and yield
immediate PR-friction relief.

---

## Status: 🧭 SURVEY — ready for prioritisation

Pick which phases to greenlight. Recommend starting with Phase A as a
pure cost-free win, then Phase B for the LSP fixes (silent data-loss
bug fix in `send_reply_message` is genuinely user-visible), then
Phase C as the warm-up to the larger structural moves.
