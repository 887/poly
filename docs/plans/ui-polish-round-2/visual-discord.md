# Discord Backend — Visual Audit Report

**Accounts:** Koala (account 1), Kangaroo (account 2)
**Date:** 2026-04-21
**Screenshots:** `screenshots/discord/`

---

## Koala (Discord)

### Landing (koala-01-landing.png)
- Initial click on Koala showed "Plugin sidebar failed to load — showing channels" error in the channel list panel
- This error was transient — after navigating to `/discord/discord/1/dms` directly, the UI loaded correctly
- **Bug:** Plugin sidebar load fails intermittently on first account activation

### Server / Channel List (koala-02-server.png)
- Second nav shows Discord server icons (colored circles with letter initials)
- Server list panel correctly shows channels grouped by category (TEXT CHANNELS, VOICE CHANNELS)
- Channel names shown with `#` prefix

### Chat / Messages (koala-03-channel.png)
- Message view renders Discord-style messages with avatar, username (with role color), timestamp
- "You need the `VIEW_CHANNEL` permission" error shown for some channels — expected for accounts without permissions
- Pinned messages icon visible in channel header
- Members panel not visible by default

### DMs (koala-04-dms.png)
- DM list shows Discord friends/contacts
- Standard "New Conversation" and "Saved Messages" items at top
- Right panel: "Select a conversation" placeholder

### Friends (koala-05-friends.png)
- People panel with Friends/Ignored/Blocked Users tabs
- Empty state: "No friends found" — Koala has no friends in the test data

### Notifications (koala-06-notifications.png)
- Notifications panel with categorized tabs (All, Mentions, Other)
- Empty state: "No new notifications"

### Settings (koala-07-settings.png)
- Opens global Settings (not per-account) — shows Accounts list
- Account-specific settings accessible via ⚙ icon in account bar

---

## Kangaroo (Discord)

### All views (kangaroo-01 through 07)
- Same layout patterns as Koala
- kangaroo has similar empty states for Friends and Notifications
- Discord server icons and channel list rendered correctly
- Message view shows discord-style formatting

---

## Discord Backend Issues

1. **"Plugin sidebar failed to load — showing channels"** — intermittent error on account activation, likely a timing issue with plugin initialization
2. **"You need the VIEW_CHANNEL permission"** shown inline in message area — this is a Discord permission error surfaced as a message, not a styled empty state; could be improved
3. **Settings button in account bar opens global settings** — not account-specific; inconsistent with demo backend which opens per-account settings
4. **Direct URL navigation to Discord routes redirects to Settings** — only sidebar clicks work; the router does not handle full page loads for Discord-specific routes
5. **Second nav server icons** use letter-initial circles instead of server icons — Discord server icons (PNG) not loaded; may be a CORS or authentication issue

---

## Console Errors
- "Plugin sidebar failed to load" appears in the channel list panel on first load; no browser console errors captured

---

## Phase-5 Code Audit (2026-04-27)

### Status: partial

### Account Login
OAuth2 bearer token. `authenticate()` calls `GET /api/v10/users/@me`. `AuthFailed` on invalid token.

### Overview Page
`get_account_overview_view()` returns `ViewKind::CardGrid`. `get_view_rows("")` fetches guilds with member counts. `get_channel_view` returns `NotSupported` (chat-only, correct).

### Messaging
`send_message`, `send_reply_message`, `send_typing`, `delete_message` all implemented. `search_messages` not overridden (NotSupported).

### 14 New Backend Ops (commit 5b142e67)
Most complete: `block_user`, `add_friend`/`remove_friend`, `set_friend_nickname`, `set_user_note`, `close_dm_channel`, `unmute_conversation`, `leave_group_dm`, `edit_group_dm`, `add_users_to_group_dm`, `invite_user_to_server` all via REST. `ignore_user`/`unignore_user` and `mute_conversation` use trait defaults.

### Moderation Ops
All implemented: `kick_member`, `ban_member`, `unban_member`, `timeout_member`, `untimeout_member`, `get_bans`, `get_moderation_log`, `delete_message`.

### Known Gaps
1. Intermittent plugin sidebar load failure on first account activation.
2. Server icons not loading (CORS/auth on image URL fetch).
3. `search_messages` not implemented (Discord has `/messages/search` endpoint).
4. `mute_conversation` uses trait default NotSupported.
