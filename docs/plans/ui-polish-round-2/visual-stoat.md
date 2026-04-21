# Stoat Backend — Visual Audit Report

**Accounts:** Raccoon (RACCOON01), Stoat (STOAT01)
**Date:** 2026-04-21
**Screenshots:** `screenshots/stoat/`

---

## Raccoon (Stoat)

### Landing (raccoon-01-landing.png)
- Stoat backend presents like a standard chat server
- Second nav shows server icons (Stoat-specific servers)
- Account bar shows Raccoon with Online status

### Server / Channel List (raccoon-02-server.png)
- Channel list shows text channels and voice channels grouped by category
- Stoat server structure similar to Discord

### Chat / Messages (raccoon-03-channel.png)
- Message view renders with avatar, username, timestamp
- Message content displays correctly
- Inline code formatting works

### DMs (raccoon-04-dms.png)
- DM list with contacts and unread counts
- "New Conversation" and "Saved Messages" at top
- Right panel: "Select a conversation" placeholder

### Friends (raccoon-05-friends.png)
- People panel with Friends/Ignored/Blocked Users tabs
- Empty state with search box

### Notifications (raccoon-06-notifications.png)
- Notifications panel with categorized tabs
- Empty state: "No new notifications"

### Settings (raccoon-07-settings.png)
- Opens Account Settings per-account panel (unlike Discord which opens global settings)
- Shows Notifications and Content & Social settings

---

## Stoat (Stoat)

### All views (stoat-01 through stoat-07)
- Similar to Raccoon
- Both Stoat accounts show the same server structure

---

## Stoat Backend Issues

1. **Direct URL navigation to Stoat routes redirects to Settings** — same issue as Discord/Lemmy/Forgejo/GitHub; only sidebar avatar clicks work for navigation
2. **Server icons in second nav** appear as letter-initial colored circles — Stoat server icons (if any) not loading

---

## Console Errors
No critical console errors observed during Stoat backend navigation.
