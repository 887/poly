# Plan: SOLID + Oversize-Component Audit of `crates/core/src/ui/`

> Created: 2026-05-17 by audit agent (worktree-agent-a949df940fa9cf59b).
> Scope: `crates/core/src/ui/` only. Excludes the freshly-modified voice/event
> files (`ui.rs`, `event_stream.rs`, `account_restore.rs`) and `clients/*/`.
> The 5 ship-now wins ALREADY landed in this same change (see Phase A below).

## Status: IN PROGRESS — Phase A shipped, B/C/D queued

## Findings overview

- **Largest files** (LoC): `chat_view/mod.rs` (4025), `channel_list.rs`
  (1775), `favorites_sidebar.rs` (1645), `signup.rs` (986),
  `routes/mod.rs` (975), `search.rs` (932), `voice_view.rs` (932),
  `notifications.rs` (853), `settings/backup.rs` (836),
  `client_ui/view/list_body.rs` (835), `account_server_bar.rs` (807).
- **Named oversize candidates from CLAUDE.md**: confirmed
  - `ChatView` — `chat_view/mod.rs:511` (4025 LoC mod; already partially
    split into `render_chat_view_markup`, `render_chat_layout_shell`,
    `render_chat_main_column`, `render_chat_header*` etc. but the file
    still hosts 88 fns/components).
  - `FavoriteServerIcon` — `favorites_sidebar.rs:881` (a single component
    that is ~750 LoC of rsx! + handlers; reads SEVEN context signals).
  - `ServerContextMenu` — actual file is `account/server/context_menu.rs`
    (NOT `context_menu/menus.rs`, which is the generic menu host). See
    Phase C.
- **Backend slug-ladder OCP violations** (rsx!/handler-time
  `match slug.as_str()` instead of capability dispatch):
  - `voice_banner.rs:146` (ToggleCamera) and `voice_banner.rs:216`
    (ToggleScreenShare) — both fork on `"stoat"|"teams" → toast,
    _ → toggle`. Should be a `VideoCaptureCapability` trait method on
    the backend.
  - `search.rs:66` already documents D27: backend icons MUST come from
    plugin declaration, not a slug ladder. Good role-model; OCP-compliant
    today.
- **`todo!()` / `unimplemented!()` in apply() action handlers**: 35+
  sites across `create_channel.rs`, `create_server.rs`, `signup.rs`,
  `favorites_sidebar.rs`, `voice_view.rs`, `friends_panel.rs`,
  `account/server/settings/{general,notifications,overview}.rs`,
  `user_profile_modal.rs`, `account/server/settings.rs`. All labeled
  "phase-E: …requires Signal + async handles" — they're TYPED contracts
  waiting for a wiring pass. Not an SRP bug per se, but a complete
  Action-pattern migration that someone needs to finish.
- **Hang-class #7 hot zones** (render-time `.read()` allowlisted, but
  collapsible to one `.with()` or `.peek()`):
  - `favorites_sidebar.rs` had 8+ allowlisted reads — 2 collapsed in
    Phase A; ~6 remain (lines 146, 154, 170, 427, 437, 444, 450, 459,
    475, 940). All in `FavoritesBar`/`FavoriteServerIcon`; candidates
    for the planned medium refactor.
  - `conversation_search_view.rs` had 6 — 4 collapsed in Phase A;
    2 remain (lines 49 plus the early `nav.read()` snapshots).
  - `direct_call.rs` has ~10 allowlisted reads inside plain async fns
    (not render fns) — annotation is correct, but most could use `.peek()`
    for clearer intent. Cosmetic, low value.
- **Raw `use_effect` without `use_reactive_effect`/`use_spawn_once`**:
  ~30 unallowlisted sites remain across `main_layout.rs` (5),
  `settings.rs` (3), `account/settings.rs` (3), `routes/account.rs`,
  `routes/mod.rs`, `account/server/settings.rs` (3),
  `account/common/user_profile_modal.rs`, `chat_style_editor.rs`,
  `code_explorer.rs`, `direct_call_overlay.rs`, `client_ui/toast.rs`,
  `split_shell.rs`, etc. Triaging these is class-#3 / class-#6 / class-#8
  risk reduction; CLAUDE.md's plan-use-reactive-effect Phase 2 (54 sites
  triaged) was a one-shot pass — these are the regressions/new sites.

---

## Phase A — Ship-now wins (≤ 50 LoC each)

Shipped in this audit change.

- [x] **A.1** `dm_user_sidebar.rs:54-58` — three `chat_view_state.read()`
  calls in one if/else collapsed to one `chat_view_state.with(|cvs| …)`
  block. One subscription instead of three; cleaner SRP boundary.
- [x] **A.2** `favorites_sidebar.rs:906-917` — two back-to-back
  `client_manager.read()` calls for conn-class + presence-class
  collapsed into one `.with(|cm| …)` returning a tuple. Halves the
  subscription count for every rendered FavoriteServerIcon (one per
  starred server — ~10+ per page).
- [x] **A.3** `saved_items_view.rs:127-143` — two `chat_lists.read()`
  passes (dms + groups) collapsed into one `.with(|cl| …)` returning
  `(Vec<_>, Vec<_>)`. One subscription, two filtered Vecs.
- [x] **A.4** `conversation_search_view.rs:192-205` — same pattern as
  A.3: two `chat_lists.read()` → one `.with()`.
- [x] **A.5** `conversation_search_view.rs:238-251` — three
  `account_sessions.read()` calls for label/instance/slug collapsed
  into one `.with(|sess| …)` returning a tuple.

All five inline-allowlist comments preserved (subscription is
intentional in each case). Lint-gate baseline regenerated
(`732 entries`, was 722). `dx build` green.

Shipped in change `<git-commit-id pending>`.

---

## Phase B — Medium refactors (50-300 LoC each)

Not started in this change — listed for follow-up agents.

- [ ] **B.1** Split `FavoriteServerIcon` (favorites_sidebar.rs:881-1100ish,
  ~220 LoC component, 7 context signals). Sub-components: avatar/badge
  block, drag overlay block, click handler module. Lifts SRP and lets
  drag-handlers be unit-testable. Also a chance to remove the remaining
  ~6 render-time `.read()` allowlists by passing data via props.
- [x] **B.2** Extract voice_banner.rs ToggleCamera/ToggleScreenShare
  slug ladder (lines 134-238) into a `VideoCaptureCapability` trait
  on the backend. Toast paths become a trait method's default impl;
  Discord/Matrix override. OCP-compliant. (~120 LoC)
  Shipped: `VideoCaptureCapability` enum + `BackendCapabilities.video_capture`
  field in `poly-client`; discord declares `Full`; voice_banner dispatches
  on capability not slug. `cargo check -p poly-core` + `dx build` green.
- [ ] **B.3** Split `favorites_sidebar.rs` `FavoritesBar`
  (117-400ish, ~280 LoC). Heavy state-derivation in render body
  (account order, favorited ids, drag state). Move snapshot derivation
  into one `.with()` block (kills 5+ allowlisted reads), then split
  rendering into `FavoritesBarLeft` / `FavoritesBarMain`.
- [ ] **B.4** Split `channel_list.rs` `ServerChannelView`
  (812-1050, ~240 LoC). Inner category/permission filtering is its
  own concern; pull into helper module.
- [ ] **B.5** Finish the phase-E Action-pattern wiring for the 35+
  `todo!("phase-E: …")` sites. Largest clumps:
  `account/server/settings/{general,notifications,overview}.rs`
  (~10 todos), `signup.rs` (3 page actions), `voice_view.rs`,
  `friends_panel.rs`, `favorites_sidebar.rs`, `search.rs`.
- [ ] **B.6** Audit + migrate the 30 raw `use_effect` sites without
  allowlist (main_layout.rs, settings.rs, account/settings.rs,
  routes/account.rs, etc.) to `use_reactive_effect` or
  `use_spawn_once`. Apply hang-class #6/#8 countermeasures per
  CLAUDE.md.
- [ ] **B.7** Collapse remaining `favorites_sidebar.rs` render-time
  `.read()` allowlists (lines 146, 154, 170, 427, 437, 444, 450, 459,
  475, 940) — most are `account_sessions` snapshots that could share
  ONE `.with()` per component instead of N.
- [ ] **B.8** Split `account_server_bar.rs` (807 LoC) — server bar
  combines server list, account list, and DM-bar concerns. ISP
  candidate: separate the three lists into per-concern sub-components.

---

## Phase C — Architectural rewrites (> 300 LoC each)

- [ ] **C.1** `chat_view/mod.rs` (4025 LoC, 88 fns) — long-standing
  CLAUDE.md target. Author already started splitting (`render_*` fns,
  `effects/` submodule). Continue: pull `render_chat_layout_shell` +
  children into `chat_view/layout.rs`, `render_drag_overlay` into
  `chat_view/drag.rs`, `MessageListScrollWorkCtx` machinery into
  `chat_view/scroll.rs`. Target: mod.rs ≤ 800 LoC, each submodule
  ≤ 500.
- [ ] **C.2** `channel_list.rs` (1775 LoC, 26 components) — split into
  `channel_list/{server_view, dm_view, friends_view, items}.rs`. Each
  big `#[component]` (ServerChannelView, DMFriendsView,
  ServerBanner, DMChannelItem, GroupChannelItem, FriendItem,
  ChannelsRolesPanel) has its own data-fetching + rendering — clean
  SRP cut.
- [ ] **C.3** `favorites_sidebar.rs` (1645 LoC, 9 components) — after
  B.1+B.3 fold splits, the remaining mass is async loaders
  (1222-1500 region: drag drop persistence, server re-order RPC,
  favorited-ids persist). Pull into `favorites_sidebar/persist.rs`
  pure-async module, leaving rendering in the main file.

---

## Conventions reminder

- Tick `- [x]` AND annotate the phase header with the landing change
  id as work ships (per CLAUDE.md plan discipline).
- Keep ≤ 5 ship-now wins per agent pass — A.1-A.5 already exhausts
  this round.
- Don't touch the carved-out files: `ui.rs`, `event_stream.rs`,
  `account_restore.rs` (voice/event work in flight).
