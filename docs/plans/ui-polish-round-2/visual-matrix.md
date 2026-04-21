# Matrix Backend — Visual Audit Report

**Accounts:** Axolotl (AXOLDEVICE01), Owl (OWLDEVICE01)
**Date:** 2026-04-21
**Screenshots:** `screenshots/matrix/`

---

## Axolotl (Matrix)

### Landing (axolotl-01-landing.png)
- Matrix "Spaces" architecture: second nav shows Space icons instead of servers
- Second nav shows: DMs icon (💬), Friends/People icon (👥), Bell/Notifications icon (🔔), then Space icons as colored letter-circles
- Spaces shown in second nav

### Server / Space List (axolotl-02-server.png)
- Left panel shows "SPACES" with space names and room lists
- Rooms within spaces show with `#` prefix
- Space hierarchy visible

### Chat / Channel (axolotl-03-channel.png)
- Matrix-style message view with avatar, display name, timestamp
- Message content renders correctly
- Matrix room member list can be toggled

### DMs (axolotl-04-dms.png)
- DM conversations listed with contacts
- Matrix direct messaging works with MXID display names
- Right panel shows "Select a conversation" until a DM is selected

### Friends (axolotl-05-friends.png)
- People panel shows Friends/Ignored/Blocked Users tabs
- Matrix doesn't have a traditional "friends" concept; this panel is shared UI across backends

### Notifications (axolotl-06-notifications.png)
- Notifications with categorized tabs
- Matrix notifications appear in the "All notifications" tab

### Settings (axolotl-07-settings.png)
- Per-account settings accessible via ⚙ gear button
- Shows Matrix-specific settings (Notifications, Content & Social)

---

## Owl (Matrix)

### All views (owl-01 through owl-07)
- Similar patterns to Axolotl
- Both Matrix accounts use the same Space/room structure
- Owl shows rooms from spaces differently (different subscribed spaces)

---

## Matrix Backend Issues

1. **Matrix "Friends" concept mismatch** — the People/Friends panel is generic across backends; Matrix doesn't have friends lists natively. The panel shows "No friends found" for Matrix accounts without meaningful action.
2. **Space icons in second nav** appear as letter-initial colored circles, not Matrix space thumbnails — similar to Discord, images may not be loading
3. **Room list nesting** — Spaces contain rooms but the second nav structure (Space icon → room list) is clear and correct
4. **DM contacts** use MXID format (@username:server) in some places but display names in others — minor inconsistency
5. **Notifications panel** appears functional; Matrix provides rich notification data

---

## Console Errors
No critical console errors observed during Matrix backend navigation.
