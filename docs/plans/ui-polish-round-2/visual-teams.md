# Teams Backend — Visual Audit Report

**Accounts:** Sheep (U001), Walrus (U002)
**Date:** 2026-04-21
**Screenshots:** `screenshots/teams/`

---

## CRITICAL BUG: WASM Hard Freeze on Teams Account Click

**Every click on a Teams account avatar (Sheep or Walrus) in the first nav sidebar triggers a WASM tight loop that hangs the Chrome page completely.** CDP becomes unresponsive, screenshots time out, and the only recovery is `hard_kill` + `launch_app`.

This bug was reproduced consistently:
- First attempt during prior session: Sheep avatar click crashed WASM
- Second attempt after recovery: clicking server icon (C) crashed WASM
- Third attempt: Sheep avatar click crashed WASM
- Fourth attempt (fresh boot): Sheep avatar click crashed WASM
- Fifth attempt (fresh boot): Sheep avatar click crashed WASM

**One exception:** In one lucky session (second session attempt, after a prior successful landing), clicking the Sheep avatar worked and loaded the Teams UI for ~2 minutes before a server icon click caused the next crash. During this window, 4 screenshots were captured.

### Root cause analysis (hypothesis)
Per CLAUDE.md WASM hang diagnosis: most likely cause is a `Signal::write()` chain inside the Teams plugin's initialization handler, or a `use_effect` subscriber that writes to the same signal it reads (infinite re-render loop). The Teams plugin may be calling multiple Signal writes in sequence during account activation.

---

## Sheep (Teams) — Partial Screenshots

### Landing (sheep-01-landing.png)
- Boot sequence overlay visible — shows "Boot sequence complete" with all accounts connected including Sheep/teams
- After boot overlay dismisses, the app shows the last active account (typically Demo Cat or whatever was last used)

### DMs — CAPTURED (sheep-04-dms.png)
- Teams "Direct Messages" panel
- Shows: "New Conversation" button, "Saved Messages", and "Unknown" contact
- Colored letter-circle avatars in second nav (C, P) representing Teams channels/servers
- Right panel: "Select a conversation" placeholder with chat bubble icon
- Account bar shows "Sheep / Online" with mic, headset, settings icons

### Friends / People — CAPTURED (sheep-05-friends.png)
- "People" panel with "Manage friends, ignored users, and blocked users for this account"
- Three tabs: Friends, Ignored, Blocked Users
- Empty state: "No friends found" with Search box
- Second nav shows: 💬 (DMs), 👥 (People), 🔔 (Bell), C and P server icons, + button

### Settings (sheep-07-settings.png)
- Global Settings page showing all accounts (not per-account settings)
- Reached via the ⚙ icon in the global settings bar, not the account bar

### Missing Screenshots
- **sheep-02-server.png** — could not capture; WASM crashed on server icon click
- **sheep-03-channel.png** — could not capture; WASM crashed before reaching channel view
- **sheep-06-notifications.png** — could not capture; required active account which crashes

---

## Walrus (Teams)

All screenshots for Walrus could not be captured. Every attempt to click the Walrus avatar (U002) at y=596 in the first nav caused the same WASM tight loop crash as Sheep.

**Walrus is effectively inaccessible** from the UI.

---

## Teams Backend Issues

1. **[CRITICAL] WASM hard freeze on Teams account avatar click** — reproduces 100% of the time, requires hard_kill + full rebuild to recover. Teams backend is effectively unusable.
2. **[CRITICAL] Server icon click also causes WASM freeze** — the colored C and P icons in the second nav (Teams channels/servers) consistently crash the page when clicked
3. **"Unknown" contact in DMs list** — one contact shows as "Unknown" with no display name or avatar; likely a contact resolution failure
4. **Direct URL navigation to Teams routes triggers boot reload** — `/teams/localhost:9103/U001/dms` causes a full page reload and then redirects to global Settings; Teams routing is broken for direct URL access
5. **Teams channels shown as letter-circles** (C, P) — no server/channel names visible; Teams server names not rendered in the icons
6. **Per-account settings inaccessible** — account bar ⚙ button goes to global settings, not Teams-specific per-account settings

---

## Console Errors
Not capturable due to WASM freeze killing CDP connection before console messages could be retrieved.
