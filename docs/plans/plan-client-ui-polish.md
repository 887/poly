# Plan — Client-UI Surface Polish (Post-WP 9 Cleanup)

> **Created:** 2026-04-18
> **Status:** 🟡 PLANNING
> **Parent:** `docs/plans/plan-client-ui-surface.md` (the 31-decision refactor that landed WPs 0-9)
> **Scope:** every cosmetic / rough-edge / deferred-TODO item carried over from WP 0-9, plus the fixes shipped in `fix(client-ui-surface)` and `fix(demo)` follow-up commits
> **Goal:** take the functional-but-rough landing from 'works' to 'polished enough to demo without apologies'

---

## 0. What "polish" means here

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

## 1. Severity Legend

| Tag | Meaning |
|---|---|
| **🔴 visible bug** | User sees a wrong / broken UI state today |
| **🟠 rough edge** | Works but looks unfinished; reviewer would flag at a code review |
| **🟡 deferred feature** | Deliberately skipped in the parent WP with a TODO |
| **🔵 architectural debt** | No user-visible impact; matters for code quality / future-proofing |

---

## 2. Item Inventory

### 2.1 View renderers (`crates/core/src/ui/client_ui/view/`)

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

### 2.2 Context menu (`crates/core/src/ui/client_ui/menu.rs`)

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

### 2.3 Settings sections (`crates/core/src/ui/client_ui/settings_section.rs`)

| # | Severity | Item | Files |
|---|---|---|---|
| P18 | 🔴 | **Storage is stubbed.** Every plugin's `set_setting_value` returns `NotSupported`. Users toggle settings and they revert. Wire each plugin's `get/set_setting_value` to `host-api.kv_get` / `kv_set`. Storage key: `plugin/<plugin-id>/<scope>/<scope-id>/<key>`. | Every `clients/<name>/src/lib.rs` with a section declaration |
| P19 | 🟡 | **Per-channel settings never render.** `ChannelSettingsPage` doesn't exist in the host. Needs creation + wire `PluginSettingsSection` with `scope: PerChannel`. | `crates/core/src/ui/account/channel/settings/` (new) |
| P20 | 🟠 | **Scroll-spy registration missing** for plugin sections. Per the WP 3.C agent report, plugin-declared section divs are in the DOM but not registered with the scroll-spy config, so scroll-to-section from the sidebar doesn't highlight the active plugin section. | `crates/core/src/ui/account/server/settings/mod.rs`, the scroll-spy helper |
| P21 | 🟡 | **Info-blocks inside sections** still render `[custom-block pending WP 5]` stub. Update to call real `CustomBlock` component. | `settings_section.rs` |
| P22 | 🟡 | **No save-confirmation UX.** User toggles a setting, value writes via `set_setting_value`, but there's no "Saved" toast or success indicator. Pair with P11. | `settings_section.rs` |
| P23 | 🟠 | Field labels render `plugin-<id>-setting-<key>-label` text **without a description** next to them. The `setting-descriptor` has no `desc_key` field in WIT, but host has an implicit `-desc` FTL key convention. Update widgets to look up + show. | `settings_section.rs`, WIT may need a desc_key field |

### 2.4 Sidebar (`crates/core/src/ui/client_ui/sidebar/`)

| # | Severity | Item | Files |
|---|---|---|---|
| P24 | 🟠 | `SpacesRoomsLayout` (Matrix) is a **placeholder** that renders a header + the existing ChannelList + a flat server list. Real Matrix sidebar needs spaces (outer) → rooms (nested) tree. | `sidebar/spaces_rooms.rs` |
| P25 | 🟠 | `CommunitiesLayout` (Lemmy) is a **flat list** of servers. Real Lemmy sidebar has subscribed + local + all tabs (the ones visible in the demo_forum screenshot are ChannelList's built-ins, not ours yet). | `sidebar/communities.rs` |
| P26 | 🟠 | `FeedLayout` (HN) has 6 **hardcoded inert rows** (Top/New/Best/Ask/Show/Jobs). They're not clickable. Wire them as sidebar-items returned from the plugin via `get_sidebar_declaration`. | `sidebar/feed.rs`, `clients/hackernews/src/lib.rs` |
| P27 | 🟠 | `RepoTreeLayout` (GitHub) has **hardcoded Issues/PRs/Discussions** children. Plugin should declare these via `sidebar-section.items` with `children` via `parent_id` flat pattern. | `sidebar/repo_tree.rs`, `clients/github/src/lib.rs`, `clients/forgejo/src/lib.rs` |
| P28 | 🔴 | **ClientEvent::SidebarInvalidated** doesn't trigger a refetch yet. Per D19 this is event-driven. Wire the host event loop to kick `use_resource` invalidation on receipt. | `sidebar/mod.rs`, `state/*.rs` (event router) |
| P29 | 🟡 | **Sidebar fallback is silent.** On error from `get_sidebar_declaration`, we fall back to `ChannelListLayout`. User never knows the plugin failed. Add a discrete error badge or toast. | `sidebar/mod.rs` |
| P30 | 🟠 | **CustomSidebar icons not rendered** — `SidebarItem.icon: Option<IconSource>` is passed but `SidebarItemRow` shows label-only. Same fix as P15. | `sidebar/custom.rs` |

### 2.5 Composer + message actions (`crates/core/src/ui/client_ui/composer.rs`)

| # | Severity | Item | Files |
|---|---|---|---|
| P31 | 🔴 | **Composer buttons visually flat** — no hover, no active state, no tooltip FTL. Match existing `.toolbar-btn` styling better. | `composer.rs`, theme CSS |
| P32 | 🟠 | **Empty hook containers take layout space.** `ComposerHooks` renders three `<div>` slots (above/left/right of input) even when the plugin has no buttons. If `buttons.is_empty()`, render nothing. | `composer.rs` |
| P33 | 🟠 | **MessageActions renders after Forward** but has no separator. Insert a `<div class="message-action-separator">` when plugin actions are non-empty. | `composer.rs` |
| P34 | 🟡 | Same `ActionOutcome::Navigate`/`Toast` logged-but-not-actioned problem as the menu (P10/P11). Share a common helper. | `composer.rs`, `menu.rs` |
| P35 | 🟡 | **Icon rendering** same issue as menu — composer-button icons are passed through `icon: String` (emoji only, no SVG). WIT should migrate to `IconSource` for consistency. | WIT `composer-button`, `composer.rs` |

### 2.6 Host-universal menu items on non-chat backends

| # | Severity | Item | Files |
|---|---|---|---|
| P36 | 🟠 | "Mute Server" and "Hide Muted Channels" / "Show All Channels" toggles **appear on demo_forum / Lemmy** where they don't make sense. Strict D10 says they should move to plugins. Two options: (a) gate on capability flag (e.g. `has_channels`), or (b) move them into the relevant plugin declarations. | `crates/core/src/ui/account/server/context_menu.rs` |
| P37 | 🟠 | "Notification Settings" submenu arrow on forum backends: the navigation target is `ServerSettingsRoute` which may or may not exist for the backend. Verify routes resolve. | `server/context_menu.rs` |

### 2.7 Custom-block

| # | Severity | Item | Files |
|---|---|---|---|
| P38 | 🔵 | **Scoped CSS, not true shadow-root.** WP 5 punted on shadow-root. Plugins can theoretically break out of scoping if ammonia misses something. Upgrade to shadow-root via `document::eval` + `attachShadow`. | `custom_block.rs` |
| P39 | 🔵 | **SVG sanitizer allowlist** hasn't had a formal security audit. Check: can SVG `<use xlink:href="...">` reference external resources? Can CSS `background-image: url(javascript:...)` sneak through the stylesheet? | `custom_block.rs` |
| P40 | 🟡 | **No usage lint yet.** Per D4 / §9 risk: plugin authors over-using custom-block should trigger a review. Add a `custom_block_usage` counter to lint-gate. | `crates/lint-gate/build/custom_block_usage.rs` (new) |

### 2.8 Plugin data integration

| # | Severity | Item | Files |
|---|---|---|---|
| P41 | 🟡 | **`get_view_rows` returns empty** for Lemmy / HN / GitHub / Forgejo. Real API integration deferred in WP 5. Hook up the HTTP fetchers that probably already exist in those backends. | `clients/{lemmy,hackernews,github,forgejo}/src/lib.rs` |
| P42 | 🟡 | **`get_view_detail` returns NotSupported** for all backends except demo_forum. Same deferred-integration. Detail render (post body + comments) waits on this. | same |
| P43 | 🟠 | **Lemmy context-menu items are non-conditional** — "Subscribe" shows even when already subscribed, "Unsubscribe" logic absent. Menu declaration should inspect state: `if subscribed { Unsubscribe } else { Subscribe }`. Plugin needs state access. | `clients/lemmy/src/lib.rs` |
| P44 | 🟡 | Same for Discord (Mute/Unmute, Favorite/Unfavorite conditional) — every backend's menu could benefit from state-aware declarations. | every `clients/<backend>/src/lib.rs` |

### 2.9 FTL coverage

| # | Severity | Item | Files |
|---|---|---|---|
| P45 | 🟠 | **Missing FTL keys fall back to raw string.** The current `t()` behavior logs a warning but renders `plugin-foo-menu-bar-label` verbatim to the user. Either (a) show the fallback more gracefully (Title-Cased from key), or (b) make the build-time lint P21 more aggressive so missing keys fail the build, not runtime. | `crates/core/src/i18n/mod.rs`, `crates/lint-gate/build/ftl_label_key_coverage.rs` |
| P46 | 🟡 | **Some plugins lack `-desc` entries** for settings fields. Even when P23 lands (show desc next to label), entries need to exist. | each `clients/<name>/locales/en/plugin.ftl` |
| P47 | 🔵 | **Non-English locales not populated.** The plugin FTL files under `locales/de/`, `locales/es/`, `locales/fr/` were never updated for WP 2-6 declarations. Translators need a seed list of new keys. | every `clients/<name>/locales/{de,es,fr}/plugin.ftl` |

### 2.10 Accessibility

| # | Severity | Item | Files |
|---|---|---|---|
| P48 | 🟠 | **No ARIA on new components.** `ClientMenu` renders divs; should be `role="menu"` + `role="menuitem"`. Same for `PluginSettingsSection`, `ClientSidebar`. | every component file |
| P49 | 🟠 | **Custom-block content has no ARIA guidance.** Plugin authors need guidance on required ARIA attributes. Document in `docs/plugin-authoring.md`. | docs |
| P50 | 🟡 | **Keyboard navigation** on `ClientMenu` not wired (arrow keys, Escape). The existing `ServerContextMenu` uses click-only too — check whether existing chat menu has keyboard support; if yes, match it; if no, add to both. | `menu.rs` |

### 2.11 MCP polish

| # | Severity | Item | Files |
|---|---|---|---|
| P51 | 🟡 | **No capability-driven tool filtering.** Per phase-2.20 D4: MCP exposes all 18 new tools regardless of account. `context_menu_server` on HN always returns `[]` but the tool is advertised. Filter: hide tools where a synthetic `get_*` call against the account's plugin returns consistently empty. | `mcp/chat-mcp/src/tools.rs` |
| P52 | 🟡 | **Old Discord-shaped tools coexist** with new surfaces. `list_friends`, `list_dms`, etc. stay — deprecation + eventual removal. | `mcp/chat-mcp/src/tools.rs` |

### 2.12 Leftover cleanup

| # | Severity | Item | Files |
|---|---|---|---|
| P53 | 🔵 | **`BackendCapabilities` fields tagged TODO(D12)** for removal (`presence`, `typing_indicators`, `reactions`, etc.) but not yet removed. Coordinate with all readers across the workspace. | `clients/client/src/types.rs` + readers |
| P54 | 🔵 | **`settings/accounts.rs:81 backend_emoji(slug: &str)` slug match** survived WP 7's sweep (outside the `as_str()` scanner rule). Should be replaced with plugin-declared icons. | `crates/core/src/ui/settings/accounts.rs` |
| P55 | 🔵 | **`forum_view.rs` still carries dead code** — `ForumPostView`, `ForumPostCard`, `ForumThreadView`, `ForumComment` kept "for continuity" per the WP 5 agent report. If no external callers survive, delete. | `crates/core/src/ui/account/common/forum_view.rs` |
| P56 | 🔵 | **UI snapshot goldens deferred** in WP 0. Still needed for regression safety. Demo-only for now is fine; add remaining backends as we get credentials. | `tests/snapshots/` |

### 2.13 Capability-gated surfaces (phase-2.20 leftover)

| # | Severity | Item | Files |
|---|---|---|---|
| P57 | 🟠 | **DMs/Friends/Notifications tabs unconditional** on non-chat backends. Phase-2.20 D1 — still not fixed. Gate on `backend_capabilities.dms / friends / notifications` fields. | `crates/core/src/ui/account/common/account_server_bar.rs` |
| P58 | 🟠 | **Notification filter enum hardcoded Discord** (All / Mentions / FriendRequests / ServerInvites / VoiceInvites / Other). Still shows "Voice invites" filter on HN. Phase-2.20 D2. Plugin declares categories. | `crates/core/src/ui/account/common/notifications.rs` |
| P59 | 🟠 | **Message composer renders on read-only backends** (HN). Should be disabled / hidden. Phase-2.20 D11. Gate on capability. | `chat_view.rs` |
| P60 | 🟠 | **Unconditional routes** like `/hackernews/.../friends` render an empty shell instead of 404/redirect. Phase-2.20 D5. Gate routes on capability. | `crates/core/src/ui/routes.rs` |

### 2.14 Voice / video surface

| # | Severity | Item | Files |
|---|---|---|---|
| P61 | 🟠 | **Mic / speaker buttons** in account bar render on non-voice backends. Same capability gate as P57. | `crates/core/src/ui/account/common/account_bar.rs`, `voice_banner.rs` |

---

## 3. Proposed Sequencing

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

## 4. Principles

1. **One pack per PR.** Each pack lands cleanly green (`cargo check --workspace` + `cargo test -p poly-core --lib`) before the next begins.
2. **Every pack ships its tests** (per D31). Visual changes get Playwright snapshot diffs; behavioral changes get unit tests + e2e harness assertions.
3. **Don't regress WP 0-9.** If a polish item requires touching code that WP 2-6 established, the change preserves the plugin-declarative pattern (no hardcoded slug matches creeping back).
4. **Prefer declarative CSS over inline styles.** Every theme tweak goes in the existing theme files (not inline `style: "..."`).
5. **Leave no TODO-less stubs.** Every deferred follow-up from this plan gets a tracked comment referencing this document's item number (`// P23: field descriptions`).

---

## 5. What this plan doesn't cover

- Re-architecting anything in `chat_view.rs`. It stays at 223 KB per D8.
- True shadow-root implementation requires deep Dioxus/WASM work (P38) — included but non-trivial.
- Running new WIT migrations. All items here work within the interfaces landed in WP 1.
- Per-backend styling (e.g. making Lemmy look exactly like lemmy.ml). Only covered generically.

---

## 6. Open questions

1. **Toast system** (P11) — do we have one already? If not, is this one component or a queue + component?
2. **Event routing for SidebarInvalidated** (P28) — where does the host's `ClientEvent` stream get consumed today? Probably `crates/core/src/state/` — confirm during execution.
3. **Keyboard nav** (P50) — match existing or add fresh? The existing ServerContextMenu is click-only so there may be no reference implementation.
4. **Custom-block shadow-root** (P38) — does Dioxus support `ref` hooks well enough to attach a shadow-root on mount? If not, eval-based is a fallback.
5. **Plugin state awareness** (P43/P44) — the plugin's menu declaration is a pure function of `(target, target_id)` today. Adding state (subscribed vs not) implies the plugin's declaration function fetches state. That's fine but changes the mental model from "static declaration" to "live query". Document the expectation.

---

## 7. Execution Order (Recommendation)

**Quick wins (hours):** Pack A (visual polish only) + P32/P33/P16/P29.

**Medium (days):** Pack B (interaction wiring) + Pack C (storage).

**Deep (weeks if done thoroughly):** Pack E (real API integration per-backend), Pack F (capability gating migration), Pack G (shadow-root + security).

**Ongoing:** Pack H (cleanup) + Pack I (i18n) — split across multiple small PRs.

---

## 8. Acceptance Criteria (per pack)

- **Pack A:** screenshots before/after of every affected surface. No console errors. `cargo check` green.
- **Pack B:** every `ActionOutcome` variant demoed working on at least one backend. Toast + spinner visible in Playwright snapshot.
- **Pack C:** a setting saved on Discord survives a page reload. ChannelSettingsPage reachable at a stable route.
- **Pack D:** Matrix sidebar shows an actual tree. Lemmy sidebar has functional Subscribed/Local/All. HN feed tabs clickable.
- **Pack E:** real posts visible on Lemmy/HN/GitHub (at least one end-to-end integration).
- **Pack F:** HN account shows no DMs/Friends/Notifications tabs. No "Voice invites" filter.
- **Pack G:** `<script>`-in-custom-block test passes shadow-root isolation. Security audit report committed.
- **Pack H:** zero `TODO(D12)` comments in workspace. `cargo check --all-features` green.
- **Pack I:** every declared label-key resolves in at least the English bundle; non-English seeds stubbed with `// TODO: translate`.

---

## 9. Total

**61 items** across **9 packs**. Each item has a size (not filled in — fill during triage). Severity split:

- 🔴 visible bugs: **10** (P3, P10, P11, P18, P28, plus some 🟠s that are actually user-visible)
- 🟠 rough edges: **~30**
- 🟡 deferred features: **~15**
- 🔵 architectural debt: **~6**

Reading order during execution: start from §3 packs; within a pack, start from the highest-severity items. Tests go in the same PR as the code change (D31).
