# Lemmy Backend — Visual Audit Report

**Accounts:** Beaver (lemmy-session-2), Hedgehog (lemmy-session-3)
**Date:** 2026-04-21
**Screenshots:** `screenshots/lemmy/`

---

## Lemmy Backend Overview

Lemmy is a federated link aggregator (Reddit-like platform). The Poly Lemmy plugin has a very limited UI surface:
- **Second nav** shows only: 🔔 (Notifications bell) and + (Add community) button
- **No community/server icons** in the second nav — subscribed communities are not listed
- **No DMs section** — Lemmy private messaging not implemented
- **No Friends section** — Lemmy has no friends concept
- **Notifications** work via the bell icon

---

## Beaver (Lemmy)

### Landing / Notifications (beaver-01-landing.png)
- Clicking the Beaver avatar opens the Lemmy Notifications view immediately
- Second nav shows: 🔔 bell icon (selected) and + button
- Left panel shows "Notifications" with tabs: "All notifications (0)", Mentions (0), Other (0)
- Right panel: "No new notifications" empty state
- Account bar shows "Beaver / Online" with ⚙ settings gear

### Server List (beaver-02-server.png)
- Clicking the "+" button shows: "lemmy doesn't support creating servers from Poly. Redirecting you back…"
- This message appears in the main content area
- The message persists and redirects back automatically
- **Design issue:** The "+" button icon implies adding a server, but the tooltip/message reveals it's about communities. Should be hidden or labeled differently for Lemmy.

### Channel / Community (beaver-03-channel.png)
- No community icons in the second nav to click
- The Lemmy account has no subscribed communities shown in the sidebar
- Screenshots show the notifications view (only accessible content from sidebar)

### DMs (beaver-04-dms.png)
- Same as notifications view (no DMs route accessible via sidebar)
- Direct URL navigation to `/lemmy/.../dms` redirects to Settings

### Friends (beaver-05-friends.png)
- Same as notifications view (no Friends route accessible via sidebar)

### Notifications (beaver-06-notifications.png)
- Same as landing: "No new notifications"

### Settings (beaver-07-settings.png)
- Per-account settings accessible via ⚙ gear button in account bar
- Shows "Account Settings" with:
  - **Notifications section (LEMMY-SESSION-2)**: Notify me about (People I know start streaming, Friends join voice channels, Someone reacts to my messages), Sounds (New Message, Direct Messages, Incoming Ring), Badges (Enable Unread Message Badge)
  - **Content & Social section**: Sensitive Media (DMs from friends, DMs from others, Server channels — all set to "Hide"), DM Spam Filter radio buttons, Social Permissions
- These settings appear to be generic/template settings, not Lemmy-specific ones (e.g., "Friends join voice channels" doesn't apply to Lemmy)

---

## Hedgehog (Lemmy)

### All views (hedgehog-01 through hedgehog-07)
- Identical pattern to Beaver
- Same empty notifications, same "+" redirect message, same per-account settings

---

## Lemmy Backend Issues

1. **[HIGH] No community list in sidebar** — subscribed communities are not shown as icons in the second nav. Lemmy users can't navigate to their subscribed communities from the sidebar. The second nav is nearly empty (just bell + +).
2. **[HIGH] No way to browse communities** — there's no communities browser or community search reachable from the Poly UI for Lemmy accounts
3. **[MEDIUM] "+" button triggers misleading "doesn't support creating servers"** — the + button text and behavior should be "Browse Communities" or simply hidden for Lemmy. The message "Redirecting you back..." is poor UX.
4. **[MEDIUM] Per-account settings show generic/wrong settings** — the Notifications settings list Discord-style options ("Friends join voice channels", "Incoming Ring") that are meaningless for a Lemmy account
5. **[MEDIUM] DMs/Friends routes inaccessible** — no navigation available from sidebar; direct URL navigation fails (redirects to Settings)
6. **[LOW] "No new notifications" empty state** — functional but minimal; no invite to browse or configure

---

## Console Errors
No browser console errors captured during navigation.

---

## Phase-5 Code Audit (2026-04-27)

### Status: partial — community list missing, DMs/Friends inaccessible

### Account Login
`POST /api/v3/user/login` with username/password. Returns JWT token. `is_authenticated()` checks session.

### Overview Page
`get_account_overview_view()` returns `ViewKind::CardGrid`. `get_channel_view` returns `ViewKind::Tree` with Hot/New/Top sort options (community post feed). `get_view_rows("")` routes to federation listing for overview.

### Channel Sidebar
`get_channels(server_id)` returns subscribed communities as channels. However, communities do NOT appear as server icons in second nav — sidebar render gap. `get_dm_channels()` returns `NotSupported`.

### Messaging
`send_message` returns `NotSupported` (posts/comments are separate via view layer). `delete_message` implemented for post/comment removal.

### 14 New Backend Ops
All 14 use trait defaults (NotSupported). Lemmy has no social graph, DMs, or group concepts.

### Moderation Ops
`kick_member`, `ban_member`, `unban_member`, `timeout_member` implemented for community moderation. `get_moderation_log` not overridden (Lemmy has `/api/v3/modlog`).

### Known Gaps
1. **[HIGH] Community list absent from sidebar** — subscribed communities don't appear in second nav. Main navigation is broken.
2. **[HIGH] No DMs/Friends navigation** — `get_dm_channels` returns NotSupported; no DMs icon in sidebar.
3. "+" button shows misleading "doesn't support creating servers" message.
4. Per-account settings show Discord-style options irrelevant to Lemmy.
5. `get_moderation_log` not implemented.
6. `search_messages` not implemented.
