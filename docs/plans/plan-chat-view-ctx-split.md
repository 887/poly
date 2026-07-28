# Plan: Split `ChatViewMarkupCtx` — the 74-field god-struct cloned 18x per render

## Status: 📋 PLANNED

> Opened 2026-07-28 from the multi-agent review fan-out. SOLID pre-merge gate
> **item 4 (ISP)** and **item 6 (no god-objects)** — verified against the tree,
> not asserted.

---

## Why this plan exists

Measured, current tree:

| Fact | Evidence |
|---|---|
| `ChatViewMarkupCtx` has **74** `pub(super)` fields | `crates/core/src/ui/account/common/chat_view/markup_ctx.rs:30`; `grep -c 'pub(super)'` = 76, minus the struct decl and `build_chat_view_markup_ctx` |
| `ChatViewSignals` has **40** fields | `.../chat_view/signals.rs:19`; same counting method on 42 |
| The struct is `#[derive(Clone)]` | `.../markup_ctx.rs:29` |
| It is cloned **18 times** per ChatView render | `grep -rn 'ctx.clone()' .../chat_view/` → layout.rs ×8, composer.rs ×4, scroll.rs ×4, message_row.rs ×1, mod.rs ×1 |
| The chat_view module is **6857 lines** across 14 files | `wc -l .../chat_view/*.rs` |

Every plain (non-`#[component]`) render fn in the module takes the whole
74-field struct **by value** and clones it before delegating. Because the derive
is a deep clone, each of those 18 clones copies every contained
`Option<String>`, `Vec<Message>`, `Vec<MessageSearchHit>`, `Vec<SearchFilterOption>`,
`Channel` and `User`.

**ISP (item 4)** — the reasoning the gate applies to god-traits applies
identically to a parameter object every consumer is forced to accept:
`render_chat_header_info` needs roughly 6 of the 74 fields;
`render_message_input_row` needs roughly 11. No consumer needs more than a
fifth of it.

**Item 6 (no god-objects)** — by field count this is the largest type in
`poly-core`.

It also **defeats the G.2/G.3/G.5/G.6 signal splits** documented at
`crates/core/src/ui.rs:1768-1796`. Those splits exist to narrow subscriber sets;
`build_chat_view_markup_ctx` (`.../markup_ctx.rs:116`) then re-reads all of them
into one blob that every render fn receives, so the narrowing buys nothing
downstream.

**Failure scenario:** any state change that re-renders ChatView — a single
incoming message, one keystroke in the composer via `message_input`, a
typing-indicator tick — walks `build_chat_view_markup_ctx` to materialise all 74
fields and then performs 18 deep clones including the `Vec<MessageSearchHit>` /
`Vec<SearchFilterOption>` allocations, before any DOM diffing starts. On WASM's
single thread that is the render-cost floor for every keystroke in a busy
channel. This is adjacent to (not identical with) hang class #1: it does not
schedule extra reactive passes, it makes each pass more expensive.

---

## Ground rules for every phase

- **Land region-by-region so each step compiles.** Do not attempt one big
  rewrite; `ChatViewMarkupCtx` stays in place shrinking phase by phase, and is
  deleted only in Phase F.
- **Pass the new context structs by reference (`&HeaderCtx`), never by value.**
  Eliminating the clones is the point; a by-value split reproduces the bug with
  smaller structs.
- **Line-count neutrality matters here.** `crates/core/` is a lint-gate crate
  with a line-keyed baseline (`crates/lint-gate/baseline.json`) and a line-keyed
  allowlist (`tools/scripts/render-time-read-allowlist.txt`). A prior agent
  desynced 33 baseline entries with an unrelated reflow. Expect to regenerate;
  see `plan-lint-gate-integrity.md` Phase A, and **never** regen as a way of
  absorbing a genuinely new violation.
- Fields shared by all regions (`nav`, `ui_layout`, `client_manager` — all
  `Copy` `BatchedSignal`s) go in a tiny `ChatViewCore` passed alongside, not
  duplicated into each region struct.

---

## Phase A — Introduce `ChatViewCore` and the region-ctx module, no call-site changes

- [ ] **A.1** Create `crates/core/src/ui/account/common/chat_view/ctx/mod.rs`
  with `pub(super) struct ChatViewCore` holding only the `Copy`
  `BatchedSignal` fields currently at `.../markup_ctx.rs:31-37`
  (`nav`, `ui_layout`, `ui_overlays`, `client_manager`, `chat_lists`,
  `chat_view_state`, `voice_state`). Derive `Copy, Clone` — it is signal handles
  only, so cloning it is free and by-value passing stays fine.
- [ ] **A.2** Add `fn core(&self) -> ChatViewCore` to `ChatViewMarkupCtx` so both
  representations coexist during the migration.
- [ ] **A.3** Add `build_core(signals: &ChatViewSignals) -> ChatViewCore`
  alongside `build_chat_view_markup_ctx` (`.../markup_ctx.rs:116`).
- [ ] **A.4** `cargo check -p poly-core` green; lands as a no-op.

## Phase B — `HeaderCtx` (smallest region, proves the pattern)

- [ ] **B.1** Define `pub(super) struct HeaderCtx` in `.../chat_view/ctx/header.rs`:
  channel / server / DM identity fields, `utility_panel`, and the
  `header_actions_*` fields. Derive `Clone` only if a consumer genuinely needs
  an owned copy — prefer not to.
- [ ] **B.2** `fn build_header_ctx(signals: &ChatViewSignals) -> HeaderCtx`.
- [ ] **B.3** Convert `crates/core/src/ui/account/common/chat_view/header.rs`
  (572 lines) and the header region of `layout.rs:173-527` to take
  `(&ChatViewCore, &HeaderCtx)`. Delete the corresponding `ctx.clone()` sites in
  `layout.rs`.
- [ ] **B.4** Remove the now-unused header fields from `ChatViewMarkupCtx`.
- [ ] **B.5** `cargo check -p poly-core` + `cargo check -p poly-core --target wasm32-unknown-unknown` green.

## Phase C — `ComposerCtx`

- [ ] **C.1** Define `ComposerCtx` in `.../chat_view/ctx/composer.rs`:
  `message_input`, `pending_attachments`, `reply_target`, the `command_*`
  fields, `show_input_emoji`, `markdown_enabled`.
- [ ] **C.2** `fn build_composer_ctx(signals: &ChatViewSignals) -> ComposerCtx`.
- [ ] **C.3** Convert `.../chat_view/composer.rs` (836 lines) and
  `composer_helpers.rs` (231 lines) to `(&ChatViewCore, &ComposerCtx)`; delete
  the 4 `ctx.clone()` sites in `composer.rs`.
- [ ] **C.4** Remove the migrated fields from `ChatViewMarkupCtx`.
- [ ] **C.5** Both `cargo check` targets green. This is the phase that should
  show up as a per-keystroke win — note any before/after observation in this
  file.

## Phase D — `MessageListCtx`

- [ ] **D.1** Define `MessageListCtx` in `.../chat_view/ctx/message_list.rs`:
  `messages`, `virtual_window`, history state, `unread_*`, `msg_context_menu`.
  This is the region carrying the largest `Vec`s, so it must be
  reference-passed everywhere.
- [ ] **D.2** `fn build_message_list_ctx(signals: &ChatViewSignals) -> MessageListCtx`.
- [ ] **D.3** Convert `.../chat_view/scroll.rs` (729 lines),
  `message_row.rs` (318) and `virtualization.rs` (290); delete the 4
  `ctx.clone()` sites in `scroll.rs` and the 1 in `message_row.rs`.
- [ ] **D.4** Remove the migrated fields from `ChatViewMarkupCtx`.
- [ ] **D.5** Both `cargo check` targets green.

## Phase E — `SideColumnCtx`

- [ ] **E.1** Define `SideColumnCtx` in `.../chat_view/ctx/side_column.rs`:
  `search_*`, `pinned_*`, `threads_*`.
- [ ] **E.2** `fn build_side_column_ctx(signals: &ChatViewSignals) -> SideColumnCtx`.
- [ ] **E.3** Convert `.../chat_view/search_filter.rs` (485 lines) and
  `utility_rail.rs` (466 lines).
- [ ] **E.4** Remove the migrated fields from `ChatViewMarkupCtx`.

## Phase F — Delete `ChatViewMarkupCtx` and re-check the gate

- [ ] **F.1** `render_chat_layout_shell` (`.../chat_view/layout.rs`) builds all
  four region contexts plus `ChatViewCore` once and passes each by reference.
  Delete the remaining `ctx.clone()` in `mod.rs` and `layout.rs`.
- [ ] **F.2** Delete `ChatViewMarkupCtx`, `build_chat_view_markup_ctx` and
  `markup_ctx.rs`; move `ctx/` to be the module's only context source.
- [ ] **F.3** Assert the win mechanically: add a `crates/core/tests/` guard that
  fails if any file under `.../chat_view/` contains `ctx.clone()`, and a
  compile-time `const _: () = assert!(size_of::<MessageListCtx>() <= …)` style
  check is **not** required — the clone-site guard is the load-bearing one.
- [ ] **F.4** Re-state the SOLID gate for the split, item by item, in this file:
  SRP / OCP / LSP / ISP / DIP / no-god-objects / test-seams / pure-plugins, with
  one sentence of evidence each. PARTIAL must name the item and reason.

## Phase G — Verify (QA gate — iterate until a clean round)

- [ ] **G.1** `cargo clippy --workspace -- -D warnings` clean.
- [ ] **G.2** `cargo test --workspace` green.
- [ ] **G.3** `cargo check -p poly-lint-gate` rc=0. Compare the (path, rule,
  detail) multiset before/after — **zero entries added**. Line shifts in this
  module will desync line-keyed entries; re-point the allowlist entries rather
  than changing code to satisfy the scanner.
- [ ] **G.4** Live walk via the poly-web MCP against the already-running dev
  server (hot-reload, **not** `hard_kill`): open a busy channel, type in the
  composer, scroll history, open search, open a thread. No console errors, no
  visual regression.
- [ ] **G.5** Re-run G.1–G.4 after the last fix; tick DONE only off a round that
  surfaced nothing new.
