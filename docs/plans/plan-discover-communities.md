# Plan: Discover Communities (Lemmy + Reddit)

> Owner: alexander.stuermer@aareon.com
> Created: 2026-05-03
> Status: 🧱 DRAFT — pending user approval

## Background

Two regressions + one missing UX have piled up in the Lemmy / Reddit
forum-style backends:

1. **Lemmy sidebar regression** — `SidebarLayoutKind::Communities` (the
   tabbed Subscribed / Local / All UI shipped in P25) was the wrong fit.
   Subscribed communities are *already* the items in Bar 1 (server bar);
   duplicating them as a Bar 2 channel-area tabset is redundant. The
   tab-area should instead behave like Discord channels: list the *sort
   modes* (Hot / Active / Scaled / Controversial / New / Old / Most
   Comments / New Comments / Top Hour / Top Day / Top Week / …) for the
   *currently-selected community* on Bar 1.
2. **Reddit sidebar regression** — Reddit currently uses
   `SidebarLayoutKind::Feed`, which renders the **HN-hardcoded** feed
   list `Top / New / Best / Ask / Show / Jobs`. Wrong list, wrong shape.
   Reddit needs sort-mode-as-channel (`Hot / New / Top / Rising /
   Controversial`) per subreddit.
3. **No "Discover Communities" page** — there is no UI for the user to
   browse Local / All communities (Lemmy) or search subreddits (Reddit)
   without already having subscribed to them. The existing flow is
   "either it's in Bar 1 or it doesn't exist."

Reference: see screenshots saved at
`~/.cache/tmux-paste-image/image_2026-05-03_16-{53,57,58,59}-*.png` and
the lemmy.zip web UI for the canonical sort-mode dropdown + Posts /
Comments tab placement.

## Goals

1. **Revert the Communities sidebar layout for Lemmy** — switch Lemmy
   to a sort-mode-driven channel list ("channels = sort modes") that
   matches Reddit's intended shape.
2. **Replace Reddit's Feed layout** with the same sort-mode-as-channel
   shape, populated from Reddit's actual sorts (`Hot / New / Top /
   Rising / Controversial` + `Top: hour / day / week / month / year /
   all`).
3. **Add a "Discover Communities" account-level route** alongside
   Notifications:
   - Route: `/:backend/:instance_id/:account_id/discover`
   - Lemmy: tabs `Subscribed | Local | All` + search field, paginated
     results, click → preview the community in Bar 2/3 OR subscribe.
   - Reddit: search field only (single instance), results, click → open
     OR subscribe (logged-in) / favourite (anonymous-mode local-DB
     only).
4. **Add a Posts / Comments toggle at the top of Lemmy's forum view**
   (mirroring lemmy.zip; Reddit doesn't have this — its post detail
   *is* the comments page).
5. **Sidebar nav button**: a "Discover" icon under the Notifications
   bell (Bar 1 footer area), shown only for backends whose plugin
   declares `CommunitySearch` capability.

## Non-Goals

- No NSFW filter UI in this plan (Lemmy `nsfw_warning` already exists
  in settings; honour it but don't expand).
- No multi-instance Lemmy federation switcher in this plan — current
  instance only.
- No real-time community-list updates (refresh on tab focus + manual
  reload only).

---

## Phase A — Sort-mode-as-channel sidebar layout (`SidebarLayoutKind::SortModes`)

A new layout kind that takes a backend-declared list of `(action_id,
label_key, group_label_key?)` tuples and renders them as a Discord-
style channel list under the active server (subreddit / community) in
Bar 1.

- [ ] **A.1** Add `SidebarLayoutKind::SortModes` variant to
      `clients/client/src/lib.rs::SidebarLayoutKind` (NOT renaming
      `Feed`; `Feed` stays for HN since its model genuinely is a
      static feed list, not per-server sort).
- [ ] **A.2** Add a `sort_modes: Vec<SidebarItem>` carrier on
      `SidebarDeclaration` (already there as `sections` — reuse the
      first section's items rather than adding a parallel field).
- [ ] **A.3** Implement `crates/core/src/ui/client_ui/sidebar/sort_modes.rs`
      following the `feed.rs` shape: clickable rows that dispatch
      `invoke_sidebar_action(action_id)`. Selected row gets a `selected`
      class. Sub-grouping (e.g. Reddit's `Top: hour / day / week`) via
      a collapsible `<details>` per group.
- [ ] **A.4** Wire it into `crates/core/src/ui/client_ui/sidebar.rs`
      next to the existing `Feed` / `Communities` / `RepoTree`
      branches.
- [ ] **A.5** FTL keys: `ui-sidebar-sort-{hot,active,scaled,…}` for
      Lemmy + `ui-sidebar-sort-{reddit-hot,reddit-new,…}` for Reddit.
      Keep the current Reddit-uses-HN-FTL strings as transitional
      `# DEPRECATED` aliases for one release.

## Phase B — Lemmy: drop CommunitiesLayout, ship SortModes

- [ ] **B.1** Change `clients/lemmy/src/lib.rs::get_sidebar_declaration`
      to return `SidebarLayoutKind::SortModes` with the 12 sort modes
      from the lemmy.zip dropdown (Hot / Active / Scaled / Controversial
      / New / Old / Most Comments / New Comments / Top Hour / Top Six
      Hours / Top Twelve Hours / Top Day / Top Week / Top Month / Top
      Year / Top All).
- [ ] **B.2** Wire `invoke_sidebar_action("sort-hot")` → set the
      lemmy backend's current sort + reload posts via `chat_data`.
      Currently the sort is selected in `LemmyBackend`'s settings;
      reuse the existing sort-state plumbing (don't re-invent).
- [ ] **B.3** Delete `crates/core/src/ui/client_ui/sidebar/communities.rs`
      and its `CommunitiesLayout` export from `sidebar.rs`. Remove the
      `Communities` variant from `SidebarLayoutKind` (or keep it as a
      `#[deprecated]` shim returning `SortModes` for one release).
- [ ] **B.4** Smoke: navigate to a subscribed Lemmy community, verify
      the channel list shows the 16 sort modes with the active one
      highlighted; click a row, verify the post list re-orders.

## Phase C — Reddit: drop FeedLayout abuse, ship SortModes (shipped in commit `178f58bc`)

- [x] **C.1** Change `clients/reddit/src/backend.rs::get_sidebar_declaration`
      to return `SidebarLayoutKind::SortModes` with Reddit's 5 sorts
      (`hot / new / top / rising / controversial`) + a "Top by"
      sub-group (`hour / day / week / month / year / all`).
- [x] **C.2** Wire `invoke_sidebar_action("sort-reddit-hot")` etc.
      to set the Reddit backend's current sort and reload posts.
- [ ] **C.3** Smoke: navigate to a subreddit, verify Hot/New/Top/…
      replace the old Top/New/Best/Ask/Show/Jobs (HN) labels.

## Phase D — Posts / Comments toggle (Lemmy only) — shipped

- [x] **D.1** Add `view_filter: PostsOrComments` enum to
      `crates/core/src/state.rs` (or wherever the lemmy forum scope
      lives) with default `Posts`.
- [x] **D.2** In `forum_view.rs`'s ClientView header, render two
      pill-buttons "Posts | Comments" when the active backend's
      `BackendCapabilities::supports_comment_feed` is true (new flag,
      defaults false; Lemmy sets it true).
- [x] **D.3** When `Comments` is active, swap the data source from
      `get_messages(channel)` to `get_comments(channel)` (new backend
      method on Lemmy returning `Vec<Message>` of recent comments
      across the community). Implemented via synthetic `lemmy-comments-{id}`
      channel prefix; `get_channel_view` + `get_view_rows` detect it.
- [x] **D.4** Add the existing `Filter…` text input next to the
      Posts|Comments pill buttons, debounced 250 ms via `gloo_timers`,
      threaded into `ClientView` via new `forum_filter: Option<String>` prop.

## Phase E — DiscoverCommunities route + page

- [ ] **E.1** Add `BackendCapabilities::community_search:
      CommunitySearchSupport` enum (`None / Single / Subscribed_Local_All`).
- [ ] **E.2** Add `ClientBackend::search_communities(query: &str,
      scope: CommunityScope, cursor: Option<String>) -> ClientResult<
      CommunityPage>` — `CommunityPage { items: Vec<Server>,
      next_cursor: Option<String> }`.
- [ ] **E.3** Implement on Lemmy via `/api/v3/community/list?type_=
      {Subscribed,Local,All}&q={query}` + on Reddit via
      `/subreddits/search.json?q={query}`.
- [ ] **E.4** Add route `/:backend/:instance_id/:account_id/discover`
      → `DiscoverCommunitiesView { … }` route component in
      `crates/core/src/ui/routes.rs`.
- [ ] **E.5** Implement `DiscoverCommunitiesView` in
      `crates/core/src/ui/account/common/discover_communities.rs`:
      header (search input + scope tabs), results CardGrid (reuse
      `ListBodyRow`), each card has Subscribe / Favourite / Open
      actions matching `community_search` capability.
- [ ] **E.6** Reddit anonymous-mode favourite path: persist locally
      to `poly_kv` under `reddit.favourites.<account_id>` so the
      favourited subreddit shows up in Bar 1 alongside subscribed ones
      (use the existing `favorited_server_ids` list).
- [ ] **E.7** Sidebar nav: add a "Discover" icon under the
      Notifications bell in `crates/core/src/ui/sidebar.rs` (Bar 1
      footer area), visible only when the active backend's
      `community_search != None`.

## Phase F — Tests + smoke

- [ ] **F.1** Unit tests for the new SortModes layout (renders the
      right number of rows, dispatches the right action_id on click).
- [ ] **F.2** Integration test: full direct-deep-link flow for Lemmy
      `/lemmy/lemmy.world/lemmy-beaver/discover` → search "rust" →
      results render → click Subscribe → row vanishes, server appears
      in Bar 1.
- [ ] **F.3** Integration test: same for Reddit anonymous mode →
      favourite a subreddit → check `poly_kv` row exists and the
      sub appears in Bar 1.
- [ ] **F.4** Update `TEST_HARNESS.md` step 6.5 to include direct nav
      to a discover URL on cold-boot.

## Open questions

- Does the existing `Server` shape carry enough metadata to render a
  community card (icon, name, subscriber count, NSFW flag, short
  description)? **A:** Lemmy already populates icon_url, name; need to
  add `subscriber_count: Option<u32>` and `nsfw: bool` fields. Reddit
  already returns `subscriber_count` via the search JSON.
- Where does the "Discover" sidebar icon live exactly? Below the
  Notifications bell in Bar 1, or as a footer button next to the cog?
  Mock both; let user pick during Phase E.
- For Lemmy, should the search auto-issue `?q=` against `/api/v3/
  community/list` or use the dedicated `/api/v3/search?type_=
  Communities` endpoint? Pick the latter — it sorts by relevance.

## Rollout order

1. Phase A + B + C (sidebar refactor) as one PR — both lemmy + reddit
   move off broken layouts in a single landing.
2. Phase D (posts/comments toggle) as a follow-up — independent of E.
3. Phase E (discover page) as a third PR — biggest surface area.
4. Phase F (tests) ride along with each phase, not as a separate PR.

---

## Status: 🧱 DRAFT
