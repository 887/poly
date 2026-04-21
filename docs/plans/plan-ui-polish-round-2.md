# Plan — UI Polish Round 2

> **Created:** 2026-04-21
> **Status:** 🟥 PLANNED — pending implementation kickoff
> **Audit data:** `docs/plans/ui-polish-round-2/`
> **Predecessor:** `docs/plans/plan-client-ui-polish.md` (Round 1, ✅ shipped 2026-04-18)
> **Trigger:** post-merge of test-account auto-signin + offline-account sidebar work, the user opened every animal account in the dev app and reported broken account-bar layout, native right-click menus everywhere, and a Teams crash. This plan covers the audit findings and the resulting fixes.

---

## 0. How this plan is organised

The audit fanned out into seven reports (three code-only, four visual). This document indexes them, calls out the cross-cutting findings, and then lists the fixes as a phased checklist.

| Report | Path | Scope |
|---|---|---|
| Visual Audit Index | [`ui-polish-round-2/visual-INDEX.md`](ui-polish-round-2/visual-INDEX.md) | Per-backend issue counts, top-5 cross-backend issues, screenshot map |
| Visual: Demo | [`ui-polish-round-2/visual-demo.md`](ui-polish-round-2/visual-demo.md) | Cat, Dog, Platypus |
| Visual: Discord | [`ui-polish-round-2/visual-discord.md`](ui-polish-round-2/visual-discord.md) | Koala, Kangaroo |
| Visual: Matrix | [`ui-polish-round-2/visual-matrix.md`](ui-polish-round-2/visual-matrix.md) | Owl, Axolotl |
| Visual: Stoat | [`ui-polish-round-2/visual-stoat.md`](ui-polish-round-2/visual-stoat.md) | Stoat, Raccoon |
| Visual: Teams | [`ui-polish-round-2/visual-teams.md`](ui-polish-round-2/visual-teams.md) | Sheep, Walrus (CRITICAL — WASM crash) |
| Visual: Forgejo | [`ui-polish-round-2/visual-forgejo.md`](ui-polish-round-2/visual-forgejo.md) | Otter, Flamingo |
| Visual: GitHub | [`ui-polish-round-2/visual-github.md`](ui-polish-round-2/visual-github.md) | Penguin, Chameleon |
| Visual: Lemmy | [`ui-polish-round-2/visual-lemmy.md`](ui-polish-round-2/visual-lemmy.md) | Beaver, Hedgehog |
| Code: layout-collapse diagnosis | [`ui-polish-round-2/audit-layout-collapse.md`](ui-polish-round-2/audit-layout-collapse.md) | Account-bar / voice-bar pushed up to channel-list end |
| Code: native right-click leakage | [`ui-polish-round-2/audit-native-rclick-leakage.md`](ui-polish-round-2/audit-native-rclick-leakage.md) | Macro is a no-op + global guard absent |
| Code: `context_menu(inherit)` audit | [`ui-polish-round-2/audit-context-menu-inherit.md`](ui-polish-round-2/audit-context-menu-inherit.md) | 284 sites classified, 72 should be `none`, 20 missing menus |

Screenshots: 121 PNGs under `docs/plans/ui-polish-round-2/screenshots/<backend>/`.

---

## 1. Headline findings

The audit surfaced four foundational bugs that explain a large fraction of the symptoms across backends. Fixing these unlocks several "smaller" issues at once.

| # | Finding | Source | Blast radius |
|---|---|---|---|
| **F1** | **Account-bar / voice-bar pushed up to channel-list end on every non-demo backend.** Root cause: `.channel-list-wrapper { height: 100% }` doesn't resolve inside an `overflow: visible` parent (`.poly-split-shell.account-view-main`). | layout-collapse audit | All non-demo backends |
| **F2** | **The `#[context_menu(...)]` macro is a pure validator — it injects no DOM handler.** All 318 annotations across the workspace are documentation-only with zero runtime effect. The previously-claimed global `oncontextmenu: prevent_default` guard at `main_layout.rs:299` (commit `f627d9fc`) is **absent from the current source**. | native-rclick audit | Every right-click target in the app |
| **F3** | **`UserRowContextMenu` is fully authored but wired to zero call sites.** Right-clicking any user row in the member list, DM sidebar, or DM contact panel suppresses the native menu (where a manual handler exists) but shows nothing. | inherit audit | All user rows in DMs / member lists / contact panels |
| **F4** | **Teams backend WASM hard freeze on every account-avatar click.** Reproduced 5/5. Sheep yielded 4 screenshots in one lucky pre-crash window; Walrus is entirely inaccessible. | visual-teams | Sheep, Walrus → entire Teams backend |

Plus three cross-backend recurring symptoms (top of [`visual-INDEX.md`](ui-polish-round-2/visual-INDEX.md)):

- **R1** Direct URL navigation always redirects to Settings (Teams, Discord, Stoat, Forgejo, GitHub, Lemmy — every non-demo backend). Sidebar Link components work; `dx serve` deep-links and browser back/forward do not.
- **R2** Server / repository / space icons render as letter-initial circles instead of the actual remote image. Affects Discord, Matrix, Stoat, Forgejo, GitHub, Lemmy, Teams.
- **R3** Per-account settings (⚙ in account-bar) shows generic Discord-style options ("Friends join voice channels", "Incoming Ring") for backends like Lemmy / Teams where those concepts are meaningless. Settings are not gated by `BackendCapabilities`.

---

## 2. Phased fix plan (checklist)

Phases are ordered by **blast radius / dependency**: phase A unblocks the most pages with the smallest diff; phase D is per-backend feature work that depends on phase A's macro fix and phase B's routing fix.

### Phase A — Foundation fixes (1 commit each, all small)

The four highest-leverage one-line/few-line fixes. Land these first; they make the rest easier.

- [ ] **A1. Restore `.channel-list-wrapper` height resolution.** In `crates/core/assets/styling/account-shell.css` (~line 235), replace `height: 100%;` with `align-self: stretch;`. Fixes F1 (account-bar / voice-bar layout collapse) on every non-demo backend.
- [ ] **A2. Add the missing global `oncontextmenu` guard.** In `crates/core/src/ui/main_layout.rs` (~line 295, on the `.main-layout` root `<div>`), add `oncontextmenu: |evt| evt.prevent_default()`. This was the §4.5.1 of `plan-context-menu-quality-control.md` claimed-shipped at `f627d9fc` but absent today. Re-landing it suppresses the native browser menu on every surface in one diff. Fixes F2 user-visible symptom.
- [ ] **A3. Wire `UserRowContextMenu` to the three call sites that need it.** Per [`audit-context-menu-inherit.md` §2](ui-polish-round-2/audit-context-menu-inherit.md): `DmMemberRow`, `DmContactRow`, `UserSidebar` member list. The menu definition exists in `ui/context_menu/menus.rs`; the work is one annotation per site. Fixes F3.
- [ ] **A4. Diagnose & fix the Teams WASM freeze.** Per [`visual-teams.md`](ui-polish-round-2/visual-teams.md) and `CLAUDE.md` "Debugging hard WASM hangs" recipe: bisect-warn the Teams plugin's account-activation closure; the first warn that doesn't appear is the offending Signal-write chain or `use_effect`-on-self-write. Likely site: `clients/teams/src/lib.rs` `authenticate()` callback or the Teams equivalent of `commit_to_client_manager`. Fixes F4.

### Phase B — Macro upgrade (Phase B of `plan-context-menu-quality-control.md`)

- [ ] **B1. Make the `#[context_menu(...)]` macro inject DOM handlers.** Currently it's a pure validator (`crates/ui-macros/src/context_menu.rs`, 152 lines, no `oncontextmenu` injection). Implement:
  - `#[context_menu(none)]` → injects `oncontextmenu: |evt| evt.prevent_default()` on the root element.
  - `#[context_menu(allow_default)]` → injects `oncontextmenu: |evt| evt.stop_propagation()` so the global guard from A2 doesn't fire.
  - `#[context_menu(SomeMenuEnum)]` → injects `oncontextmenu` that calls `evt.prevent_default()` + `open_menu::<SomeMenuEnum>(evt, props_as_menu_ctx)`.
  - `#[context_menu(inherit)]` → no injection (current behaviour, correct).
  Once this lands, the global guard from A2 becomes belt-and-suspenders rather than the only working layer, and `allow_default` actually works for text inputs / markdown anchors.
- [ ] **B2. Re-flip 14 `inherit` sites to `allow_default`** for text-edit surfaces that need the OS spellcheck / cut-copy-paste menu. List in [`audit-native-rclick-leakage.md` §3.2](ui-polish-round-2/audit-native-rclick-leakage.md): `MessageInlineEdit`, `NoteEditor`, `CssEditorArea`, `ChatStyleEditor` textareas, the compose `<textarea>`, password / text `<input>` fields.
- [ ] **B3. Re-flip `MessageContentView` markdown wrapper and `custom-block-content` to `allow_default`** so the OS "Open link / Save link" menu works on rendered markdown anchors.

### Phase C — `inherit` → `none` mass migration

Per [`audit-context-menu-inherit.md` §1](ui-polish-round-2/audit-context-menu-inherit.md), 72 sites use `inherit` where `none` is the correct semantic. ~55 are a single batch in the settings subtree. After A2 they're already runtime-correct via the global guard; this phase is about *being explicit* so a future macro change doesn't silently regress them.

- [ ] **C1. Settings-tree mass-flip (~55 sites).** Single commit, single sed-style edit across `crates/core/src/ui/settings/`, `crates/core/src/ui/account/settings/`, `crates/core/src/ui/account/server/settings/`. Full list in [`audit-context-menu-inherit.md` §1.1–§1.4](ui-polish-round-2/audit-context-menu-inherit.md).
- [ ] **C2. Modal / overlay / toast / wing host mass-flip (~10 sites).** [§1.5–§1.6](ui-polish-round-2/audit-context-menu-inherit.md): `LayoutToggleSwitch`, `WingShell`, `WingTabBar`, agent-panel sections, etc.
- [ ] **C3. The remaining ~7 misc sites.** [§1.7](ui-polish-round-2/audit-context-menu-inherit.md): tutorial overlays, splash, error overlays.

### Phase D — Genuine missing menus (the 20 sites that need a typed menu, not `none`)

Per [`audit-context-menu-inherit.md` §2](ui-polish-round-2/audit-context-menu-inherit.md). A1–A3 of phase A handle the most urgent (`UserRowContextMenu`). The rest:

- [ ] **D1. Wire existing menus to obvious sites:** `MessageContextMenu` to `MessageContentView`'s body wrapper (currently only the row container has it), `ChannelContextMenu` to `ChannelListItem` in forum-style channel lists, `ServerContextMenu` to all three server-icon variants (`AccountServerIcon`, `ServerIconDisplay`, `FavoriteServerIcon` already has it via raw handler — normalize via macro after B1).
- [ ] **D2. Author 4 new typed menus** for surfaces with no current menu but real expected actions:
  - **`AttachmentContextMenu`** — for in-chat image / video / file attachments (Open / Save as / Copy URL).
  - **`ReactionContextMenu`** — for the reaction chips on messages (Show who reacted / Remove my reaction).
  - **`AvatarContextMenu`** — for user avatars in message rows / member list (View profile / Send DM / Mention).
  - **`ForumPostContextMenu`** — partially defined for `ForumComment` but missing for `ForumPostCard` and the `HnFeedView` post rows. Finish wiring per [`audit-native-rclick-leakage.md` §4.4](ui-polish-round-2/audit-native-rclick-leakage.md).

### Phase E — Cross-backend functional bugs (R1, R2, R3, +)

- [ ] **E1. Direct URL navigation redirects to Settings on every non-demo backend (R1).** Investigate: the dx-fullstack server's catch-all may be falling through to `/settings` rather than serving `index.html` for unknown paths, OR the Dioxus router is missing a default-route hydration step on first paint. Repro via `curl http://localhost:3000/teams/localhost:9103/U001/dms` and checking the response. Fix at the router level, not per-backend.
- [ ] **E2. Server / repo / space icon images don't load (R2).** Affects all 7 non-demo backends. Inspect the actual `<img src=…>` URL in DevTools for one icon (e.g. Discord guild) and confirm the failure mode (CORS preflight failed, 401 from upstream, network error, image proxy not wired). Most likely fix: thread the auth token through the existing `client.fetch_image(url)` path instead of letting the browser fetch the URL directly.
- [ ] **E3. Per-account settings ignore backend capabilities (R3).** In `crates/core/src/ui/account/settings/`, gate each settings section on `client_manager.capabilities(&account_id).has_voice()`, `has_friends()`, etc. The `BackendCapabilities` field exists (per Round-1 plan §H — D12 deferred); use it.
- [ ] **E4. Issue / PR detail fails to load on click (Forgejo: "Failed to load detail", GitHub: stays on "Select an item").** [`visual-forgejo.md`](ui-polish-round-2/visual-forgejo.md), [`visual-github.md`](ui-polish-round-2/visual-github.md). Likely a `get_view_detail` plugin call returning the wrong shape; or the `detail.fetch_url` is constructed wrong for those backends.
- [ ] **E5. Boot hang watchdog fires too eagerly on Dog (demo) account switch.** [`visual-demo.md`](ui-polish-round-2/visual-demo.md). The "App not responding" overlay appears ~18s after the avatar click and the Reload button doesn't clear it without a manual `page_reload`. Either bump `BOOT_HANG_TIMEOUT_MS` in `crates/core/src/wasm_crash_handler.rs` for demo accounts with many DMs, or fix the underlying slow account-switch path so it actually finishes in time.
- [ ] **E6. Plugin sidebar fails to load intermittently on Discord first activation.** [`visual-discord.md`](ui-polish-round-2/visual-discord.md). "Plugin sidebar failed to load — showing channels" message appears in the channel list panel; transient (clears after navigating). Race between plugin init and first channel-list render.
- [ ] **E7. Hackernews test server fails to start.** Discovered while starting the test runner: `poly-test-hackernews` panics at `servers/test-hackernews/src/main.rs:243` with *"Path segments must not start with `:`. For capture groups, use `{capture}`."* — an axum 0.7 → 0.8 router-syntax regression that needs the `:id` → `{id}` migration applied to that file. Blocks any HN test data.

### Phase F — Per-backend feature gaps (each requires backend-specific work)

These are real backend gaps surfaced by the visual audit, not styling. Each is its own scoped commit (or sub-plan).

#### F-Lemmy
- [ ] **F-LE-1.** Subscribed communities are not listed as icons in the second nav. The nav is nearly empty (just 🔔 + +). Wire `lemmy::list_subscribed_communities()` to populate the second nav like Discord guilds. ([`visual-lemmy.md`](ui-polish-round-2/visual-lemmy.md) #1)
- [ ] **F-LE-2.** No way to browse / discover communities from the Poly UI. Add a "Browse Communities" entry — the existing "+" button currently shows a misleading "lemmy doesn't support creating servers" toast, repurpose it. ([`visual-lemmy.md`](ui-polish-round-2/visual-lemmy.md) #3)

#### F-Teams
- [ ] **F-TE-1.** "Unknown" contact in Teams DMs list — contact resolution failure. ([`visual-teams.md`](ui-polish-round-2/visual-teams.md) #3)
- [ ] **F-TE-2.** Teams server / channel icons render as colored letter-circles (C, P) without channel names. Wire the Teams channel-list `name` field through to the favorites sidebar. ([`visual-teams.md`](ui-polish-round-2/visual-teams.md) #5)
- [ ] **F-TE-3.** Per-account ⚙ button on Teams account-bar opens global settings instead of per-account. ([`visual-teams.md`](ui-polish-round-2/visual-teams.md) #6)

#### F-Discord
- [ ] **F-DC-1.** "You need the `VIEW_CHANNEL` permission" surfaces as an inline message instead of a styled empty-state. ([`visual-discord.md`](ui-polish-round-2/visual-discord.md) #2)
- [ ] **F-DC-2.** Account-bar ⚙ opens global settings (vs. per-account) — same as F-TE-3, fix once across backends.

#### F-Forgejo
- [ ] **F-FJ-1.** "forgejo doesn't support direct messages" renders as raw plain text in the main content area instead of a styled unsupported-feature empty state. ([`visual-forgejo.md`](ui-polish-round-2/visual-forgejo.md) #2)
- [ ] **F-FJ-2.** Repository icons render as letter-circles instead of repo avatars. (Subset of E2.) ([`visual-forgejo.md`](ui-polish-round-2/visual-forgejo.md) #4)

#### F-GitHub
- [ ] **F-GH-1.** Repository cards show only icon + name; no description / star count / language / last update. Card design needs more info. ([`visual-github.md`](ui-polish-round-2/visual-github.md) #2)
- [ ] **F-GH-2.** "No items" empty state for issue panel is plain text. Style with icon + helpful messaging. ([`visual-github.md`](ui-polish-round-2/visual-github.md) #3)

#### F-Matrix
- [ ] **F-MX-1.** Matrix DM contacts use `@username:server` MXID in some places and display name in others; pick one. ([`visual-matrix.md`](ui-polish-round-2/visual-matrix.md) #4)
- [ ] **F-MX-2.** Spaces icons in second nav render as letter-circles instead of Matrix space thumbnails (subset of E2). ([`visual-matrix.md`](ui-polish-round-2/visual-matrix.md) #2)

#### F-Stoat
- [ ] **F-ST-1.** Stoat server icons render as letter-circles. (Subset of E2.) ([`visual-stoat.md`](ui-polish-round-2/visual-stoat.md) #2)

#### F-Demo
- [ ] **F-DM-1.** Friends panel shows "No friends found" with no add-friend affordance for demo accounts. (Demo can't add friends to itself, so an empty state explaining that is fine.) ([`visual-demo.md`](ui-polish-round-2/visual-demo.md) #3)

---

## 3. Suggested rollout order

If shipping incrementally, the dependency-honest order is:

1. **Phase A** (one PR per item, four PRs total — all small).
2. **Phase E1, E2, E3, E5** in parallel (each has its own owner; no cross-dependencies).
3. **Phase B** (one PR — the macro upgrade unlocks B2, B3, C, D).
4. **Phase C** (single sweeping PR after B — explicit `none` annotations).
5. **Phase D** (one PR per typed menu).
6. **Phase F** (per-backend, no shared dependencies — easy to parallelise).
7. **Phase E7** (test-server fix — small, do whenever).

Estimated: phase A is 4-8 hours of work total. Phase B is a half-day macro change with broad regression-test surface. Phase C+D is 1-2 days. Phase E is 1-2 days. Phase F is per-backend; estimate per backend.

---

## 4. Test coverage requirements (inherits from Round 1)

Every fix lands its test matrix per the policy from `docs/plans/plan-client-ui-polish.md` §1. Specifically:

- **Phase A** items each get a unit test where applicable plus a manual verification screenshot in the PR.
- **Phase B** macro changes get golden tests in `crates/ui-macros/tests/` that snapshot the expanded TokenStream for each decorator variant.
- **Phase C** is mechanical and covered by the existing context-menu coverage CSV in `docs/plans/context-menu-coverage.csv` once it is regenerated post-flip.
- **Phase D** typed menus each get a `MenuKind` enum test verifying the variant exists and a wiring test confirming the host component declares the menu attribute.
- **Phase E** functional fixes get integration tests at the lowest layer that reproduces the symptom (router test for E1, capability test for E3, plugin test for E4, etc.).

Run TEST_HARNESS.md via a haiku subagent after each phase before merging.
