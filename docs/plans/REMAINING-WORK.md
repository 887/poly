# Remaining Work — Lint-Gate Plan Trio

> Last updated: 2026-04-17
> Pick up point for the next session. See each plan's own file for full context;
> this is the short "what's left" index.

## plan-component-lints.md ✅
Fully done. Nothing left.

## plan-connected-routes-static-check.md 🟡

**Shipped:** Phase A infra + Phase B backfill + baseline drain (§5.3.1). Baseline
is empty; `E-ROUTE-001` / `E-ROUTE-002` fail `cargo check` today.

**Pending:**
- §5.3.2 — remove the `regen-baseline` warn-downgrade path from
  `crates/lint-gate/build.rs` (route-graph still falls back to warn behind the
  feature). Small edit once §7 lands.
- §5.3.3 — **add a bare `navigator().push(Route::...)` ban scan** to
  `crates/lint-gate/build/`. Today there is nothing stopping someone from
  bypassing `nav!` / `Link` — the loophole is wide open. Mirror the
  `allow_ban.rs` pattern: structural line scan, emit `cargo::error=` for every
  occurrence outside a `nav!` expansion or a `Link { to: ... }` RSX attr.
- §7.1–7.5 — test coverage. Unit tests for `#[connected]`, trybuild
  compile-fail fixtures (orphan / unreachable / wrong-target / unimplemented /
  multiple-entry-points), an integration test crate `crates/ui-macros-tests/`,
  an optional debug-assertions runtime coverage counter in
  `sync_route_to_app_state`, and a haiku-run harness entry.

## plan-context-menu-quality-control.md 🟡

**Shipped:** lint-gate coverage scan (§3.1.2), root guard on `.main-layout`
(§4.5.1), 318-annotation reclassification (§5.1.3), mobile z-index fix (not in
plan — orthogonal bug surfaced while testing).

**Pending — runtime infrastructure (Phase A, §2.2 + §2.3 + §4):**
- §2.2 / §3.1.1 — **real proc-macro expansion.** `crates/ui-macros/src/lib.rs:44`
  is still `pub fn context_menu(_attr, item) -> TokenStream { item }` — a no-op
  pass-through. Needs to (a) parse the arg into one of `Foo`/`None`/`allow_default`/`inherit`,
  (b) inject a `#[linkme::distributed_slice(CTX_MENU_COVERAGE)]` entry, (c)
  emit the appropriate DOM wrapper for each variant (or no wrapper for
  `None`/`inherit`/`allow_default`).
- §2.3 — **`ContextMenuFor<Props>` trait + stack refactor.** New file
  `crates/core/src/ui/context_menu/mod.rs` with the trait; refactor
  `ServerContextMenu` / `ChannelContextMenu` / `MsgContextMenuOverlay` to impl
  it against their props. Replace `AppState.context_menu` +
  `AppState.channel_context_menu` with one `context_menu_stack: Vec<ActiveContextMenu>`.
- §3.1.4 — add `#[diagnostic::on_unimplemented]` on `ContextMenuFor` so
  `#[context_menu(Foo)]` where `Foo` doesn't impl the trait gives a readable error.
- §4.1–4.2 — `ContextMenuStack` host in `crates/core/src/ui/context_menu/host.rs`,
  mounted once at `MainLayout` level. Desktop anchors (cursor + submenu-anchored-below).
- §4.3 — **mobile center-overlay variant.** Detect mobile via
  `runtime_mobile_ui_active()`; full-screen fixed overlay + 70% scrim, centered
  card with `max-height: 70vh`. Dismiss on scrim-click / back
  (`hashchange`-based like `UserProfileModal`) / horizontal swipe / Escape.
  Submenu = new overlay pushed on top, parent stays underneath dimmed. Scroll
  lock on body.
- §4.4 — extract the long-press state machine out of `channel_list.rs:1283-1330`
  into `crates/core/src/ui/context_menu/long_press.rs`. Haptic feedback
  (`navigator.vibrate(10)` best-effort). Do NOT install long-press on
  `allow_default` surfaces.

**Pending — Phase 0 inventory (§1.1.1–1.1.4):**
- `scripts/audit_context_menus.sh` — CSV of `<file, component, has_oncontextmenu, ...>`
- Store classification as `docs/plans/context-menu-coverage.toml`
- "Currently-bleeding" list (surfaces where the wrong menu fires today)
- Per-forum-backend menu extras (`clients/hackernews`, `clients/lemmy`,
  `clients/github`, `clients/forgejo` have no `context_menu.rs` today)

**Pending — classification follow-ups (3 in-code TODOs):**
- `crates/core/src/ui/account/common/forum_view.rs:653` — `ForumPostCard`
  needs a `ForumPostContextMenu`
- `crates/core/src/ui/account/common/forum_view.rs:778` — `ForumComment` same
- `crates/core/src/ui/account/common/dm_user_sidebar.rs:91` — `DmMemberRow`
  needs a `UserRowContextMenu`

**Pending — tests (§6.1.1–6.1.7):**
- `linkme` slice parse smoke test in `crates/core/tests/context_menu.rs`
- Per-menu dispatch unit tests (`ContextMenuFor::Ctx` + rendered items by i18n key)
- `insta` snapshot of rendered menu HTML (single, submenu, allow-default)
- MCP UI desktop tests — right-click each annotated surface, assert
  `.context-menu` present, native menu absent
- MCP UI mobile test — `set_viewport(390,844)` + CDP touch events
- Forum-specific regression — right-click Lemmy post ≠ "Invite" / "Server Boost"
- `TEST_HARNESS.md` §8 "context-menu smoke"

**Pending — Phase D deny (§5.1.4):**
- Re-verify once Phase A macro expansion and Phase C classification are real.
  Baseline is already empty; it just needs the macro/trait work to give the
  classifications runtime teeth.

**Pending — ergonomics (§5.2):**
- `cargo check --features regen-baseline` workspace re-gen path
- Editor snippet docs in `crates/core/agents.md`

## Suggested order

1. **Connected-routes §5.3.3** (bare-push ban) — small, self-contained, closes
   the loophole. Do before anyone adds new `navigator().push` callsites.
2. **Context-menu Phase A runtime** (§2.2 + §2.3 + §4.1) — the proc-macro +
   trait + stack host. Unlocks everything else. Single contiguous work block;
   cannot be parallelized across subagents.
3. **Context-menu §4.3 mobile overlay + §4.4 long-press** — depend on (2).
4. **Three in-code TODOs** — depend on (2) since they need the stack API.
5. **Tests** — last; need (2) + (3) to be meaningful.

## Risks to flag before restarting

- The pre-existing hand-rolled `ServerContextMenu` / `ChannelContextMenu` /
  `MsgContextMenuOverlay` are the only menus with runtime behavior today.
  Phase A runtime must keep those working during the migration; don't delete
  `AppState.context_menu` / `.channel_context_menu` until new stack is
  functionally equivalent.
- `linkme` on `wasm32-unknown-unknown` under LTO is the documented primary
  path; `inventory` is the fallback (plan §3.2). If `linkme` misbehaves,
  that's where to go.
