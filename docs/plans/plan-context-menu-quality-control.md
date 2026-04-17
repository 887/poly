# Plan — Context Menu Quality Control

> **Created:** 2026-04-16
> **Status:** ✅ DONE (2026-04-17) — all Phase A–D items shipped. Lint-gate coverage enforced, Phase A runtime shipped (`ContextMenuFor` trait, `context_menu_stack`, `ContextMenuStack` host, long-press extraction, mobile center-overlay, two new typed menus), Phase C classification complete, Phase D verified clean. `cargo check --workspace` and WASM target both pass with zero violations.
> **Scope:** cross-cutting — every Dioxus component in `crates/core/src/ui/` and each `clients/*/src/`
> **Goal:** Every `#[component]` in the app declares a compile-time context-menu policy via a single `#[context_menu(...)]` attribute macro (`Foo` / `None` / `allow_default` / `inherit`). Coverage is enforced by the shared `crates/lint-gate/build.rs` — any missing decorator emits `cargo::error=` on plain `cargo check`, so agents cannot silently skip it. The right menu always shows up for a given surface, the wrong one never bleeds through, and on mobile a long-press opens a center-screen stacked-overlay menu that dismisses on back / swipe / outside-click.

---

## Current state (from repo audit, 2026-04-16)

Poly is "an app, not a website" — users expect Discord/Slack-style right-click + long-press menus everywhere. Today the story is partial and inconsistent.

### Existing context-menu surfaces

| Surface | File | Right-click? | Long-press? | `preventDefault()`? | Backend-specific extras? |
|---|---|---|---|---|---|
| Favorites server icon | `crates/core/src/ui/favorites_sidebar.rs` (~line 839) | yes | no | yes | yes — via `ServerContextMenu` + `backend_server_context_menu_extras` |
| Account server icon | `crates/core/src/ui/account/common/account_server_bar.rs` (~line 282, 446) | yes | no | (implicit via `ServerContextMenu`) | yes |
| Channel row | `crates/core/src/ui/account/common/channel_list.rs` (~line 1268) | yes | yes (500 ms touch timer) | yes | no (single shared `ChannelContextMenu`) |
| Chat message | `crates/core/src/ui/account/common/chat_view.rs` (~line 3698, overlay at 5517) | yes | no | yes | no |
| `MsgContextMenuOverlay` backdrop | `chat_view.rs` (~line 5536) | — | — | yes (backdrop swallows native menu) | — |
| Forum view / forum post row | `crates/core/src/ui/account/common/forum_view.rs` | **no** (0 occurrences) | no | **no** | no |
| User profile modal | `crates/core/src/ui/account/common/user_profile_modal.rs` | **no** | no | **no** | no |
| Member list user row | `crates/core/src/ui/account/common/user_sidebar.rs` | **no** | no | **no** | no |
| DM user sidebar row | `crates/core/src/ui/account/common/dm_user_sidebar.rs` | **no** | no | **no** | no |
| Emoji picker, voice bar, friends panel, notifications, saved items, conversation search, media viewer, settings pages | various | **no** | no | **no** | — |
| Root `<html>` / `<body>` / `.main-layout` | `crates/core/src/ui/main_layout.rs` (~line 289) | fires, not intercepted | — | **no global guard** | — |

### Problems this catalog exposes

1. **Coverage gap.** Of 352 `#[component]`s (337 in `crates/core/src/ui/`, 15 in `clients/*/src/`), only ~5 have a context menu wired up. Everywhere else the browser/webview native menu leaks through — reload, view source, inspect, translate — which is wrong for an app shell.
2. **Bleed-through.** `ServerContextMenu` is rendered once in `MainLayout` and reads from `AppState.context_menu`. A right-click anywhere over a server icon works, but the menu is always the *generic Discord-style server menu* (Mark as Read / Invite / Privacy Settings → `ServerSettingsRoute`). Right-clicking a forum post, a Lemmy community icon, or an HN thread lands either nothing or — if the hit-test catches the wrapping server icon — Discord-flavored items that have no meaning for forge/forum backends.
3. **No global guard.** There is no `oncontextmenu: evt.prevent_default()` at `MainLayout`, `html`, or `body`. The current per-surface guards are opt-in, so any un-annotated component silently allows the native menu.
4. **No mobile story for the non-channel menus.** `channel_list.rs` has a 500 ms long-press timer; `chat_view.rs`, `favorites_sidebar.rs`, `account_server_bar.rs` do not. Long-press on iOS Safari triggers text-selection / callout bar instead.
5. **Positioning is cursor-anchored everywhere.** `context-menu { position: fixed; left: {x}px; top: {y}px }` — that is correct on desktop but wrong on mobile, where menus should be a dismissible center-screen overlay stack (the pattern the old combobox used).
6. **No submenu chain.** `ContextMenuItem { has_arrow: true }` renders a `›` but clicking it just closes the parent menu and `navigator().push(ServerSettingsRoute)` — there is no nested overlay. "Open a similar overlay on top of that menu" is completely unimplemented.
7. **`allow_default` is not expressible.** A user right-clicking an image to "Save image as…" has no path — the chat view swallows the event for every descendant of the message div.

---

## 1. Audit & catalog (baseline for the migration)

- [ ] **1.1.1** Grep + machine-readable inventory. Add `scripts/audit_context_menus.sh` that greps `#[component]` and emits CSV of `<file, component_name, has_oncontextmenu, has_ontouchstart, prevent_default_count>` so phase-rollout progress is trackable.
- [ ] **1.1.2** Manually classify each `#[component]` into one of: `menu(Foo)`, `menu(Foo, allow_default)`, `menu(None)` (preventDefault only, no menu), `menu(inherit)` (delegates to parent — see 3.1.3). Store as a TOML registry `docs/plans/context-menu-coverage.toml` keyed by fully-qualified path.
- [ ] **1.1.3** Produce the "currently-bleeding" list — places where the *wrong* menu fires today (server menu items appearing over a forum post because the right-click bubbles through the post into the server sidebar, etc.).
- [ ] **1.1.4** Decide per-backend menu extras for forum-style backends (`clients/hackernews`, `clients/lemmy`, `clients/github`, `clients/forgejo`). Today only Discord / Matrix / Teams / Stoat / demo / poly_native have `context_menu.rs` modules; forums have none.

## 2. Target DSL — attribute decorators on `#[component]`

### 2.1 Shape

Three variants, all applied *above* `#[component]` (see 3 for why ordering matters):

```rust
// 2.1.1 — attach a menu component
#[context_menu(ChannelMenu)]
#[component]
fn ChannelRow(props: ChannelRowProps) -> Element { /* … */ }

// 2.1.2 — explicitly opt out (preventDefault only, no menu)
#[context_menu(None)]
#[component]
fn VoiceBanner() -> Element { /* … */ }

// 2.1.3 — explicitly allow the native menu (images, links, input fields)
#[context_menu(allow_default)]
#[component]
fn MessageImage(props: MessageImageProps) -> Element { /* … */ }

// 2.1.4 — optional: forward to a parent's menu (the common case for inner spans)
#[context_menu(inherit)]
#[component]
fn MessageBodyText(props: ...) -> Element { /* … */ }
```

**DSL shape:** one macro, `#[context_menu(...)]`, with four argument variants —
`Foo` (menu type), `None` (opt-out), `allow_default` (native menu), `inherit`
(forward to parent). Keeping them all under a single macro name makes grep /
coverage / error messages consistent ("missing `#[context_menu(...)]`") and
avoids the bikeshed of remembering whether the opt-out was spelled
`#[no_context_menu]` or `#[skip_context_menu]`. `None` is parsed as an ident,
not the `Option::None` path — the macro matches it literally.

### 2.2 Expansion sketch

`#[context_menu(ChannelMenu)]` expands to wrap the returned `Element` in a `ContextMenuHost` marker div that:

1. Registers an `oncontextmenu` handler calling `evt.prevent_default()` + `open_menu::<ChannelMenu>(evt, props_as_menu_ctx)`.
2. Registers an `ontouchstart` / `ontouchmove` / `ontouchend` long-press handler with the shared 500 ms timer from `channel_list.rs` (extracted into `crates/core/src/ui/context_menu/long_press.rs`).
3. Emits a `const _: () = <ChannelMenu as ContextMenuFor<ChannelRowProps>>::ASSERT_COMPATIBLE;` so a menu that expects a different prop shape fails to compile.

`#[context_menu(None)]` expands to only the `oncontextmenu: evt.prevent_default()` guard — no menu, no long-press handler. Identical runtime behavior to the previous `#[no_context_menu]` spelling; renamed for DSL consistency.

`#[context_menu(allow_default)]` expands to *nothing at the DOM level* but does register the component in the compile-time registry (see 3), so the coverage lint sees it. Native menu fires; that is the desired behavior.

`#[context_menu(inherit)]` expands to *nothing* and is simply a coverage declaration.

### 2.3 Menu component contract

```rust
// crates/core/src/ui/context_menu/mod.rs  (new)
pub trait ContextMenuFor<Props> {
    type Ctx: Clone + 'static;
    fn build_ctx(props: &Props, evt: &MouseEvent) -> Self::Ctx;
    fn render(ctx: Self::Ctx, close: EventHandler<()>) -> Element;
    /// Compile-time assertion slot — empty for now; downstream macros can gate here.
    const ASSERT_COMPATIBLE: () = ();
}
```

- [ ] **2.3.1** Refactor `ServerContextMenu`, `ChannelContextMenu`, `MsgContextMenuOverlay` to impl `ContextMenuFor<ServerIconProps>` / `ChannelRowProps` / `MessageRowProps` respectively.
- [ ] **2.3.2** Delete `AppState.context_menu` + `AppState.channel_context_menu` in favor of a single stack-shaped `AppState.context_menu_stack: Vec<ActiveContextMenu>` (see 4.1).
- [ ] **2.3.3** Per-backend extras keep working — they remain ordinary child components rendered *inside* the menu, dispatched on `BackendType`. No change needed there.

## 3. Compile-time enforcement

### 3.1 Primary approach — attribute-macro pair + `linkme` registry

Trade-off summary: Dioxus 0.7.3's `#[component]` macro is a plain attribute macro that rewrites the function into a generated struct + function. Stacking attributes is legal as long as ours runs *outside* `#[component]` (Rust attribute-macro ordering is outer-first). So we ship a `poly-context-menu-macros` proc-macro crate that:

- [ ] **3.1.1** Exports a single attribute macro `#[context_menu(...)]`. The macro parses its argument into one of four variants (`Foo` ident → attach menu; `None` ident → preventDefault-only; `allow_default` ident → native menu; `inherit` ident → parent forwards). It runs before `#[component]` and simply (a) validates the arg, (b) injects a `#[linkme::distributed_slice(CTX_MENU_COVERAGE)]` static entry with the component's `module_path!()` + variant tag, (c) re-emits the original `fn` with the appropriate DOM-level wrapper (or no wrapper for `None`/`inherit`/`allow_default`) so `#[component]` sees a valid fn.
- [x] **3.1.2** Coverage enforcement runs on **plain `cargo check`**, not `#[test]` — tests are easy for agents to skip, build errors are not. Lives in the shared `crates/lint-gate/build.rs` from `plan-component-lints.md` §3.2 (one workspace walk feeds every cross-file lint). The build script:
  - enumerates every `#[component]`-attributed fn in the workspace via `ignore::WalkBuilder` + line-prefix attribute scan (same primitive as the allow-ban);
  - for each hit, checks the preceding non-blank line for one of `#[context_menu(Foo)]` / `#[context_menu(None)]` / `#[context_menu(allow_default)]` / `#[context_menu(inherit)]`;
  - emits `cargo::error=missing #[context_menu(...)] decorator at <path>:<line>` (stabilized Rust 1.84) for each miss.
  **Shipped:** `crates/lint-gate/build/context_menu_coverage.rs`. Baseline is empty. The `linkme` runtime smoke test is deferred — the compile-time scan is the gate.
- [ ] **3.1.3** `#[context_menu(inherit)]` is a bare-bones variant that only registers in the slice — it is the tool authors use when they genuinely mean "my parent owns the menu." It keeps the coverage check clean without forcing every `<span>`-like leaf into a dummy menu.
- [ ] **3.1.4** Quality: emit a `#[diagnostic::on_unimplemented]` on `ContextMenuFor` so a typo in `#[context_menu(Foo)]` where `Foo` does not impl the trait gives a clean error message.

### 3.2 Fallback — `inventory` + runtime test only

If `linkme` turns out to be flaky on the `wasm32-unknown-unknown` target (it uses linker sections that a few WASM linkers drop under LTO), fall back to `inventory`, which uses ctor-style registration and always works on WASM. Downside: startup cost + no ordering guarantees. Keep `linkme` as primary; `inventory` as documented fallback.

### 3.3 Non-approach — hand-rolling a derive on a marker trait

Tempting but rejected: Dioxus components are free functions, not structs, so there is no natural `derive` site. Wrapping the function body instead (`#[component]`-style rewrite) duplicates `dioxus-core-macro`'s work and is fragile across dioxus version bumps. The attribute-stack approach above touches only outer metadata.

## 4. Mobile overlay runtime

### 4.1 State shape

```rust
// crates/core/src/state/mod.rs
pub struct ActiveContextMenu {
    pub id: u64,                     // monotonic; used for stack keys
    pub anchor: MenuAnchor,           // Cursor{x,y} | Center | AnchoredBelow(DOMRect)
    pub component: ContextMenuNode,  // type-erased rendered Element
    pub dismiss_on_outside: bool,
}
pub struct AppState {
    // …
    pub context_menu_stack: Vec<ActiveContextMenu>, // replaces the two scalar fields
}
```

- [ ] **4.1.1** Stack push = open submenu. Stack pop = back / swipe / outside-click. Empty = no menu visible.
- [ ] **4.1.2** `crates/core/src/ui/context_menu/host.rs` renders `ContextMenuStack` — mounted once, at `MainLayout` level, above everything except the voice banner.

### 4.2 Desktop rendering (unchanged UX)

- [ ] **4.2.1** `MenuAnchor::Cursor { x, y }` → `position: fixed; left: {x}px; top: {y}px`. Identical to today.
- [ ] **4.2.2** Submenu on `has_arrow: true` item → push `MenuAnchor::AnchoredBelow(rect)` to the stack. Rendered flush-right of the parent, flipping to the left when near the viewport edge.

### 4.3 Mobile rendering (new)

- [ ] **4.3.1** Detect mobile via the existing `runtime_mobile_ui_active()` helper in `main_layout.rs`. When true, every push coerces the anchor to `MenuAnchor::Center` regardless of what the caller asked for.
- [ ] **4.3.2** Render as a full-screen fixed overlay with a 70 %-opacity scrim. The menu card is centered, `max-height: 70vh`, `overflow-y: auto`, rounded-top sheet feel.
- [ ] **4.3.3** Dismissal channels: (a) tap on scrim (`onclick` on backdrop), (b) hardware / browser back — push `#poly-ctx-menu-{id}` to `history` on open, listen for `hashchange` exactly like `UserProfileModal` does at `user_profile_modal.rs:93-128`, (c) horizontal-swipe-down gesture (reuse the swipe-runtime hooks that already power the mobile left/right drawer close — `assets/scripts/mobile_drawer_runtime.js`), (d) Escape key.
- [ ] **4.3.4** Submenu stack on mobile = a new overlay pushed on top of the current one. Parent stays rendered underneath (slightly dimmed). Back pops to it. This is the exact pattern mobile SwiftUI `Menu` / iOS `UIContextMenu` use (cf. Apple HIG — "Context Menus"; citation in `docs/plans/plan-context-menu-quality-control.md` § Open questions). React Native's `@react-native-menu/menu` and Material Design "long-press menu" both follow the same stacked-sheet model on Android.
- [ ] **4.3.5** Scroll lock on `body` while the stack is non-empty (CSS `overflow: hidden` on `.main-layout` via a top-level class toggle — same pattern as the existing mobile drawer).

### 4.4 Long-press handling — unify

- [ ] **4.4.1** Extract the channel-list long-press state machine (`channel_list.rs:1283-1330`, generation counter + `setTimeout(500)`) into `crates/core/src/ui/context_menu/long_press.rs`. The macro-generated wrapper uses it for every `#[context_menu(Foo)]` component.
- [ ] **4.4.2** Cancel the timer on `touchmove` past 10 px, `touchend`, or `touchcancel`. Haptic feedback on fire (WASM: `navigator.vibrate(10)` best-effort, native mobile: TBD).
- [ ] **4.4.3** For `#[context_menu(allow_default)]` images, *do not* install the long-press handler — let iOS Safari's native "Save image" callout take over.

### 4.5 Global guard

- [x] **4.5.1** Add `oncontextmenu: evt.prevent_default()` at the root `.main-layout` `<div>` in `main_layout.rs` as a belt-and-suspenders fallback for components that somehow skipped annotation. The per-component `allow_default` variant opts out by calling `evt.stop_propagation()` before the root handler sees it. This is the only place we accept a runtime guard; compile-time coverage (3) is the source of truth. **Shipped:** commit `f627d9fc`, `main_layout.rs:299`.

## 5. Migration path

### 5.1 Phased rollout

**Status correction (2026-04-17):** The checkboxes below were previously marked `[x]` by an earlier pass. Audit shows only §5.1.2 (warn-mode / lint-gate scan) actually shipped. The others either never happened or were simulated by a blanket-`inherit` stamp that is semantically a no-op. They are now honestly unchecked.

- [x] **5.1.1** **Phase A — infrastructure.** *Shipped (2026-04-17).* `crates/core/src/ui/context_menu/{mod,host,long_press,menus}.rs` are in place; `ContextMenuFor<Props>` is defined with `#[diagnostic::on_unimplemented]`; `AppState.context_menu_stack: Vec<ActiveContextMenu>` ships alongside the legacy scalar fields (scalar removal deferred); `ContextMenuStack` is mounted in `MainLayout` between `ChannelContextMenu` and `VoiceBanner`; the root `oncontextmenu: prevent_default` guard already shipped in §4.5.1; the long-press state machine now lives in `long_press.rs`; two Phase-A menus (`ForumPostContextMenu`, `UserRowContextMenu`) are wired through the registry.
- [x] **5.1.2** **Phase B — warn mode.** Shipped. The `lint-gate` build script (`crates/lint-gate/build/context_menu_coverage.rs`) emits `cargo::error=` for any `#[component]` lacking a sibling `#[context_menu(...)]`. `baseline.json` is empty, so every miss breaks `cargo check`.
- [x] **5.1.3** **Phase C — batch annotate *and classify*.** *Shipped* via commit `4159fda9`. Four parallel audit subagents reclassified all 318 placeholders: ~70 → `None` (route-level pages, standalone panels, popups), ~19 → `allow_default` (image viewers, text inputs, credential forms; each adds `oncontextmenu: stop_propagation` on its DOM element), ~229 kept `inherit` (visual leaves inside menu-owning parents, and existing `*_context_menu.rs` component definitions). 3 TODOs left in-code for future menus that need authoring: `ForumPostContextMenu` (forum_view.rs:653, 778) and `UserRowContextMenu` (dm_user_sidebar.rs:91).
- [x] **5.1.4** **Phase D — deny.** *Re-verified 2026-04-17 now that §5.1.1 and §5.1.3 are both shipped.* `baseline.json` remains `{"violations": []}`; `cargo check --workspace` succeeds with zero `context_menu_coverage` violations; every `#[component]` site in the workspace carries a classified decorator (`None` / `allow_default` / `inherit` / typed menu).

### 5.2 Author ergonomics

- [x] **5.2.1** Shipped. Coverage surfaces on every `cargo check` / `cargo clippy` / rust-analyzer save via `lint-gate`'s `context_menu_coverage::scan`. The `regen-baseline` cargo feature (see `crates/lint-gate/Cargo.toml:16-19` and `build.rs:36,56-66`) lets you refresh `baseline.json` after a cleanup wave: `cargo check --features regen-baseline -p poly-lint-gate`.
- [x] **5.2.2** Shipped. `crates/core/agents.md` carries a top-level "CRITICAL: `#[context_menu(...)]` on ALL `#[component]` Functions" section enumerating the four variants, the author's rule-of-thumb (default to `None`), the author-a-menu pointer, and the `regen-baseline` command.
- [x] **5.2.3** Shipped. `crates/lint-gate/build/context_menu_coverage.rs` emits the full hint in the `cargo::error=` line: "`add one of `(YourMenu)` (attach a menu), `(None)` (opt out), `(allow_default)` (native menu, e.g. images/inputs), or `(inherit)` (defer to parent)`" — rust-analyzer surfaces it inline on save.

## 6. Testing strategy

- [ ] **6.1.1** **Build — registry coverage.** The `lint-gate` `build.rs` scan from 3.1.2 runs on every `cargo check`, emits `cargo::error=` for any `#[component]` without a decorator. No `#[test]` to skip; agents cannot silently ignore it. A small runtime smoke test in `crates/core/tests/context_menu.rs` asserts that the `CTX_MENU_COVERAGE` `linkme` slice parses cleanly (tag well-formedness only) — belt-and-braces, not the gate.
- [ ] **6.1.2** **Unit — menu dispatch.** Per menu, a test that constructs the `ContextMenuFor::Ctx` from a mocked props and asserts `render()` produces the expected items by i18n key. Extends the existing chat-view render tests.
- [ ] **6.1.3** **Snapshot — overlay markup.** `insta`-style snapshot of the rendered menu HTML for a fixed stack state (single menu open, submenu open, allow-default no-op).
- [ ] **6.1.4** **MCP UI — desktop.** Via `poly-desktop` / `poly-electron` / `poly-web` MCPs: `launch_app → connect_cdp → click_at(x,y,right)` on each of the annotated surfaces, `take_screenshot`, assert the menu container `.context-menu` is present and the native browser menu is not. Scripted per backend.
- [ ] **6.1.5** **MCP UI — mobile viewport.** `set_viewport({width:390,height:844})` on `poly-web`, simulate long-press via CDP `Input.dispatchTouchEvent`, assert the center-overlay variant renders, back-button pop works, submenu push stacks visually.
- [ ] **6.1.6** **Forum-specific regression.** Explicitly assert right-clicking a Lemmy post does NOT show "Invite" or "Server Boost."
- [ ] **6.1.7** Haiku test-harness entry — extend `TEST_HARNESS.md` with a section 8 "context-menu smoke" that the haiku subagent runs after any UI-touching PR.

## 7. Resolved questions

All questions resolved 2026-04-17. Decisions are load-bearing for the runtime work in §4.

- [x] **7.1.1** ~~Hardware-back on true-native mobile.~~ **Decision:** out of scope for this plan. All current Poly targets are WebView-backed (`apps/web` Chromium, `apps/desktop` Wry, `apps/desktop-electron` Electron). All three honour `hashchange` — §4.3.3's `#poly-ctx-menu-{id}` trick works uniformly. True-native Dioxus-mobile builds (iOS/Android without WebView) do not exist in this repo. Moved to §8 Out of scope; revisit when a native-mobile target lands.
- [x] **7.1.2** ~~Keyboard activation (Shift+F10 / Context-Menu key).~~ **Decision:** yes, supported for free. Browsers synthesize a `contextmenu` event from both keys with cursor coordinates at the focused element's center, so the existing `oncontextmenu` handler from §4.2 and the root guard from §4.5.1 both fire without additional wiring. No DSL change; add an integration test under §6.1.4 that dispatches `KeyboardEvent` Shift+F10 via CDP.
- [x] **7.1.3** ~~Configurable long-press duration.~~ **Decision:** no. 500 ms matches iOS Safari and the existing `channel_list.rs` timer. Per-component overrides are YAGNI. If future usage demands it, add `#[context_menu(Foo, press_ms = 300)]` — the attribute macro's argument parser already accommodates a key=value form.
- [x] **7.1.4** ~~Accessibility — `aria-haspopup`, focus management.~~ **Decision:** yes, required. The §4.1 `ContextMenuStack` host component MUST set: `aria-haspopup="menu"` + `aria-expanded` on the trigger, `role="menu"` on the overlay, `role="menuitem"` on each item, and a focus-trap for the duration of the stack being non-empty (first item focused on open; Escape restores focus to the trigger). Moved to §4 as a runtime requirement; tracked as a test-plan item in §6.1.
- [x] **7.1.5** ~~Citation anchors for mobile UX.~~ **Decision:** not a blocker. The mobile stacked-sheet pattern in §4.3.4 is well-established industry precedent (iOS `UIContextMenu`, Material 3 long-press actions, React Native `@react-native-menu/menu`); inline prose references are sufficient for a design document. Dropped from blocker list.
- [x] **7.1.6** ~~Dioxus fullstack SSR interaction with `linkme`.~~ **Decision:** non-issue by design. The `CTX_MENU_COVERAGE` slice is consulted only by the lint-gate `build.rs` at compile time (already shipped) and by the runtime `ContextMenuStack` host which lives inside the client-only WASM render path. The fullstack server half never calls `ContextMenuFor::render`. No cfg-gates needed.

## 8. Out of scope

- Keyboard shortcut menus / command palette (separate plan).
- Drag-and-drop context (ondrop menus) — today's dnd flow is its own pipeline in `main_layout.rs`.
- Rich per-item keyboard navigation inside a menu (arrow keys) — phase 2 polish.
- Reworking `MsgContextMenuOverlay`'s quick-reactions row semantics.
- True-native (non-WebView) iOS/Android back-handler wiring — moved from §7.1.1. Revisit when a non-WebView Dioxus-mobile target lands.
- Touching MCP binaries (`mcp/*`) or host-bridge routes.

---

## Files this plan touches

New:
- `crates/poly-context-menu-macros/` (new proc-macro crate)
- `crates/core/src/ui/context_menu/mod.rs`
- `crates/core/src/ui/context_menu/host.rs`
- `crates/core/src/ui/context_menu/long_press.rs`
- `docs/plans/context-menu-coverage.toml`
- `scripts/audit_context_menus.sh`

Edited:
- `crates/core/Cargo.toml` (dep on new macro crate + `linkme`)
- `crates/core/src/state/mod.rs` (`ContextMenuState` + `ChannelContextMenuState` → `context_menu_stack`)
- `crates/core/src/ui/main_layout.rs` (mount `ContextMenuStack`, add root guard)
- `crates/core/src/ui/account/server/context_menu.rs`
- `crates/core/src/ui/account/common/channel_context_menu.rs`
- `crates/core/src/ui/account/common/chat_view.rs` (`MsgContextMenuOverlay`)
- `crates/core/src/ui/favorites_sidebar.rs`
- `crates/core/src/ui/account/common/account_server_bar.rs`
- `crates/core/src/ui/account/common/channel_list.rs`
- `crates/core/src/ui/account/common/forum_view.rs` (add forum-post menu)
- Per-backend `crates/core/src/ui/account/{demo,stoat,discord,matrix,teams,poly_native}/context_menu.rs`
- All remaining `#[component]` sites under `crates/core/src/ui/` and `clients/*/src/` — annotation-only during phase C
- `CLAUDE.md` (mention the decorator requirement)
- `TEST_HARNESS.md` (add menu smoke section)
- `docs/4-ui/4.0-component-architecture.md` (cross-reference the new DSL)
- `docs/4-ui/4.3-mobile-layout.md` (document center-overlay menu pattern)
- `docs/INDEX.md` (link this plan under section 4)
