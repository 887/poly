# Plan — Per-account Overview view + Search-affordances cleanup

## Status: ✅ DONE — all phases shipped (Phase 1 through Phase 5 landed across multiple commits)

## Context

User feedback after walking the GitHub plugin:

- The existing `/overview` page (used by GitHub + Forgejo today) is a nice per-account landing page, but it's the *only* backend that has one. Other backends drop the user on DMs / first-server / notifications instead.
- That same overview page renders **without the channel sidebar**, so the mobile pattern (Bar 1 / Bar 2 / channel sidebar / content) breaks: the user sees Bar 1 + Bar 2 + content, no third column.
- The bottom-left magnifier (global search) currently scopes to the active account when clicked from inside an account — the user wants it to always be truly global, no surprise scoping.
- The per-account search affordances (`AccountFilter` in `crates/core/src/ui/search.rs:188-219`) use raw `<input type="checkbox">` and lack a "select / deselect all" master toggle, breaking visual consistency with the toggle UI used everywhere else in settings.

User decisions on the open questions:

1. **Overview is per-ACCOUNT, not per-server.** Per-server overviews already exist (each repo has its own page). The new affordance is one *Overview* button per account, first item in the AccountServerBar.
2. **WIT method is REQUIRED.** Every backend must implement `get_account_overview_view`; no `NotSupported` fallback. Forces consistency across all 11 plugin impls in one wave.
3. **Button placement: AccountServerBar (Bar 2), as the new FIRST item** — above the existing DMs / Friends / Notifications row.
4. **Overview is the DEFAULT landing page for every backend, unless the backend explicitly opts out.** That's the intent of the new affordance — the user always lands on the overview unless the plugin overrides it. Backends that prefer a different default (e.g. Discord may still want `DirectMessages`) declare `LandingPage` other than `Overview` explicitly in `capabilities_for_slug`.

Guiding principle (carried over from the moderation plan):
> Our client shouldn't be unified, it should be accurate to what the user expects for that backend.

→ Each plugin's overview content is plugin-defined. GitHub keeps its repo grid. Lemmy could surface subscribed communities. Discord/Teams/Matrix could show servers with unread/mention stats. Hacker News could show a "Top stories" card. The host renders the returned `ViewDescriptor` via the existing `ClientView` machinery — same body engines we already have (List / Card / Tree / Split).

---

## Strategy — 5 phases

### Phase 1 — WIT + trait + host route + landing default (1 agent, blocks Phase 2)

- Add `get-account-overview-view: func() -> result<view-descriptor, client-error>` to `wit/messenger-plugin.wit` next to the existing `get-channel-view` (line 1216 area).
- Add corresponding `get_account_overview_view(&self) -> ClientResult<ViewDescriptor>` to the `ClientBackend` trait in `clients/client/src/lib.rs:718-735`. **No default impl** — required.
- Refactor the existing `Route::ServerOverviewRoute` at `crates/core/src/ui/routes.rs:360-361` (`/{backend}/{instance_id}/{account_id}/overview`):
  - Component now calls `client.get_account_overview_view()` instead of rendering the hardcoded `ServerOverviewPage`.
  - Renders the returned `ViewDescriptor` through the existing `ClientView` (`crates/core/src/ui/client_ui/view/mod.rs`) — same plumbing demo-forum / Lemmy / HN already use.
- Wrap the route render so that the **channel sidebar is always present** (i.e. the route renders inside the standard account layout shell with `ClientSidebar` on the left, like `ServerHome` / `ServerChat` already do). This fixes the missing-left-wing bug.
- **Make `Overview` the default landing page**:
  - Add `LandingPage::Overview` variant in `clients/client/src/types.rs` (next to `ServerOverview`/`FirstServer`/`DirectMessages`).
  - Update the `BackendCapabilities` struct defaults (`READ_ONLY_FEED`, `MESSAGING_NO_SOCIAL`, `FULL_SOCIAL_CHAT` — all in `clients/client/src/types.rs:743-841`) to set `landing: LandingPage::Overview`.
  - Audit `capabilities_for_slug` (`clients/client/src/types.rs:75-128`): each backend that previously declared `landing: DirectMessages` / `FirstServer` / `ServerOverview` either keeps its explicit override (if it really wants something else) or drops the override and inherits `Overview`. Sensible per-backend defaults to discuss in Phase 2.
  - Wire `LandingPage::Overview` into the AccountIcon click fallback in `crates/core/src/ui/favorites_sidebar.rs:698-731` so it routes to `Route::ServerOverviewRoute`.
  - The capability validator added in commit `27ac8b46` (skip stored-route restore when incompatible) needs an extra branch for `/overview` paths — overview is always compatible because every backend implements it.
- Drop `crates/core/src/ui/server_overview.rs` from the host (the `ServerOverviewPage` component moves into the GitHub/Forgejo plugin code in Phase 2).
- `cargo check --workspace` will fail until every backend implements the new trait method — that's the point of making it required. Phase 2 fixes it.

### Phase 2 — Per-backend overview impls (10 sonnet agents in parallel, worktree-isolated)

Each agent picks a sensible overview ViewDescriptor for its backend. The host already supports `ListBody` / `CardBody` / `TreeBody` / `SplitBody` — agents pick whichever fits their data. **No host changes.** Each agent only edits its own `clients/<backend>/src/lib.rs` + `clients/<backend>/src/guest.rs` + (optionally) backend-local FTL keys.

**Per-backend agents:**

- **A-GH GitHub** — port the existing `ServerOverviewPage` repo-grid logic from `crates/core/src/ui/server_overview.rs` into the plugin's `get_account_overview_view`. Uses `CardBody` (the existing rendering matches the card layout). All current functionality preserved.
- **A-FJ Forgejo** — same shape as GitHub. Repo grid via `CardBody`.
- **A-LE Lemmy** — `CardBody` of subscribed communities, with stats (subscribers, active users, unread posts).
- **A-DS Discord** — `CardBody` of guilds for this account, with member count + unread/mention badges.
- **A-MX Matrix** — `CardBody` of joined rooms / spaces with unread/mention badges.
- **A-ST Stoat** — `CardBody` of servers with member counts + unread.
- **A-TE Teams** — `CardBody` of teams with channel counts + unread/mention.
- **A-DM Demo** — three sub-backends (`demo`, `demo_chat`, `demo_forum`). Each gets a sensible overview: `demo` / `demo_chat` show server cards; `demo_forum` shows community cards (mirrors Lemmy).
- **A-PS poly-server** — `CardBody` of joined servers.
- **A-HN Hacker News** — `ListBody` showing the current top-N stories as a curated welcome view.

DO NOT touch (cross-agent guard): each agent only edits its own `clients/<name>/` directory. Locale changes go in `clients/<name>/locales/{en,de,es,fr}/plugin.ftl` (each backend already has its own FTL bundle). No agent edits `wit/messenger-plugin.wit`, `clients/client/`, or `crates/core/`.

**Mandatory verification per agent** (per CLAUDE.md "MANDATORY before the subagent exits"):
1. `jj describe -m "feat(<backend>): get_account_overview_view returning <body-kind>"`.
2. `jj log -r 'worktree-agent-<id> & description("<backend>")' --no-graph -T 'commit_id.short()'` and paste in final message.
3. Local `cargo check -p poly-<backend>` clean.

### Phase 3 — AccountServerBar Overview button (1 agent)

- Add a new `AccountBarOverviewButton` as the FIRST item in `AccountServerBar` (`crates/core/src/ui/account/common/account_server_bar.rs`), above the existing DMs / Friends / Notifications row.
- Icon: probably the existing `home` / `compass` SVG; FTL key `account-bar-overview-tooltip` in `locales/{en,de,es,fr}/main.ftl`.
- Route target: `Route::ServerOverviewRoute { backend, instance_id, account_id }`.
- Active state: route matches.

### Phase 4 — Search-affordances cleanup (1 agent)

Two independent fixes in `crates/core/src/ui/search.rs`:

- **Global magnifier always-global**: in `crates/core/src/ui/favorites_sidebar.rs:320-350`, change the magnifier onclick to ALWAYS push `Route::SearchRoute` (no `locked_account_id`). The per-account search affordance lives elsewhere (see existing `ConversationSearchView` and the per-server search tab); the global magnifier should always be truly global.
- **AccountFilter UI**: in `crates/core/src/ui/search.rs:188-219`:
  - Replace each `<input type="checkbox">` with the `ToggleRow` pattern from `crates/core/src/ui/account/settings/content_social.rs:72`. Concrete: extract `ToggleRow` into a shared component (`crates/core/src/ui/components/toggle.rs` if not already shared) and call it here.
  - Add a master "All" toggle at the top of the column. State: indeterminate when partial, on when all enabled, off when none. Click toggles all on/off.

### Phase 5 — Verify in browser (orchestrator-driven)

For each of the 11 backends, manual smoke-test in poly-web:
- Click the account icon → lands on `/overview` by default (NOT DMs / first server) → channel sidebar visible on the left → overview body renders the plugin's ViewDescriptor.
- For backends that explicitly opted out (e.g. Discord may keep `DirectMessages`), confirm the override still works.
- Click the AccountServerBar Overview button on any account → routes to `/overview`.
- Click an overview entry (server/community/repo) → navigates into that server.
- Click the bottom-left magnifier from inside an account → opens `/search` with NO account locked.
- On `/search`, click "All" → all account toggles flip; click again → all flip off.

---

## Critical files

- `wit/messenger-plugin.wit` — add `get-account-overview-view`
- `clients/client/src/lib.rs:718-735` — add trait method
- `clients/client/src/types.rs` — no schema changes (reuse `ViewDescriptor`)
- `crates/core/src/ui/routes.rs:360,1839` — refactor `ServerOverviewRoute` to render `ClientView`
- `crates/core/src/ui/server_overview.rs` — delete (logic migrates into github/forgejo plugins)
- `crates/core/src/ui/client_ui/view/mod.rs` — already supports `ViewDescriptor` rendering, no changes needed
- `crates/core/src/ui/account/common/account_server_bar.rs` — new Overview button
- `crates/core/src/ui/favorites_sidebar.rs:320-350` — magnifier always-global
- `crates/core/src/ui/search.rs:188-219` — AccountFilter → toggles + All master
- `crates/core/src/ui/account/settings/content_social.rs:72` — `ToggleRow` source pattern (extract to shared)
- `clients/{discord,forgejo,github,lemmy,matrix,stoat,teams,demo,server-client,hackernews}/src/lib.rs` + `guest.rs` — implement `get_account_overview_view`
- `locales/{en,de,es,fr}/main.ftl` — overview button FTL key
- `clients/{name}/locales/{en,de,es,fr}/plugin.ftl` — backend-local overview strings (server-card label keys, etc.)

## Reusable patterns already in tree

- **`ViewDescriptor` rendering**: `crates/core/src/ui/client_ui/view/mod.rs` (`ClientView`) — already used by demo-forum, Lemmy, HN. Body engines `list_body.rs` / `card_body.rs` / `tree_body.rs` / `split_body.rs` cover every shape we need.
- **`get_channel_view` pattern**: `clients/lemmy/src/lib.rs:678-700` is a clean reference impl. Account overview follows the same shape, just no channel-id parameter.
- **Demo's per-scope `get_view_rows`**: `clients/demo/src/lib.rs:1400-1445` shows how to handle different "tabs" — pattern reusable if any backend wants a sort/filter inside its overview.
- **Capability gating**: `BackendCapabilities::should_show_dms/friends/etc.` (`clients/client/src/types.rs:799-816`) is the model. We do NOT add a `has_overview` flag — overview is required, not optional.
- **`ToggleRow`**: `crates/core/src/ui/account/settings/content_social.rs:72` — extract to shared, reuse from `AccountFilter`.

## Verification

- `cargo check --workspace` clean after Phase 1 will FAIL (every plugin missing the new method) → green only after all of Phase 2 lands. That's the gate.
- `cargo test --workspace` covering existing plugin tests + new tests per-backend asserting `get_account_overview_view` returns a non-empty ViewDescriptor.
- Manual via poly-web: walk all 11 backends per Phase 5.
- Visual: per the screenshots, the channel sidebar must be visible on the overview page (third column never disappears).
- The `/search` page after Phase 4 has all toggle-style account rows + a single "All" master toggle.
