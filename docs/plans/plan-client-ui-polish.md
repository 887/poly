# Plan — Client-UI Surface Polish (Post-WP 9 Cleanup)

> **Created:** 2026-04-18
> **Status:** 🟡 PLANNING
> **Parent:** `docs/plans/plan-client-ui-surface.md` (the 31-decision refactor that landed WPs 0-9, now ✅ COMPLETED structurally)
> **Scope:** every cosmetic / rough-edge / deferred-TODO item carried over from WP 0-9, plus the fixes shipped in `fix(client-ui-surface)` and `fix(demo)` follow-up commits
> **Goal:** take the functional-but-rough landing from 'works' to 'polished enough to demo without apologies'
> **Test policy:** every pack lands its full test matrix at every applicable layer (see §1 below). A pack is not complete until its tests are green.

---

## 1. Testing Requirements (MANDATORY — inherits D31 from parent plan)

**Every pack in this plan MUST ship tests at every applicable layer before landing.** No exceptions. No "tests in a follow-up." Tests are the proof the pack works — without them the pack is speculative.

### 1.1 The five layers (from plan-client-ui-surface.md §6A)

| Layer | Where it lives | When this polish plan uses it |
|---|---|---|
| **(a) Unit tests** inline `#[cfg(test)] mod tests` | Same file as the code | Every host component that gains logic (parsers, state machines, event handlers, URL validators, sanitizer rules, row-template interpreters, tree-reconstructors). Every plugin that gains non-trivial code (state-aware menu declarations, real-data fetchers). |
| **(b) Per-backend capability tests** | `clients/<name>/tests/capabilities.rs` | Every polish item that changes what a backend declares (new sort options, new menu items, state-aware conditionals, icon sources, toolbar tabs). Pin exact declared shape. |
| **(c) Per-backend integration tests** | `clients/<name>/tests/integration*.rs` | Every polish item that wires real API data (Lemmy `get_view_rows`, HN feed fetch, GitHub issue fetch). Must round-trip HTTP fixtures (VCR-style cassettes if live API not stable). |
| **(d) E2E via WASM host** | `crates/plugin-host-tests/tests/client_e2e/` — extend existing `harness_menus.rs`, `harness_settings.rs`, `harness_sidebar.rs`, `harness_views.rs`, `harness_composer.rs`, `harness_custom_block.rs`, `harness_build_route.rs` | Every interaction fix — action-id → invoke_context_action round-trip, setting save-reload-persist round-trip, sidebar invalidation event emit-and-receive, toolbar sort re-fetch, paginated view-rows cursor flow. Fill in the `todo!()` bodies that WP 0 left. |
| **(e) Cross-backend parity** | `clients/client/tests/client_ui_surface_parity.rs` — the `#[ignore]` stubs already exist | Remove `#[ignore]` attrs and implement each `#[test]` as polish items land. `server_menu_never_empty_when_groups_supported` (P44), `user_menu_has_block_action_if_blocking_supported`, `settings_sections_respect_scope` (P18/19), `sidebar_layout_matches_capabilities_mapping` (P24-27), `view_descriptor_cursor_kind_matches_declaration` (P5, P23), `composer_buttons_match_backend_features` (P32), `no_dead_per_backend_files` (P55). |
| **(f) Lint-gate scanner tests** | `crates/lint-gate/src/lib.rs` `#[cfg(test)] mod tests` | Every new scanner (P40 custom-block usage counter). Every extension to existing scanners (P45 FTL missing-key strictness upgrade). |
| **(g) Trybuild compile-fail** | `crates/ui-macros/tests/compile-fail-client-ui/*.rs` + `*.stderr` | Every compile-time invariant we add (P45 missing FTL → build error; P25 duplicate action ids; P39 custom-block with `<script>`). |
| **(h) UI snapshots** | `tests/snapshots/<backend>/<surface>.html` via Playwright through the poly-web MCP | Every visual change in Packs A, B, D, E. Before-snapshot taken first; after-snapshot diffed; golden updated in same PR. `/demo_forum/.../comm-rust-lang` is the demo_forum golden. |

### 1.2 Per-pack test matrix — what each pack MUST ship

**Pack A (visual polish — Ps 1, 2, 3, 8, 13, 14, 15, 16, 17, 30, 31, 32, 33, 45, 48, 49, 50):**
- (a) Unit: row-template renderer pure-fn tests (given inputs produce expected DOM strings); icon-source rendering (emoji vs SVG path via sanitizer); ARIA-attr presence checks.
- (d) E2E: fill in `harness_menus::menu_items_use_kebab_action_ids`, `menu_items_have_valid_ftl`.
- (h) Snapshots: before/after for ClientMenu, ListBody, TreeBody, CardBody, ViewHeader, ComposerHooks, MessageActions, settings section rows, sidebar rows. At least one backend per surface (demo_forum for all non-chat; discord for chat).
- (g) Trybuild: no new lints expected in A.

**Pack B (interaction wiring — Ps 4, 5, 7, 10, 11, 12, 28, 34):**
- (a) Unit: `ActionOutcome` handler dispatch (Navigate/Toast/Pending/Completed/RefreshTarget/RefreshSidebar) — each variant hits the right side-effect path. Toast queue pop/push. Pending-handle polling state machine (mock plugin returns Pending → Completed, host polls n times, stops).
- (b) Per-backend: each backend that declares a Navigate outcome — assert the built route string resolves via `host-api.build-route`.
- (d) E2E: fill in `harness_menus::invoke_action_roundtrip`, `menu_pending_action_polls`, `harness_sidebar::sidebar_invalidated_event_refetches`. Every backend driver exercises known action IDs + one unknown (asserts NotFound).
- (e) Cross-backend: `sidebar_invalidated_refreshes_on_event` parity test.
- (h) Snapshots: Pending spinner visible (mock plugin with sleep); Toast component rendered with severity variants.

**Pack C (storage + real data — Ps 18, 19, 20, 22, 23):**
- (a) Unit: per-plugin storage-key derivation (plugin-id/scope/scope-id/key → KV key); JSON encode/decode for each `SettingKind`; scope-id isolation (setting on server A doesn't leak to server B).
- (b) Per-backend capability: no change — still asserts declared shape.
- (c) Per-backend integration: **round-trip test** — `set_setting_value(scope, scope-id, key, json)` then `get_setting_value(scope, scope-id, key)` returns the written json. Restart the plugin instance between set and get (simulates reload).
- (d) E2E: fill in `harness_settings::setting_roundtrip`, `setting_persists_across_reload`. Cross-plugin KV isolation test — plugin A can't read plugin B's key.
- (e) Cross-backend: `settings_sections_respect_scope` parity test un-`#[ignore]`'d and implemented.
- (h) Snapshots: settings panel with saved value round-trip visible; save-confirmation toast visible.

**Pack D (sidebar completeness — Ps 24, 25, 26, 27, 29):**
- (a) Unit: each layout component renders correct tree from canned sidebar-declarations (nested items, collapsible sections, badges, default-collapsed state).
- (b) Per-backend capability: Matrix spaces-rooms shape has nesting depth ≥ 2. Lemmy communities has Subscribed/Local/All as declared sidebar-sections. HN feed declares 6 feed tabs as sidebar-items. GitHub/Forgejo repo-tree declares Issues/PRs/Discussions as children via parent_id.
- (d) E2E: fill in `harness_sidebar::sidebar_declaration_well_formed` (already exists, body is `todo!()`), `sidebar_layout_matches_capabilities`.
- (e) Cross-backend: `sidebar_route_kinds_have_host_handlers` un-`#[ignore]`'d — every sidebar-route-kind on a sidebar-item must map to an existing host handler.
- (h) Snapshots: one per-backend sidebar rendered with real plugin-declared data.

**Pack E (plugin data integration — Ps 6, 41, 42, 43, 44):**
- (a) Unit: data-model mappers (Lemmy JSON post → ViewRow; HN story API response → ViewRow; GitHub issue → ViewRow). State-aware menu predicate (subscribed? → show Unsubscribe; else Subscribe).
- (c) Per-backend integration: VCR-style HTTP fixtures (or live API if stable). Fetch a page of posts, assert non-empty rows with expected fields. Fetch a post detail, assert body+comments.
- (d) E2E: fill in `harness_views::view_rows_paginate`, `view_cursor_is_structured`, `view_detail_returns_custom_block` (all three currently `todo!()`).
- (e) Cross-backend: `view_descriptor_cursor_kind_matches_declaration` parity.
- (h) Snapshots: real Lemmy/HN/GitHub content rendered — each at least one page.

**Pack F (capability gating — Ps 36, 37, 57, 58, 59, 60, 61):**
- (a) Unit: capability-check helpers (is_voice_supported, has_friends, etc.) return expected bool for each of the 10 backends.
- (b) Per-backend capability: pin which UI surfaces each backend exposes (HN: no friends/dms/notifications/voice; Lemmy: no friends/voice; Discord: all).
- (d) E2E: new harness helpers `harness_capabilities::account_bar_respects_caps`, `notification_filter_respects_caps`, `composer_disabled_on_readonly_backends`.
- (e) Cross-backend: `unconditional_routes_404_on_unsupported` (P60).
- (h) Snapshots: HN account shows no DMs/Friends/Notifications tabs; no voice buttons; no notification-filter "Voice invites" chip.

**Pack G (custom-block hardening + lint — Ps 38, 39, 40):**
- (a) Unit: shadow-root attachment works (given sanitized HTML + CSS, shadow DOM is created with expected structure); XSS corpus of ~30 known-bad inputs each passes sanitization; `<use xlink:href="...">` external refs blocked; CSS `url(javascript:...)` blocked.
- (f) Lint-gate: `custom_block_usage_ok` (under threshold), `custom_block_usage_warns` (over threshold), counter accuracy.
- (g) Trybuild: `custom_block_with_script_tag.rs` → compile fail; `custom_block_with_event_handler.rs` → compile fail.
- **Security audit:** dedicated `docs/security/custom-block-audit.md` enumerates allowlist decisions with rationale.

**Pack H (cleanup debt — Ps 51, 52, 53, 54, 55, 56, 6):**
- (a) Unit: post D12-field removal, no test should reference removed fields (a grep test).
- (f) Lint-gate: `forbid_backend_slug_match` sweep re-run — zero violations after removing `backend_emoji` survivor.
- (d) E2E: full matrix re-run — no tests broken by removals.
- (h) Snapshots: all existing goldens still match (unchanged behavior).

**Pack I (i18n + locales — Ps 46, 47):**
- (a) Unit: FTL parser accepts all new bundles; `t(key)` returns non-key-echo for every declared label.
- (f) Lint-gate: `ftl_label_key_coverage` sweep — zero violations after backfill.
- (b) Per-backend: assert every declared label-key has an entry in `locales/en/plugin.ftl` (currently enforced at build time; this is the runtime audit).

### 1.3 How tests are verified as part of each pack

Before a pack's PR merges, run:

```bash
# All five layers
cargo test --workspace --all-features                       # (a) + (b) + (c) + (f) layers
cargo test -p poly-plugin-loader-tests --features test-demo # (d) layer for each enabled backend
cargo test -p poly-client --test client_ui_surface_parity   # (e) layer
cargo test -p poly-ui-macros --test compile_fail_client_ui  # (g) layer

# (h) layer: re-run Playwright snapshot runner via poly-web MCP
# Compare against committed goldens under tests/snapshots/<backend>/*.html
```

CI runs this full matrix on every pack PR. No pack merges with red tests, even if the production code "looks right."

### 1.4 The `todo!()` audit — pre-condition for Pack A

Before Pack A begins: inventory every `todo!()` body left in the WP 0 harness skeletons:

```bash
grep -rn 'todo!' crates/plugin-host-tests/tests/client_e2e/harness_*.rs
```

Each `todo!` is a test that exists in the file but panics on call — Packs A through I will fill in these bodies per §1.2 mapping. By the end of this plan, **zero `todo!()` remain in the harness.**

---

## 2. What "polish" means here

WPs 0-9 shipped the architectural refactor: plugins own context menus, settings sections, sidebars, channel views, composer hooks. The user's original bug (Lemmy showing Discord items) is fixed. `cargo test` is green across the board.

**What's NOT polished yet:**

- Several `ActionOutcome` variants (`Navigate`, `Toast`, `Pending`) are **logged but not actioned** — they appear to work at the WIT layer but don't cross the last mile into user-visible UX.
- Body engines (`ListBody`, `TreeBody`, `CardBody`, `SplitBody`) render data **as flat text**, not cards with hover states / separators / click handlers.
- Toolbar `sort_id` / `filter_id` / `tab_id` selections **don't re-fetch rows** — they only update local state.
- **Empty plugin-scope host containers** (e.g. `MessageActions` renders an empty div when the plugin returns `[]`) take layout space.
- **Host-universal menu items** (Mute Server, Hide Muted Channels) still appear on forum/feed backends where they make no sense.
- **Many TODOs** from WP 5-8 final reports reference real-data integration, event-driven refresh, accessibility, etc.

Each item below is tagged `severity`, `surface`, and `parent-WP` so we can sort/filter before execution.

---

## 3. Severity Legend

| Tag | Meaning |
|---|---|
| **🔴 visible bug** | User sees a wrong / broken UI state today |
| **🟠 rough edge** | Works but looks unfinished; reviewer would flag at a code review |
| **🟡 deferred feature** | Deliberately skipped in the parent WP with a TODO |
| **🔵 architectural debt** | No user-visible impact; matters for code quality / future-proofing |

---

## 4. Item Inventory

### 4.1 View renderers (`crates/core/src/ui/client_ui/view/`)

| # | Severity | Item | Files |
|---|---|---|---|
| P1 | 🟠 | `HotNew` toolbar tabs render with **no separator / spacing** — two buttons glued together. Add `.view-toolbar-tabs { display: flex; gap: 8px }` or a CSS-level separator. | `view/toolbar.rs`, theme CSS |
| P2 | 🟠 | `ListBody` / `TreeBody` rows render as **flat text blocks**. Should be cards with hover effects, click target, visual hierarchy. Borrow styles from the old `forum_view.rs` post cards (the ones we deleted — check git history for `.forum-post-card` CSS). | `view/list_body.rs`, `view/tree_body.rs` |
| P3 | 🔴 | **Row click is a no-op** in ListBody/TreeBody/CardBody. Need to dispatch to `get_view_detail(channel_id, row_id)` and render the detail either inline (TreeBody expands) or in a detail pane (SplitBody). Currently ListBody hardcodes `Route::ForumPostRoute` push which isn't the right behavior for every backend. | `view/list_body.rs` (esp), all body engines |
| P4 | 🟡 | **Toolbar clicks don't re-fetch.** `ClientViewToolbarAction::SelectSort/Filter/Tab` updates local state but `ListBody` ignores it. Wire `use_resource` to re-run `get_view_rows(channel, cursor=None, sort_id=selected, ...)` on any toolbar change. | `view/list_body.rs`, `view/toolbar.rs`, `view/mod.rs` |
| P5 | 🟡 | **Infinite scroll missing.** Body engines fetch one page then stop. Add scroll-end detection + `get_view_rows(cursor=page.next_cursor)` continuation. | `view/list_body.rs`, `view/card_body.rs` |
| P6 | 🟡 | `TreeBody` renders flat, not actually threaded. Each post has a comment tree (via `get_view_detail`) but we never fetch + render it nested. | `view/tree_body.rs` |
| P7 | 🟡 | `SplitBody` detail pane works but has **no loading state**; user sees blank area until `get_view_detail` returns. | `view/split_body.rs` |
| P8 | 🟠 | `ViewHeader` renders title + subtitle as plain `h2`/`small`. Needs visual treatment — background, padding, maybe plugin icon. | `view/header.rs`, theme CSS |
| P9 | 🟡 | `RowTemplate` field routing is hardcoded (`primary_text`, `secondary_text`, `meta_text`). Plugins can't declare additional columns. Consider arbitrary-field support in a later WIT iteration. Low priority. | WIT `row-template`, `view/list_body.rs` |

### 4.2 Context menu (`crates/core/src/ui/client_ui/menu.rs`)

| # | Severity | Item | Files |
|---|---|---|---|
| P10 | 🔴 | `ActionOutcome::Navigate(route_str)` is **logged but not pushed** to the navigator. Users click "Open in GitHub" and nothing happens. Wire to `navigator().push(Route::parse(&s))` or equivalent. | `menu.rs` |
| P11 | 🔴 | `ActionOutcome::Toast(payload)` is **logged** — no visual toast shown. Add a host toast system or use existing tracing + a notification drawer. | `menu.rs`, possibly new `toast.rs` |
| P12 | 🟡 | `ActionOutcome::Pending(handle)` polls `poll_action` but has **no spinner UX**. D16 requires a visual signal while async work runs. | `menu.rs` |
| P13 | 🟠 | **Submenu grandchildren are flattened** with a `tracing::warn!`. D28 says unbounded depth but WP 2.A only renders one level. Add recursive submenu rendering. | `menu.rs` |
| P14 | 🟠 | `menu-item::info-block` shows **placeholder text** `[custom-block pending WP 5]` instead of rendering the attached `CustomBlock`. CustomBlock ships in WP 5 — update the stub to call it. | `menu.rs`, `custom_block.rs` |
| P15 | 🟡 | **Icons are not rendered.** `MenuItem.icon: Option<IconSource>` is passed through but `ClientMenu` shows label-only. Render `Emoji(s)` as text; `Svg(s)` through the SVG sanitizer (the ammonia path from WP 5). | `menu.rs` |
| P16 | 🟠 | **Error row is hardcoded English**: "plugin error: failed to load items". Should be an FTL key — `ui-plugin-menu-error`. | `menu.rs`, `locales/en/main.ftl` |
| P17 | 🟠 | **ClientMenu fetches on every open** (D24 correct) but **no loading state** — users see a half-empty menu flicker while the fetch completes. Add a loading stub ("…") for <50ms, then show items. | `menu.rs` |

### 4.3 Settings sections (`crates/core/src/ui/client_ui/settings_section.rs`)

| # | Severity | Item | Files |
|---|---|---|---|
| P18 | 🔴 | **Storage is stubbed.** Every plugin's `set_setting_value` returns `NotSupported`. Users toggle settings and they revert. Wire each plugin's `get/set_setting_value` to `host-api.kv_get` / `kv_set`. Storage key: `plugin/<plugin-id>/<scope>/<scope-id>/<key>`. | Every `clients/<name>/src/lib.rs` with a section declaration |
| P19 | 🟡 | **Per-channel settings never render.** `ChannelSettingsPage` doesn't exist in the host. Needs creation + wire `PluginSettingsSection` with `scope: PerChannel`. | `crates/core/src/ui/account/channel/settings/` (new) |
| P20 | 🟠 | **Scroll-spy registration missing** for plugin sections. Per the WP 3.C agent report, plugin-declared section divs are in the DOM but not registered with the scroll-spy config, so scroll-to-section from the sidebar doesn't highlight the active plugin section. | `crates/core/src/ui/account/server/settings/mod.rs`, the scroll-spy helper |
| P21 | 🟡 | **Info-blocks inside sections** still render `[custom-block pending WP 5]` stub. Update to call real `CustomBlock` component. | `settings_section.rs` |
| P22 | 🟡 | **No save-confirmation UX.** User toggles a setting, value writes via `set_setting_value`, but there's no "Saved" toast or success indicator. Pair with P11. | `settings_section.rs` |
| P23 | 🟠 | Field labels render `plugin-<id>-setting-<key>-label` text **without a description** next to them. The `setting-descriptor` has no `desc_key` field in WIT, but host has an implicit `-desc` FTL key convention. Update widgets to look up + show. | `settings_section.rs`, WIT may need a desc_key field |

### 4.4 Sidebar (`crates/core/src/ui/client_ui/sidebar/`)

| # | Severity | Item | Files |
|---|---|---|---|
| P24 | 🟠 | `SpacesRoomsLayout` (Matrix) is a **placeholder** that renders a header + the existing ChannelList + a flat server list. Real Matrix sidebar needs spaces (outer) → rooms (nested) tree. | `sidebar/spaces_rooms.rs` |
| P25 | 🟠 | `CommunitiesLayout` (Lemmy) is a **flat list** of servers. Real Lemmy sidebar has subscribed + local + all tabs (the ones visible in the demo_forum screenshot are ChannelList's built-ins, not ours yet). | `sidebar/communities.rs` |
| P26 | 🟠 | `FeedLayout` (HN) has 6 **hardcoded inert rows** (Top/New/Best/Ask/Show/Jobs). They're not clickable. Wire them as sidebar-items returned from the plugin via `get_sidebar_declaration`. | `sidebar/feed.rs`, `clients/hackernews/src/lib.rs` |
| P27 | 🟠 | `RepoTreeLayout` (GitHub) has **hardcoded Issues/PRs/Discussions** children. Plugin should declare these via `sidebar-section.items` with `children` via `parent_id` flat pattern. | `sidebar/repo_tree.rs`, `clients/github/src/lib.rs`, `clients/forgejo/src/lib.rs` |
| P28 | 🔴 | **ClientEvent::SidebarInvalidated** doesn't trigger a refetch yet. Per D19 this is event-driven. Wire the host event loop to kick `use_resource` invalidation on receipt. | `sidebar/mod.rs`, `state/*.rs` (event router) |
| P29 | 🟡 | **Sidebar fallback is silent.** On error from `get_sidebar_declaration`, we fall back to `ChannelListLayout`. User never knows the plugin failed. Add a discrete error badge or toast. | `sidebar/mod.rs` |
| P30 | 🟠 | **CustomSidebar icons not rendered** — `SidebarItem.icon: Option<IconSource>` is passed but `SidebarItemRow` shows label-only. Same fix as P15. | `sidebar/custom.rs` |

### 4.5 Composer + message actions (`crates/core/src/ui/client_ui/composer.rs`)

| # | Severity | Item | Files |
|---|---|---|---|
| P31 | 🔴 | **Composer buttons visually flat** — no hover, no active state, no tooltip FTL. Match existing `.toolbar-btn` styling better. | `composer.rs`, theme CSS |
| P32 | 🟠 | **Empty hook containers take layout space.** `ComposerHooks` renders three `<div>` slots (above/left/right of input) even when the plugin has no buttons. If `buttons.is_empty()`, render nothing. | `composer.rs` |
| P33 | 🟠 | **MessageActions renders after Forward** but has no separator. Insert a `<div class="message-action-separator">` when plugin actions are non-empty. | `composer.rs` |
| P34 | 🟡 | Same `ActionOutcome::Navigate`/`Toast` logged-but-not-actioned problem as the menu (P10/P11). Share a common helper. | `composer.rs`, `menu.rs` |
| P35 | 🟡 | **Icon rendering** same issue as menu — composer-button icons are passed through `icon: String` (emoji only, no SVG). WIT should migrate to `IconSource` for consistency. | WIT `composer-button`, `composer.rs` |

### 4.6 Host-universal menu items on non-chat backends

| # | Severity | Item | Files |
|---|---|---|---|
| P36 | 🟠 | "Mute Server" and "Hide Muted Channels" / "Show All Channels" toggles **appear on demo_forum / Lemmy** where they don't make sense. Strict D10 says they should move to plugins. Two options: (a) gate on capability flag (e.g. `has_channels`), or (b) move them into the relevant plugin declarations. | `crates/core/src/ui/account/server/context_menu.rs` |
| P37 | 🟠 | "Notification Settings" submenu arrow on forum backends: the navigation target is `ServerSettingsRoute` which may or may not exist for the backend. Verify routes resolve. | `server/context_menu.rs` |

### 4.7 Custom-block

| # | Severity | Item | Files |
|---|---|---|---|
| P38 | 🔵 | **Scoped CSS, not true shadow-root.** WP 5 punted on shadow-root. Plugins can theoretically break out of scoping if ammonia misses something. Upgrade to shadow-root via `document::eval` + `attachShadow`. | `custom_block.rs` |
| P39 | 🔵 | **SVG sanitizer allowlist** hasn't had a formal security audit. Check: can SVG `<use xlink:href="...">` reference external resources? Can CSS `background-image: url(javascript:...)` sneak through the stylesheet? | `custom_block.rs` |
| P40 | 🟡 | **No usage lint yet.** Per D4 / §9 risk: plugin authors over-using custom-block should trigger a review. Add a `custom_block_usage` counter to lint-gate. | `crates/lint-gate/build/custom_block_usage.rs` (new) |

### 4.8 Plugin data integration

| # | Severity | Item | Files |
|---|---|---|---|
| P41 | 🟡 | **`get_view_rows` returns empty** for Lemmy / HN / GitHub / Forgejo. Real API integration deferred in WP 5. Hook up the HTTP fetchers that probably already exist in those backends. | `clients/{lemmy,hackernews,github,forgejo}/src/lib.rs` |
| P42 | 🟡 | **`get_view_detail` returns NotSupported** for all backends except demo_forum. Same deferred-integration. Detail render (post body + comments) waits on this. | same |
| P43 | 🟠 | **Lemmy context-menu items are non-conditional** — "Subscribe" shows even when already subscribed, "Unsubscribe" logic absent. Menu declaration should inspect state: `if subscribed { Unsubscribe } else { Subscribe }`. Plugin needs state access. | `clients/lemmy/src/lib.rs` |
| P44 | 🟡 | Same for Discord (Mute/Unmute, Favorite/Unfavorite conditional) — every backend's menu could benefit from state-aware declarations. | every `clients/<backend>/src/lib.rs` |

### 4.9 FTL coverage

| # | Severity | Item | Files |
|---|---|---|---|
| P45 | 🟠 | **Missing FTL keys fall back to raw string.** The current `t()` behavior logs a warning but renders `plugin-foo-menu-bar-label` verbatim to the user. Either (a) show the fallback more gracefully (Title-Cased from key), or (b) make the build-time lint P21 more aggressive so missing keys fail the build, not runtime. | `crates/core/src/i18n/mod.rs`, `crates/lint-gate/build/ftl_label_key_coverage.rs` |
| P46 | 🟡 | **Some plugins lack `-desc` entries** for settings fields. Even when P23 lands (show desc next to label), entries need to exist. | each `clients/<name>/locales/en/plugin.ftl` |
| P47 | 🔵 | **Non-English locales not populated.** The plugin FTL files under `locales/de/`, `locales/es/`, `locales/fr/` were never updated for WP 2-6 declarations. Translators need a seed list of new keys. | every `clients/<name>/locales/{de,es,fr}/plugin.ftl` |

### 4.10 Accessibility

| # | Severity | Item | Files |
|---|---|---|---|
| P48 | 🟠 | **No ARIA on new components.** `ClientMenu` renders divs; should be `role="menu"` + `role="menuitem"`. Same for `PluginSettingsSection`, `ClientSidebar`. | every component file |
| P49 | 🟠 | **Custom-block content has no ARIA guidance.** Plugin authors need guidance on required ARIA attributes. Document in `docs/plugin-authoring.md`. | docs |
| P50 | 🟡 | **Keyboard navigation** on `ClientMenu` not wired (arrow keys, Escape). The existing `ServerContextMenu` uses click-only too — check whether existing chat menu has keyboard support; if yes, match it; if no, add to both. | `menu.rs` |

### 4.11 MCP polish

| # | Severity | Item | Files |
|---|---|---|---|
| P51 | 🟡 | **No capability-driven tool filtering.** Per phase-2.20 D4: MCP exposes all 18 new tools regardless of account. `context_menu_server` on HN always returns `[]` but the tool is advertised. Filter: hide tools where a synthetic `get_*` call against the account's plugin returns consistently empty. | `mcp/chat-mcp/src/tools.rs` |
| P52 | 🟡 | **Old Discord-shaped tools coexist** with new surfaces. `list_friends`, `list_dms`, etc. stay — deprecation + eventual removal. | `mcp/chat-mcp/src/tools.rs` |

### 4.12 Leftover cleanup

| # | Severity | Item | Files |
|---|---|---|---|
| P53 | 🔵 | **`BackendCapabilities` fields tagged TODO(D12)** for removal (`presence`, `typing_indicators`, `reactions`, etc.) but not yet removed. Coordinate with all readers across the workspace. | `clients/client/src/types.rs` + readers |
| P54 | 🔵 | **`settings/accounts.rs:81 backend_emoji(slug: &str)` slug match** survived WP 7's sweep (outside the `as_str()` scanner rule). Should be replaced with plugin-declared icons. | `crates/core/src/ui/settings/accounts.rs` |
| P55 | 🔵 | **`forum_view.rs` still carries dead code** — `ForumPostView`, `ForumPostCard`, `ForumThreadView`, `ForumComment` kept "for continuity" per the WP 5 agent report. If no external callers survive, delete. | `crates/core/src/ui/account/common/forum_view.rs` |
| P56 | 🔵 | **UI snapshot goldens deferred** in WP 0. Still needed for regression safety. Demo-only for now is fine; add remaining backends as we get credentials. | `tests/snapshots/` |

### 4.13 Capability-gated surfaces (phase-2.20 leftover)

| # | Severity | Item | Files |
|---|---|---|---|
| P57 | 🟠 | **DMs/Friends/Notifications tabs unconditional** on non-chat backends. Phase-2.20 D1 — still not fixed. Gate on `backend_capabilities.dms / friends / notifications` fields. | `crates/core/src/ui/account/common/account_server_bar.rs` |
| P58 | 🟠 | **Notification filter enum hardcoded Discord** (All / Mentions / FriendRequests / ServerInvites / VoiceInvites / Other). Still shows "Voice invites" filter on HN. Phase-2.20 D2. Plugin declares categories. | `crates/core/src/ui/account/common/notifications.rs` |
| P59 | 🟠 | **Message composer renders on read-only backends** (HN). Should be disabled / hidden. Phase-2.20 D11. Gate on capability. | `chat_view.rs` |
| P60 | 🟠 | **Unconditional routes** like `/hackernews/.../friends` render an empty shell instead of 404/redirect. Phase-2.20 D5. Gate routes on capability. | `crates/core/src/ui/routes.rs` |

### 4.14 Voice / video surface

| # | Severity | Item | Files |
|---|---|---|---|
| P61 | 🟠 | **Mic / speaker buttons** in account bar render on non-voice backends. Same capability gate as P57. | `crates/core/src/ui/account/common/account_bar.rs`, `voice_banner.rs` |

---

## 5. Proposed Sequencing

Group items so each work package is shippable and limits rebase risk:

### Pack A — Visual polish (🟠 items, CSS-heavy, no trait changes)
P1, P2, P3 (CSS+click), P8, P13, P14, P15, P16, P17, P30, P31, P32, P33, P45 (rendering), P48, P49, P50.

### Pack B — Interaction wiring (🔴 items + deferred behaviors)
P4 (toolbar re-fetch), P5 (infinite scroll), P7 (loading state), P10 (Navigate outcome), P11 (Toast system — likely new `Toast` component + queue), P12 (Pending spinner), P28 (SidebarInvalidated event routing), P34 (shared outcome helper).

### Pack C — Storage + real data (🔴 P18 mostly)
P18 (plugin KV storage wire), P19 (ChannelSettingsPage), P20 (scroll-spy registration), P22 (save-confirmation toast), P23 (field descriptions).

### Pack D — Sidebar completeness
P24 (SpacesRooms proper tree), P25 (Communities tabs), P26 (Feed items via plugin decl), P27 (RepoTree children via plugin decl), P29 (sidebar error badge).

### Pack E — Plugin data integration
P41 (Lemmy/HN/GitHub/Forgejo get_view_rows real), P42 (get_view_detail real), P43 (Lemmy state-aware menu), P44 (Discord et al. state-aware menu).

### Pack F — Capability gating (phase-2.20 leftover, unblocks polish across many pages)
P36, P37 (move or gate universal items), P57 (DMs/Friends/Notifications tabs), P58 (notif filter), P59 (composer on read-only), P60 (routes), P61 (voice UI).

### Pack G — Custom-block hardening + lint
P38 (shadow-root upgrade), P39 (security audit), P40 (usage lint).

### Pack H — Cleanup debt
P53 (D12 flag removal), P54 (backend_emoji), P55 (dead forum code), P56 (snapshots), P51+P52 (MCP filtering + deprecation), P6 (tree threading — could be its own pack).

### Pack I — i18n + locales
P46 (desc keys), P47 (non-English seeds).

---

## 6. Principles

1. **One pack per PR.** Each pack lands cleanly green (`cargo check --workspace` + full test matrix from §1.3) before the next begins.
2. **Every pack ships its tests at every applicable layer** (per §1.2, inherits D31). Visual changes get Playwright snapshot diffs; behavioral changes get unit + e2e + per-backend integration + parity tests. No "tests in a follow-up."
3. **Don't regress WP 0-9.** If a polish item requires touching code that WP 2-6 established, the change preserves the plugin-declarative pattern (no hardcoded slug matches creeping back — the `forbid_backend_slug_match_in_ui` lint stays green).
4. **Prefer declarative CSS over inline styles.** Every theme tweak goes in the existing theme files (not inline `style: "..."`).
5. **Leave no TODO-less stubs.** Every deferred follow-up from this plan gets a tracked comment referencing this document's item number (`// P23: field descriptions`).
6. **Zero `todo!()` in harness by end-of-plan.** §1.4 inventory — every harness body gets a real implementation as its pack lands.

---

## 7. What this plan doesn't cover

- Re-architecting anything in `chat_view.rs`. It stays at 223 KB per D8.
- True shadow-root implementation requires deep Dioxus/WASM work (P38) — included but non-trivial.
- Running new WIT migrations. All items here work within the interfaces landed in WP 1.
- Per-backend styling (e.g. making Lemmy look exactly like lemmy.ml). Only covered generically.

---

## 8. Open questions

1. **Toast system** (P11) — do we have one already? If not, is this one component or a queue + component?
2. **Event routing for SidebarInvalidated** (P28) — where does the host's `ClientEvent` stream get consumed today? Probably `crates/core/src/state/` — confirm during execution.
3. **Keyboard nav** (P50) — match existing or add fresh? The existing ServerContextMenu is click-only so there may be no reference implementation.
4. **Custom-block shadow-root** (P38) — does Dioxus support `ref` hooks well enough to attach a shadow-root on mount? If not, eval-based is a fallback.
5. **Plugin state awareness** (P43/P44) — the plugin's menu declaration is a pure function of `(target, target_id)` today. Adding state (subscribed vs not) implies the plugin's declaration function fetches state. That's fine but changes the mental model from "static declaration" to "live query". Document the expectation.

---

## 9. Execution Order (Recommendation)

**Quick wins:** Pack A (visual polish only) + P32/P33/P16/P29.

**Medium:** Pack B (interaction wiring) + Pack C (storage).

**Deep:** Pack E (real API integration per-backend), Pack F (capability gating migration), Pack G (shadow-root + security).

**Ongoing:** Pack H (cleanup) + Pack I (i18n) — split across multiple small PRs.

Execution model mirrors the parent plan: AI coding agents run packs; each pack's tests must be green at the pack's exit before the next pack begins.

---

## 10. Acceptance Criteria (per pack)

Every pack must pass its §1.2 test matrix AND its visible acceptance criteria below. Both gates or the pack doesn't merge.

- **Pack A:** screenshots before/after of every affected surface (layer h). No console errors. `cargo check` green. ARIA attrs present on every new component per axe-core audit.
- **Pack B:** every `ActionOutcome` variant demoed working on at least one backend (e.g. Navigate pushes a route; Toast shows a visible toast; Pending shows a spinner then resolves). Playwright snapshots captured for each variant.
- **Pack C:** a setting saved on Discord survives a page reload (layer c+d round-trip test proves it). ChannelSettingsPage reachable at a stable route. Cross-plugin KV isolation test passes.
- **Pack D:** Matrix sidebar shows an actual tree (nested spaces→rooms). Lemmy sidebar has functional Subscribed/Local/All tabs. HN feed tabs clickable and re-render the rows. GitHub Issues/PRs/Discussions children under each repo.
- **Pack E:** real posts visible on Lemmy/HN/GitHub (at least one end-to-end integration per backend, with VCR fixture recorded for offline CI).
- **Pack F:** HN account shows no DMs/Friends/Notifications tabs. No "Voice invites" filter. Unsupported routes return 404 or redirect, not an empty shell.
- **Pack G:** `<script>`-in-custom-block test passes shadow-root isolation (genuine shadow DOM, not scoped CSS). Security audit report committed under `docs/security/`. XSS corpus passes.
- **Pack H:** zero `TODO(D12)` comments in workspace. `forbid_backend_slug_match_in_ui` lint zero violations post-`backend_emoji` removal. `cargo check --all-features` green.
- **Pack I:** every declared label-key resolves in at least the English bundle. Non-English seeds stubbed with `// TODO: translate` entries (not raw key fallback at runtime).

At the end of ALL packs: `grep -rn 'todo!' crates/plugin-host-tests/tests/client_e2e/harness_*.rs` returns **zero results** (per §1.4).

---

## 11. Total

**61 items** across **9 packs**. Each item has a size (not filled in — fill during triage). Severity split:

- 🔴 visible bugs: **10** (P3, P10, P11, P18, P28, plus some 🟠s that are actually user-visible)
- 🟠 rough edges: **~30**
- 🟡 deferred features: **~15**
- 🔵 architectural debt: **~6**

Reading order during execution: start from §5 packs; within a pack, start from the highest-severity items. Tests go in the same PR as the code change (§1.2 matrix, mandatory per §6 Principle 2).
