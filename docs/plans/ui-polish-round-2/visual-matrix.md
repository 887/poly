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

---

## Phase-5 Code Audit (2026-04-27)

### Status: partial

### Account Login
`POST /_matrix/client/v3/login` with username/password or access token. Maps Matrix session to poly `Session`. Device-key bootstrapping handled in client.

### Overview Page
`get_account_overview_view()` returns `ViewKind::CardGrid` with Space list. `get_view_rows("")` fetches joined rooms mapped to card rows. `get_channel_view` returns `NotSupported` (chat-only, correct).

### Messaging
`send_message`, `send_reply_message`, `send_typing`, `delete_message` all implemented via CS API. Avatar/display name hydrated since commits `7f4dc5df` and `44636eda`. `search_messages` not overridden (NotSupported).

### 14 New Backend Ops (commit 5b142e67)
`block_user`/`ignore_user`/`unignore_user` via `m.ignored_user_list`. `close_dm_channel`/`leave_group_dm` via leave+forget+`m.direct`. `mute_conversation`/`unmute_conversation` via push rules. `edit_group_dm` via `m.room.name` + `m.room.avatar`. `add_users_to_group_dm`/`invite_user_to_server` via room invite. `add_friend`/`remove_friend`/`set_friend_nickname`/`set_user_note` return NotSupported (no Matrix friend concept).

### Moderation Ops
All implemented: `kick_member`, `ban_member`, `unban_member`, `timeout_member` (power-level reduction), `untimeout_member`, `delete_message`. `get_moderation_log` via room state events.

### Known Gaps
1. `search_messages` not implemented (Matrix `/search` endpoint available).
2. Friends concept mismatch — People panel shows "No friends found" with no explanation.
3. Space icons appear as letter-initial circles (image proxy/CORS).
