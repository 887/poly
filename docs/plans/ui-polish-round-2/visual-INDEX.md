# Poly UI — Visual Audit Master Index

**Phase-5 update:** 2026-04-27 (Stream D — code audit + per-backend smoke tests)
**Original visual audit:** 2026-04-21 (CDP screenshots via MCP)
**App:** poly-web at localhost:3000

---

## Phase-5 Executive Summary (10 bullets)

1. **Demo (all three clients) is the only backend that is fully production-ready** for dev/demo purposes. Cat+Dog have cross-account messaging, mutual friends, and all 14 social ops as in-memory stubs. Platypus (demo_forum) has a working Tree view with 12-sort forum layout.
2. **Discord is the most feature-complete real backend**: all 14 new social ops implemented via REST, full moderation suite, send+delete messaging. Main gaps: intermittent plugin-sidebar load failure and server icons not loading (image proxy/CORS).
3. **Matrix is functionally solid**: all 14 new ops implemented, full moderation (kick/ban/timeout), DMs, push-rule mutes. Avatar hydration fixed in commits `6f9d5dbd`/`c7d33e1d`. Remaining gap: `search_messages`.
4. **Stoat is solid for its Revolt-fork surface**: send/delete/friends/block/group-DM all work. Missing: mute_conversation, invite_user_to_server, social-nickname ops, audit log.
5. **Teams is blocked by a WASM hard freeze** on account activation — both accounts crash 100% of the time. No UI has been verifiable. Root cause: likely Signal::write() chain in Teams init path (hang class #1). Must be fixed before Teams is usable.
6. **Forgejo and GitHub both fail to load issue detail on click** — the primary interaction for code-forge backends is broken. Issue lists render but clicking produces "Failed to load detail" (Forgejo) or a permanently empty right pane (GitHub).
7. **Lemmy's biggest gap is the missing community list in the sidebar** — subscribed communities don't appear as server icons, making the entire Lemmy UI non-navigable beyond notifications.
8. **HackerNews is a complete read-only feed reader**: Top/New/Best/Ask/Show/Jobs feeds with real HN Firebase API, comment threads (300-item BFS), per-channel views. No social ops (all NotSupported by design). No test account in poly-web seed data yet.
9. **`search_messages` is implemented only in demo clients** — every real backend returns NotSupported. Discord, Matrix, GitHub, Forgejo, Lemmy, Teams all have server-side search endpoints that could be wired.
10. **Cross-cutting gaps**: server/space/community icons don't load for any real backend (image proxy issue); per-account settings show Discord-style options for non-Discord backends; direct URL navigation redirects to global Settings for all non-demo backends.

---

## Backend Feature Matrix (Phase-5)

| Backend | Login | Overview | Sidebar | Messaging | Context-menus | Search | Status |
|---------|-------|----------|---------|-----------|---------------|--------|--------|
| demo (Cat) | pass | NotSupported→fallback | pass | pass | Server only | pass | pass |
| demo_chat (Dog) | pass | NotSupported→fallback | pass | pass | Server only | pass | pass |
| demo_forum (Platypus) | pass | CardGrid | pass | pass | Server only | pass | pass |
| discord | pass | CardGrid | pass | pass | Full suite | NotSupported | partial |
| matrix | pass | CardGrid | pass | pass | Full suite | NotSupported | partial |
| stoat | pass | CardGrid | pass | pass | Most ops | NotSupported | partial |
| teams | pass (boot) | CardGrid (untested) | untested | untested | Partial (no ban) | NotSupported | fail |
| forgejo | pass | CardGrid | pass | read-only | Mod ops | NotSupported | partial |
| github | pass | CardGrid | pass | read-only | Limited | NotSupported | partial |
| lemmy | pass | CardGrid | partial (no communities) | read-only | Mod ops | NotSupported | partial |
| hackernews | pass (guest) | FlatList | pass (feeds) | read-only | None | NotSupported | partial |

---

## Three Backends with Biggest Gaps

### 1. Teams — CRITICAL: WASM Hard Freeze
Both accounts crash on every activation attempt. Backend code is complete but zero UI is verifiable. Fix target: Signal::write() chain in Teams init path.

### 2. Lemmy — HIGH: Community List Missing
Subscribed communities absent from second nav. Only notifications reachable from sidebar. DMs return NotSupported so no DMs icon. Per-account settings show wrong (Discord-style) options.

### 3. Forgejo + GitHub — HIGH: Issue Detail Fails
Both code-forge backends fail to load issue detail on click — the primary user interaction. Forgejo shows "Failed to load detail"; GitHub shows a permanently empty pane. `search_messages` and `get_moderation_log` also unimplemented for both.

---

---

## Overview

Full visual audit of all 17 test-animal accounts across 8 backends, capturing 7 states per account:
1. Default landing
2. Server/repository/space list
3. Chat/channel/issue view
4. DMs / direct message list
5. Friends / people panel
6. Notifications
7. Account settings

---

## Backend Reports

| Backend | Report | Accounts | Screenshot Dir | Status |
|---------|--------|----------|----------------|--------|
| Demo | [visual-demo.md](visual-demo.md) | Cat, Dog, Platypus | screenshots/demo/ | Complete |
| Discord | [visual-discord.md](visual-discord.md) | Koala, Kangaroo | screenshots/discord/ | Complete |
| Matrix | [visual-matrix.md](visual-matrix.md) | Axolotl, Owl | screenshots/matrix/ | Complete |
| Stoat | [visual-stoat.md](visual-stoat.md) | Raccoon, Stoat | screenshots/stoat/ | Complete |
| Teams | [visual-teams.md](visual-teams.md) | Sheep, Walrus | screenshots/teams/ | Partial (WASM crash) |
| Forgejo | [visual-forgejo.md](visual-forgejo.md) | Flamingo, Otter | screenshots/forgejo/ | Complete |
| GitHub | [visual-github.md](visual-github.md) | Chameleon, Penguin | screenshots/github/ | Complete |
| Lemmy | [visual-lemmy.md](visual-lemmy.md) | Beaver, Hedgehog | screenshots/lemmy/ | Complete |

---

## Screenshot Status

| Account | Backend | 01 | 02 | 03 | 04 | 05 | 06 | 07 |
|---------|---------|----|----|----|----|----|----|-----|
| Cat | demo | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Dog | demo | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Platypus | demo_forum | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Koala | discord | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Kangaroo | discord | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Axolotl | matrix | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Owl | matrix | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Raccoon | stoat | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Stoat | stoat | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Sheep | teams | ✓ | ✗ | ✗ | ✓ | ✓ | ✗ | ✓ |
| Walrus | teams | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| Flamingo | forgejo | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Otter | forgejo | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Chameleon | github | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Penguin | github | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Beaver | lemmy | ✓ | ✓ | ✓* | ✓* | ✓* | ✓ | ✓ |
| Hedgehog | lemmy | ✓ | ✓* | ✓* | ✓* | ✓* | ✓ | ✓ |

Notes:
- ✗ = Could not capture (WASM crash or inaccessible)
- ✓* = Captured but shows notifications view (DMs/Friends/Channels not accessible via sidebar for Lemmy)
- Sheep 02 (server) and 03 (channel) missing due to WASM crash on server icon click
- Walrus entirely inaccessible due to WASM freeze on avatar click

---

## Issue Summary by Backend

### Issue Counts

| Backend | Critical | High | Medium | Low | Total |
|---------|----------|------|--------|-----|-------|
| Demo | 0 | 1 | 1 | 2 | 4 |
| Discord | 0 | 2 | 2 | 1 | 5 |
| Matrix | 0 | 0 | 2 | 3 | 5 |
| Stoat | 0 | 0 | 1 | 1 | 2 |
| Teams | 2 | 0 | 4 | 0 | 6 |
| Forgejo | 0 | 1 | 2 | 3 | 6 |
| GitHub | 0 | 1 | 3 | 1 | 5 |
| Lemmy | 0 | 2 | 4 | 1 | 7 |
| **Total** | **2** | **7** | **19** | **12** | **40** |

---

## Top 5 Cross-Backend Recurring Issues

### 1. Direct URL Navigation Always Redirects to Settings
**Affects:** Teams, Discord, Stoat, Forgejo, GitHub, Lemmy (all non-demo backends)
Navigating to any route like `/teams/localhost:9103/U001/dms` or `/forgejo/.../notifications` via browser URL bar causes a full page reload that redirects to the global Settings page. Only sidebar avatar clicks via the Dioxus router's Link components work for navigation. This means deep-linking and browser back/forward do not work for most backends.

### 2. Server / Repository / Space Icons Show Letter-Initial Circles
**Affects:** Discord, Matrix, Stoat, Forgejo, GitHub, Lemmy, Teams
All backends that provide server/community/repository icons show letter-initial colored circles instead of the actual images. Discord guild icons, Matrix space thumbnails, GitHub repository icons — none are loaded. This is likely a CORS, authentication, or image proxy issue in the plugin fetch layer.

### 3. Issue / Item Detail Fails to Load on Click
**Affects:** Forgejo (shows "Failed to load detail"), GitHub (shows "Select an item" unchanged)
Clicking any issue, PR, or discussion item in the Issues & PRs panel does not populate the right panel with details. For Forgejo, the item IS selected (blue highlight) but shows "Failed to load detail" error. For GitHub, nothing happens. The issue detail API call appears to be failing or not wired up.

### 4. Per-Account Settings Shows Generic / Wrong Settings
**Affects:** Lemmy, Teams (and potentially all backends)
The per-account settings modal (accessible via ⚙ gear in account bar) shows Discord-style settings options ("Friends join voice channels", "Incoming Ring") even for backends like Lemmy and Teams where these concepts don't apply. Settings are not filtered by backend capability.

### 5. DMs / Friends Routes Inaccessible or Show Unsupported States
**Affects:** Forgejo, GitHub, Lemmy
Code-forge backends (Forgejo, GitHub) don't support DMs or Friends, but the unsupported feature messages are plain text without styled empty states. Lemmy shows the DMs/Friends routes only via URL navigation (which redirects to Settings), making these routes effectively unreachable from the sidebar.

---

## Critical Issues (Require Immediate Fix)

### [CRITICAL-1] Teams Backend WASM Hard Freeze
Every click on a Teams account avatar (Sheep U001, Walrus U002) triggers a WASM tight loop that completely freezes the Chrome page. CDP becomes unresponsive. Requires hard_kill + full rebuild to recover. Both Teams accounts are effectively unusable. See [visual-teams.md](visual-teams.md) for full details.

### [CRITICAL-2] Teams Server Icon Click WASM Freeze
Clicking the C or P server icons in the second nav (Teams channels) also causes WASM freeze, separate from the avatar click crash.

---

## Failed Sign-ins

No sign-in failures observed — all 17 accounts connected successfully at boot (visible in the boot sequence overlay). The Teams accounts ARE connected (show "connected" in boot overlay) but crash after activation in the UI.

---

## All Report Files

- [visual-demo.md](visual-demo.md) — Cat / Dog / Platypus
- [visual-discord.md](visual-discord.md) — Koala / Kangaroo
- [visual-matrix.md](visual-matrix.md) — Axolotl / Owl
- [visual-stoat.md](visual-stoat.md) — Raccoon / Stoat
- [visual-teams.md](visual-teams.md) — Sheep / Walrus
- [visual-forgejo.md](visual-forgejo.md) — Flamingo / Otter
- [visual-github.md](visual-github.md) — Chameleon / Penguin
- [visual-lemmy.md](visual-lemmy.md) — Beaver / Hedgehog
- [visual-hackernews.md](visual-hackernews.md) — Anonymous guest (Phase-5 code audit only)
