# Hacker News Backend — Visual Audit Report

**Status:** partial — code-complete read-only feed backend
**Accounts:** Anonymous guest (no login required; any credentials accepted)
**Date:** 2026-04-27
**Screenshots:** `screenshots/hackernews/` (not available; no live UI test session)
**Source:** `clients/hackernews/src/lib.rs`, `clients/hackernews/src/api.rs`

---

## Account Login

**No credentials required.** `authenticate()` accepts any `AuthCredentials` and always returns `Ok(guest_session)` with:
- User ID: `"anonymous"` (or named if `named_session()` called)
- Display name: `"Anonymous"`
- Avatar: Inline SVG orange HN badge
- Backend: `hackernews` / `news.ycombinator.com`

`logout()` clears the session. `is_authenticated()` returns `true` when a session exists. No sign-in failure modes for normal operation.

**UX consideration:** The UI should not show a "sign in" dialog for HN — the backend is unauthenticated by design and loading an account is immediate.

---

## Overview Page & Feed Channels

### Layout

`get_account_overview_view()` returns `ViewKind::FlatList` with:
- Header: `plugin-hackernews-overview-title` (localized title), `plugin-hackernews-overview-subtitle` (subtitle)
- Row template: `primary=title`, `secondary=author-domain`, `meta=points-comments-age`
- Page size: 30 items per fetch
- Paging: offset-based cursor (`CursorKind::Offset`)

### Feed Channels

`get_channels("hn")` returns 6 static feed channels:
- `hn-top` — Top Stories (homepage, highest voted)
- `hn-new` — New Stories (newest submissions, unvoted)
- `hn-best` — Best Stories (highest voted, curated)
- `hn-ask` — Ask HN (discussion threads)
- `hn-show` — Show HN (user projects, demos)
- `hn-jobs` — Jobs (job postings)

Single server: `id="hn"`, `name="Hacker News"` with orange icon and account association.

### Feed Fetching

`get_view_rows(channel_id, cursor, ...)` routes empty `channel_id` to `HnFeed::Top` (overview) and named channels to their feed type. Fetches live from `https://hacker-news.firebaseio.com/v0/{top|new|best|ask|show|jobs}stories.json`, then batch-hydrates item details. Dead/deleted items filtered client-side.

Pagination state: cursor value = offset integer; next cursor only emitted when a full page was fetched (supports "load more" UX).

### Comment Thread Navigation

Comment threads accessed via ephemeral `hn-post-{story_id}` channels — not in static `get_channels()` output. Host must:
1. Click a story row → `get_view_detail(channel_id, row_id)` → receives `ViewDetail.comments_section` with `TreeSpec { root_page_size: 30, max_depth: 8 }`
2. Host opens the post's detail view and threads messages via the comment tree
3. On "view thread" click, host calls `get_messages("hn-post-{id}", ...)` which triggers `get_comment_thread(story_id, limit)` — a BFS traversal of HN's tree structure preserving parent IDs

Thread depth: max 8 levels, up to 1000 comments per tree (clamped from query limit). Parent links preserve nesting for correct thread render.

---

## Channel Sidebar

`get_channels("hn")` returns the 6 feed channels. Second nav renders as:
- Single "Hacker News" server icon (orange HN badge, no image load)
- Sidebar shows channels as links with flat list (not nested by category)
- No DMs, no groups, no spaces

`get_dm_channels()` returns `Vec::new()` (read-only — no DM concept).

---

## Messaging & Read Operations

### Sending / Typing

`send_message()` returns `NotSupported("Hacker News is read-only; posting requires authentication via news.ycombinator.com")`.

`send_typing()` — trait default, returns `NotSupported`.

### Reading Messages

`get_messages(channel_id, query)` fully implemented:
- **Story feed channels** (`hn-{top|new|best|...}`): fetches feed IDs, applies cursor offset, batch-hydrates story items, filters dead/deleted, maps to `Message` rows. Respects `query.limit` (default 20).
- **Comment threads** (`hn-post-{story_id}`): calls `get_comment_thread()` for BFS traversal. Default limit 300 (raised from 20 for F6 deep-thread support), clamped to 1–1000. Each comment preserves parent ID for tree reconstruction.

### Other Read Ops

- `get_user(id)` — fetches from HN API, maps to `User` (karma, created-at, about-text)
- `get_friends()` — returns `Vec::new()` (HN has no friend concept)
- `get_channel_members()` — returns `Vec::new()` (stories have no member list)

---

## View Details & Metadata

`get_view_detail(channel_id, row_id)` for a story:
- Fetches story item from HN API
- Body: prefers `item.text` (Ask/Show posts); falls back to linked URL or title
- Comments section: emitted only if story has top-level comments (`kids` array non-empty)
  - `TreeSpec { root_page_size: 30, max_depth: 8 }`
  - Host calls `get_messages("hn-post-{story_id}")` to hydrate tree

Story metadata (displayed in rows):
- **primary:** story title
- **secondary:** author + domain (for links) or "author-domain" field
- **meta:** points + comment count + age (e.g., "42 points, 15 comments, 3 hours ago")

---

## Settings

`get_settings_sections()` returns `preferences` section with two fields:
1. **`default-feed`** (Select) — options: `["top", "new", "best", "ask", "show", "jobs"]`, default `"top"`
2. **`items-per-page`** (Slider) — range: 10–100, step 5, default 30

**Current status:** Fields declared in trait but NOT persisted. `get_setting_value()` and `set_setting_value()` use in-memory storage stub (SettingsStorageCell). Host KV persistence not wired (see "Known Gaps").

---

## Context Menus & Actions

`get_context_menu_items()` returns `Ok(Vec::new())` — no menu items declared (read-only feed, no server/channel/user actions).

`invoke_context_action()` returns `NotFound` for all action IDs.

All moderation/social ops use trait defaults (`NotSupported`):
- `kick_member`, `ban_member`, `unban_member`, `timeout_member`, `untimeout_member` — no member management
- `delete_message`, `edit_message` — read-only
- `block_user`, `ignore_user`, `unignore_user` — no user management
- `add_friend`, `remove_friend`, `set_friend_nickname`, `set_user_note` — no social graph
- `mute_conversation`, `unmute_conversation` — no DMs
- `close_dm_channel`, `leave_group_dm`, `edit_group_dm`, `add_users_to_group_dm`, `invite_user_to_server` — read-only

Correct design for an anonymous read-only feed backend.

---

## Real-Time Updates

`event_stream()` returns an empty pin-boxed stream (no WebSocket). New stories appear only on next `get_view_rows()` poll. HN's `/updates.json` endpoint exists but is not subscribed.

---

## Known Gaps / TODOs

1. **[HIGH] No live UI test account** — HackerNews was not added as a test account in the 2026-04-21 poly-web session. All verification is code-only. UI smoke-test needed for: comment thread navigation, feed pagination, story detail rendering, deep-thread BFS correctness.

2. **[HIGH] No search** — `search_messages()` not implemented. HN Firebase v0 has no server-side search API. Implementing would require:
   - Integrating Algolia HN Search API (`hn.algolia.com/api/v1/search`)
   - Caching/rate-limiting to avoid hitting Algolia quota
   - Search result mapping to HN item schema

3. **[MEDIUM] Settings not persisted** — `default-feed` and `items-per-page` declared but storage stub is in-memory only. Requires wiring to host KV bridge (`host_api.kv_set/get`). See Pack C P18 comments in code.

4. **[MEDIUM] No real-time updates** — `event_stream()` disabled. Enabling would require polling `/v0/updates.json` endpoint and emitting `ClientEvent::NewMessage` / `ClientEvent::MessageUpdated`. Currently users must manually refresh feed channels.

5. **[LOW] Comment thread ephemeral channel UX** — `hn-post-{id}` channels not in static list. Host must infer them from story click → `get_view_detail()` response. Clear UX pattern exists (e.g., Discord thread opening) but needs integration test to confirm sidebar/routing handles it correctly.

---

## Phase-5 Code Audit (2026-04-27)

### Status: partial

**Full impl.** All read paths working. No write paths (by design). All 14 new backend ops return `NotSupported` (correct for anonymous read-only).

### Source Files
- `clients/hackernews/src/lib.rs` (550 LOC) — `ClientBackend` impl + session/auth
- `clients/hackernews/src/api.rs` — HTTP layer (Firebase v0 endpoints)
- `clients/hackernews/src/mapping.rs` — HN item ↔ Poly message/user/row mapping
- `clients/hackernews/src/types.rs` — `HnItem`, `HnUser`, `HnFeed` types
- `clients/hackernews/src/cache.rs` — simple in-memory item cache
- `clients/hackernews/src/signup.rs` — plugin onboarding UI

### Notes
- Comment thread BFS: lines 618–680, correctly preserves parent IDs for tree reconstruction
- Settings storage stub (Pack C P18): lines 54–56, 378–404, TODO: host KV wiring
- Feed selector `HnFeed::from_channel_id()`: correctly maps channel IDs to feed types
- Dead/deleted filtering: lines 238, 518 (client-side in `get_messages()` and `get_view_rows()`)
- Tree spec max_depth: hard-coded to 8 (line 565); not configurable per account
