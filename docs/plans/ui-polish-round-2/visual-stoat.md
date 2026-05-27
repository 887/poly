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

---

## Phase-5 Code Audit (2026-04-27)

### Status: pass

### Account Login
Revolt-fork REST: `POST /auth/session/login` with email+password. Token in `X-Session-Token` header. `is_authenticated()` checks session presence.

### Overview Page
`get_account_overview_view()` returns `ViewKind::CardGrid`. `get_channel_view` returns `NotSupported` (chat-only, correct).

### Messaging
`send_message`, `send_reply_message`, `delete_message` all implemented. Attachments uploaded to Autumn CDN. `send_typing` not overridden (trait default NotSupported). `search_messages` not overridden.

### 14 New Backend Ops (commit 2041f112)
`block_user` via `PUT /users/{id}/block`. `add_friend`/`remove_friend` via Revolt friend API. `close_dm_channel`, `leave_group_dm`, `edit_group_dm`, `add_users_to_group_dm` via Revolt channel API. `ignore_user`/`unignore_user`, `set_friend_nickname`, `set_user_note`, `mute_conversation`/`unmute_conversation`, `invite_user_to_server` all use trait defaults (NotSupported).

### Moderation Ops
`kick_member`, `ban_member`, `unban_member`, `timeout_member`, `untimeout_member` all implemented. `get_moderation_log` — comment in code: "Stoat has no audit log endpoint → default NotSupported".

### Known Gaps
1. `search_messages` not implemented.
2. `get_moderation_log` not available in Revolt API.
3. `mute_conversation`, `invite_user_to_server`, social-nickname ops all NotSupported.
4. Server icons appear as letter-initial circles (Revolt CDN URLs not loading).
