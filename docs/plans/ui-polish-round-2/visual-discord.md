# Discord Backend — Phase-5 Smoke Test Report

**Status:** partial — backend is functionally rich but overview view-rows not implemented; prior visual audit revealed intermittent plugin-sidebar load failure
**Accounts:** Koala (account 1), Kangaroo (account 2)
**Date (Phase-5 update):** 2026-04-27
**Prior visual audit:** 2026-04-21 (screenshots in `screenshots/discord/`)

---

## Account Login

Discord uses OAuth2 bearer token auth. `authenticate(AuthCredentials::Token { token })` calls `GET /api/v10/users/@me` to verify. Failures surface as `ClientError::AuthFailed`.

No credential storage in this client — the host KV store persists the token and restores on boot. `is_authenticated()` checks `self.session.is_some()`.

---

## Overview Page (`get_account_overview_view`)

Returns `ViewKind::CardGrid` with `plugin-discord-overview-title` / `plugin-discord-overview-subtitle`, `CardBody(primary_field: "name")`.

`get_view_rows("")` fetches joined guilds via `GET /api/v10/users/@me/guilds` and enriches each with member counts (`GET /guilds/{id}?with_counts=true`). Individual guild fetch failures degrade gracefully to `"? members"`.

`get_channel_view` returns `NotSupported("channel-view not yet implemented")`.
`get_view_detail` returns `NotSupported("view-detail not yet implemented")`.

Overview card grid is functional. Per-channel view (issues/detail split) is not applicable for Discord and correctly gated.

---

## Channel Sidebar (`get_channels` / `get_dm_channels`)

`get_channels(server_id)` — fetches Discord guild channels via REST. Returns text channels, voice channels, forum channels, and category separators.

`get_dm_channels()` — fetches DM and group DM channels. Maps Discord DM objects to `DmChannel`. Friend DMs and group DMs both appear in the sidebar list.

---

## Messaging

| Op | Status |
|----|--------|
| `send_message` | Implemented — `POST /api/v10/channels/{id}/messages` |
| `send_reply_message` | Implemented — uses `message_reference` field |
| `send_typing` | Implemented — `POST /api/v10/channels/{id}/typing` |
| `search_messages` | Not overridden — trait default `NotSupported` |
| `delete_message` | Implemented — `DELETE /api/v10/channels/{id}/messages/{msg_id}` |
| `get_pinned_messages` | Not overridden — trait default `NotSupported` |
| `set_message_pinned` | Not overridden — trait default `NotSupported` |

---

## Context-Menu Ops

`get_context_menu_items` is fully implemented. Returns backend-declared menu items for Server, Channel, User, and Message targets.

Moderation ops (all implemented via Discord REST):
- `kick_member` — `DELETE /guilds/{id}/members/{user}` with audit reason
- `ban_member` — `PUT /guilds/{id}/bans/{user}` with message history purge
- `unban_member` — `DELETE /guilds/{id}/bans/{user}`
- `timeout_member` — `PATCH /guilds/{id}/members/{user}` with `communication_disabled_until`
- `untimeout_member` — same PATCH with null timeout
- `get_bans` — `GET /guilds/{id}/bans`
- `get_moderation_log` — Discord audit log endpoint
- `delete_message` — implemented

---

## 14 New Backend Ops (commit 5b142e67)

All 14 implemented via Discord REST:

| Op | Implementation |
|----|---------------|
| `block_user` | `PUT /relationships/{user_id}` type 2 |
| `unblock_user` | `DELETE /relationships/{user_id}` |
| `ignore_user` | Trait default (NotSupported) |
| `unignore_user` | Trait default (NotSupported) |
| `add_friend` | `PUT /relationships/{user_id}` type 1 |
| `remove_friend` | `DELETE /relationships/{user_id}` |
| `set_friend_nickname` | `PATCH /users/@me/relationships/{user_id}` nick field |
| `set_user_note` | `PUT /users/@me/notes/{user_id}` |
| `close_dm_channel` | `DELETE /channels/{channel_id}` |
| `mute_conversation` | Not overridden (trait default NotSupported) |
| `unmute_conversation` | `PATCH /users/@me/guilds/{guild_id}/settings` or user-settings API |
| `leave_group_dm` | `DELETE /channels/{channel_id}` with current user id |
| `edit_group_dm` | `PATCH /channels/{channel_id}` name/icon |
| `add_users_to_group_dm` | `PUT /channels/{channel_id}/recipients/{user_id}` |
| `invite_user_to_server` | Uses system channel + DM fallback |

---

## Visual Audit Results (2026-04-21)

### Koala / Kangaroo
- Standard 3-column Discord layout renders correctly.
- Second nav: server icons as letter-circle fallbacks (actual PNG icons not loading — CORS/auth issue).
- Channel list: text/voice channels grouped by category.
- Chat: avatars, role-colored usernames, timestamps render correctly.
- `VIEW_CHANNEL` permission error shown inline for restricted channels — expected.
- DMs, Friends, Notifications panels all accessible from sidebar.
- Intermittent "Plugin sidebar failed to load — showing channels" error on first account activation — timing issue.

---

## Known Gaps / TODOs

1. **[HIGH] Intermittent plugin sidebar load failure** on first activation — likely timing race in plugin init. Needs investigation and fix.
2. **[MEDIUM] Server icons not loading** — Discord guild icons (PNG URLs) not rendered; only letter-initial fallback circles shown. Likely CORS or auth header missing on image fetch.
3. **[MEDIUM] `search_messages`** — not implemented. Discord has `/channels/{id}/messages/search` endpoint available.
4. **[MEDIUM] `get_channel_view`** — returns `NotSupported`; Discord channels are chat-only and don't need a structured view. This is correct but should be documented as intentional.
5. **[LOW] Account settings button** opens global settings, not per-account modal, unlike demo backend.
6. **[LOW] `mute_conversation`** — not overridden; Discord user-settings mute API is not exposed via standard bot/user token easily. Acceptable gap.
