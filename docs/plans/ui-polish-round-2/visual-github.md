# GitHub Backend — Visual Audit Report

**Accounts:** Chameleon, Penguin
**Date:** 2026-04-21
**Screenshots:** `screenshots/github/`

---

## GitHub Backend Overview

GitHub is a code forge platform. The Poly GitHub plugin maps:
- **Servers** → Repositories (chameleon/color-shift, penguin/iceberg-os, penguin/fish-tracker)
- **Channels** → Issues, Pull Requests, Discussions sections
- **DMs** → Shows "github doesn't support direct messages" (not captured explicitly, inferred from forgejo pattern)
- **Friends** → Shows empty state or "not supported"
- **Notifications** → GitHub notifications

The GitHub backend renders a distinct **Repository grid/list view** on landing, different from Forgejo which shows a channel list immediately.

---

## Chameleon (GitHub)

### Landing / Repositories (chameleon-01-landing.png, chameleon-02-server.png)
- Landing page shows "Repositories" with a search box "Search repos..."
- "ALL REPOS (1)" section showing `chameleon/color-shift` as a card with GitHub icon
- This repository grid view is different from Forgejo (which shows an inline list in the channel panel)
- The card shows just the repo name without description

### Channel / Issues (chameleon-03-channel.png)
- After clicking the repo, shows the channel view with Issues & PRs panel
- "Pull Requests" tab selected showing "Support UV spectrum colors #1 by penguin"
- Right panel: "Select an item" placeholder
- Same issue-detail issue as Forgejo — clicking items doesn't load detail (shows "Select an item" not "Failed to load detail")

### DMs (chameleon-04-dms.png)
- Likely shows "github doesn't support direct messages" or redirected to unsupported route

### Friends (chameleon-05-friends.png)
- Empty state

### Notifications (chameleon-06-notifications.png)
- Notifications panel, likely empty

### Settings (chameleon-07-settings.png)
- Global Settings page

---

## Penguin (GitHub)

### Landing / Repositories (penguin-01-landing.png, penguin-02-server.png)
- Penguin has two repos: penguin/iceberg-os and penguin/fish-tracker
- Both shown as cards with GitHub icon and letter U (likely a fallback avatar)
- Repository grid shows more entries than Chameleon

### Channel / Issues (penguin-03-channel.png)
- For penguin's repos, the Issues panel shows **"No items"** — penguin's repositories have no open issues/PRs
- Right panel: "Select an item" placeholder
- The "No items" empty state is plain text without an icon — could be improved

---

## GitHub Backend Issues

1. **[HIGH] Issue/PR detail fails to load on click** — same as Forgejo; clicking any issue/PR row does not populate the right panel with details. Right panel remains at "Select an item" placeholder.
2. **[MEDIUM] Repository card design** — repo cards show GitHub icon + repo name only; no description, star count, language, or last update info. Very minimal.
3. **[MEDIUM] "No items" empty state** — plain text without icon or helpful messaging when a repository has no issues/PRs
4. **[MEDIUM] Direct URL navigation redirects to Settings** — same router issue as all non-demo backends
5. **[LOW] Repository grid search box** — "Search repos..." input is present but filtering functionality not tested

---

## Comparison: GitHub vs Forgejo

| Feature | GitHub | Forgejo |
|---------|--------|---------|
| Landing view | Repository grid (cards) | Channel list immediately |
| Repository icons | GitHub logo (round) | Letter-initial circle |
| Issue list | Works (shows items) | Works (shows items) |
| Issue detail | Does not load | "Failed to load detail" |
| DMs | Not supported | Not supported |

GitHub shows a more visual landing page (repo cards) while Forgejo drops directly into the issue list. Both fail to load issue detail on click.

---

## Console Errors
No browser console errors captured during navigation.

---

## Phase-5 Code Audit (2026-04-27)

### Status: partial — issue detail does not load on click

### Account Login
OAuth2 PAT or app token. `authenticate()` calls `GET /user` with `Authorization: Bearer {token}`.

### Overview Page
`get_account_overview_view()` returns `ViewKind::CardGrid` (repository grid). `get_channel_view` returns `ViewKind::Split` for Issues/PRs/Discussions. `get_view_detail` — fetches issue detail; does not populate right panel in practice.

### Channel Sidebar
`get_channels(server_id)` — Issues (`gh-issues-*`), Pull Requests (`gh-pulls-*`), Discussions (`gh-discussions-*`) per repo. `get_dm_channels()` returns `NotSupported`.

### Messaging
`send_message` returns `NotSupported` — read-only. `delete_message` implemented for issue comments; `NotSupported` for PRs and discussions.

### 14 New Backend Ops
All 14 use trait defaults (NotSupported). Comment in source: "kick/ban/timeout/channel-mgmt/modlog are all NotSupported."

### Moderation Ops
`kick_member`, `ban_member`, `timeout_member` not overridden (GitHub has no server moderation). `delete_message` implemented for issue comments.

### Known Gaps
1. **[HIGH] Issue/PR detail does not load** — right panel stays at "Select an item" after clicking any issue row. Same root cause as Forgejo.
2. `search_messages` not implemented (GitHub `/search/issues` available).
3. Repository cards show name only — no description, stars, language, last-update info.
4. No moderation ops (correct for GitHub, but moderation UI items should be gated/hidden).
