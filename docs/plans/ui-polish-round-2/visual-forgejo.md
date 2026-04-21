# Forgejo Backend — Visual Audit Report

**Accounts:** Flamingo, Otter
**Date:** 2026-04-21
**Screenshots:** `screenshots/forgejo/`

---

## Forgejo Backend Overview

Forgejo is a code forge (Git hosting platform), not a chat platform. The Poly Forgejo plugin maps:
- **Servers** → Repositories (otter/fish-finder, otter/dam-builder, flamingo/*)
- **Channels** → Issues, Pull Requests, Discussions sections of a repository
- **DMs** → Not supported (shows "forgejo doesn't support direct messages")
- **Friends** → Not supported (shows empty state)
- **Notifications** → Forgejo mentions/notifications

---

## Flamingo (Forgejo)

### Landing / Server List (flamingo-01-landing.png, flamingo-02-server.png)
- Landing shows the Repositories list with flamingo's repos
- Second nav shows repository icons as letter-circles
- Left panel shows "REPOSITORIES" with repo names and subsections (Issues, Pull Requests, Discussions)

### Channel / Issues View (flamingo-03-channel.png)
- Issues & PRs panel with tab switcher (Issues, Pull Requests, Discussions)
- Filter bar with Open/Closed toggle and text filter
- Issue list shows items with title, issue number, and author
- Right panel: "Select an item" placeholder — **issues do not load detail on click** (bug)
- Refresh button (↻) at top right

### DMs (flamingo-04-dms.png)
- Shows "forgejo doesn't support direct messages" as plain text in main content area
- No styled empty state UI — just raw text

### Friends (flamingo-05-friends.png)
- Empty state — Forgejo has no friends concept
- Shows whatever the generic empty state is for this route

### Notifications (flamingo-06-notifications.png)
- Notifications panel with categorized tabs: All notifications (0), Mentions (0), Other (0)
- "No new notifications" right panel

### Settings (flamingo-07-settings.png)
- Global Settings page showing all accounts list

---

## Otter (Forgejo)

### Channel / Issues (otter-03-channel.png)
- Issue list for otter/dam-builder shows: "Support curved dam designs #1 by otter", "Water pressure calculations are off #2 by flamingo"
- **BUG:** Clicking "Support curved dam designs" issue title shows the item selected (blue highlight) but right panel shows **"Failed to load detail"** — issue detail fails to load
- This is different from "Select an item" (placeholder) — the issue IS selected but the backend returns an error

### DMs (otter-04-dms.png)
- Same "forgejo doesn't support direct messages" behavior as Flamingo

---

## Forgejo Backend Issues

1. **[HIGH] Issue detail fails to load** — clicking any issue row shows "Failed to load detail" in the right panel. The issue IS selected (blue highlight) but no detail renders. This may be an API response parsing error in the Forgejo plugin.
2. **[MEDIUM] "forgejo doesn't support direct messages"** — the DMs route for Forgejo shows raw plain text in the main content area without a styled unsupported-feature empty state
3. **[MEDIUM] Direct URL navigation to Forgejo routes redirects to Settings** — navigating to `/forgejo/localhost:9106/fj-localhost:9106-flamingo/notifications` causes a router redirect to global Settings. Only sidebar avatar clicks work.
4. **[LOW] Server icons show letter-circles** — repository icons not loading; only first-letter initials shown
5. **[LOW] Friends/DMs routes show minimal empty states** — the unsupported feature messages are plain text, not styled components
6. **[LOW] Issue filter bar UI** — the filter text box and Open/Closed toggle buttons look functional but the search functionality was not tested

---

## Console Errors
No browser console errors captured during navigation.
