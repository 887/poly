# HackerNews Backend — Phase-5 Smoke Test Report

**Status:** partial — code-complete read-only feed; no live UI session available (no test account in poly-web seed data)
**Accounts:** Anonymous guest (no login required; any credentials accepted)
**Date:** 2026-04-27
**Source audit commits:** 5b142e67 (14-op trait expansion), current HEAD

---

## Account Login

`authenticate()` always returns `Ok(guest_session)` — no credentials needed. `logout()` clears `self.session`. `is_authenticated()` returns `true` when a session exists. No sign-in failure modes for normal operation.

---

## Overview Page (`get_account_overview_view`)

Returns `ViewKind::FlatList` with:
- Header: `plugin-hackernews-overview-title` / `plugin-hackernews-overview-subtitle`
- `row_template`: primary=`title`, secondary=`author-domain`, meta=`points-comments-age`
- `page_size`: 30

`get_view_rows("")` routes the empty `channel_id` sentinel to `HnFeed::Top` and fetches the HN Firebase v0 API with offset-based paging (30 per page). Dead/deleted items are filtered client-side. The overview is backed by real live network calls and fully functional.

`get_channel_view` returns a `FlatList` descriptor for any feed channel, and a row page with the same template. `get_view_detail` fetches a single HN post item and returns a sanitized HTML body with a comments TreeSpec (max_depth unset — recursive BFS).

---

## Channel Sidebar (`get_channels` / `get_dm_channels`)

`get_channels("hn")` returns fixed feed channels:
- `hn-top` (Top Stories)
- `hn-new` (New Stories)
- `hn-best` (Best Stories)
- `hn-ask` (Ask HN)
- `hn-show` (Show HN)
- `hn-jobs` (Jobs)

There is a single `Server` (`id: "hn"`, name: "Hacker News"). Comment thread sub-channels (`hn-post-{id}`) are ephemeral and not in the static channel list.

`get_dm_channels()` returns `Vec::new()` (read-only feed — no DM concept).

---

## Messaging (`send_message`)

Returns `NotSupported("Hacker News is read-only; posting requires authentication via news.ycombinator.com")`.

Read is fully supported: `get_messages` dispatches to either the feed fetcher or a BFS comment-thread fetcher. Comment threads default to 300 items limit.

`send_typing` not overridden — trait default `NotSupported`.

---

## Context-Menu Ops

`get_context_menu_items` returns `Ok(Vec::new())` for all targets. `invoke_context_action` returns `NotFound`. No moderation ops — `kick_member`, `ban_member`, `timeout_member`, `delete_message` all use trait defaults (`NotSupported`). Correct for a read-only anonymous feed.

---

## 14 New Backend Ops (commit 5b142e67)

All 14 ops use trait defaults — all return `NotSupported`. HN has no social graph, DMs, or group concepts.

---

## Settings

`get_settings_sections()` returns a `preferences` section with `default-feed` (Select, options: top/new/best/ask/show/jobs) and `items-per-page` (Slider, 10–100, step 5). Settings fields are declared but not yet persisted to the host KV store.

---

## Known Gaps / TODOs

1. **[HIGH] No live UI test account** — HackerNews was not added as a test account in the 2026-04-21 poly-web session. All verification is code-only.
2. **[HIGH] No search** — `search_messages` not implemented. HN Firebase v0 has no server-side search; would require Algolia HN Search API (`hn.algolia.com/api/v1/search`).
3. **[MEDIUM] No real-time updates** — `event_stream()` returns empty stream. New stories only appear on next `get_view_rows` call.
4. **[MEDIUM] Comment thread navigation** — `hn-post-{id}` channels are not in the static channel list; host needs to open them via `get_view_detail` or a nav event from clicking a story row. Needs UI smoke-test.
5. **[LOW] Settings not persisted** — `default-feed` and `items-per-page` settings fields are declared but host KV persistence is not wired.
6. **[LOW] No post-submission CTA** — read-only by design, but a link to submit at news.ycombinator.com would aid engaged users.
