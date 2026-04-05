# Plugin Data Audit — Which UI Sections Are Populated vs. Hardcoded?

**Goal:** Identify every major UI section and determine whether it is
correctly populated from the active plugin's live data, always showing
hardcoded/demo stub data, or only partially wired (e.g. loaded for demo
but not for real backends).

## Background

Poly supports 6 backends: demo, stoat, matrix, discord, teams, poly-server.
A user noticed the Friends/Members list looked identical across 3 different
plugins — a classic sign that UI sections are reading stale demo data from
`ChatData` instead of fetching from the newly-active backend.

## Key Data Flow to Keep in Mind

```
ClientBackend trait methods
        ↓  called from
toggle_demo / signup on_complete / restore_poly_accounts
        ↓  write into
Signal<ChatData>  (servers, channels, messages, members, dm_channels,
                   groups, friends, notifications, blocked_users, …)
        ↓  read by
UI Components  (FavoritesBar, AccountServerBar, ChannelList,
                ChatView, UserSidebar, FriendsPanel, …)
```

`ClientManager` holds `Arc<RwLock<Box<dyn ClientBackend>>>` per account.
`ChatData` is the reactive cache — it is only as fresh as whoever last
called the backend methods and wrote results in.

There are **three distinct loading paths** for non-demo backends, and they
do not all load the same fields:

| Loading path | File | What it loads |
|---|---|---|
| Demo activation | `ui/demo.rs → toggle_demo` | servers, dm_channels, groups, notifications, friends, voice_channel_participants, blocked_users (demo hardcode), content_policy (demo hardcode) |
| Signup / Add Account (`on_complete` callback) | `ui/signup/mod.rs` | servers, dm_channels, groups, notifications, friends |
| Poly-server account restore on startup | `ui/mod.rs → restore_poly_accounts` | servers, dm_channels, friends — **NOT groups, NOT notifications** |

**Key gap:** `spawn_event_stream_listener` is only called after demo
activation (`ui/demo.rs:392`). No event-stream listener is ever started
for real backends (stoat, matrix, discord, teams, poly-server). This means
real-time events (new messages, presence changes, typing indicators) do not
reach the UI for any non-demo backend.

---

## Known Starting Point

**Friends/Members list** — the user confirmed this looked identical across
3 plugins. Two distinct issues are likely:

1. `UserSidebar` (channel member list) reads `ChatData::members` which is
   populated by `get_channel_members()` on **channel open** — this is wired
   for all backends, but a stale write from a previous backend session will
   persist until a new channel is opened. The file header contains the
   comment `TODO(phase-2.5.7): Wire user sidebar to backend data` — this
   suggests the wiring was known to be incomplete at write time.

2. `FriendsPanel` reads `ChatData::friends` (keyed by account_id). For the
   poly-server restore path (`ui/mod.rs`), friends **are** loaded. But for
   no backend is `blocked_users` ever fetched from the live backend — only
   the demo path hardcodes `poly_demo::data::demo_blocked_users()`.

---

## UI Section Audit Checklist

For each section: mark **[ ]** not yet audited, **[✓]** confirmed wired,
**[!]** confirmed stub/hardcoded, **[~]** partially wired, **[?]** needs
deeper look.

---

### 1. Sidebar — Account Icons (Bar 1, FavoritesBar)

**File:** `crates/core/src/ui/favorites_sidebar.rs` → `AccountIcon`

**What it shows:** One icon per active account with avatar, emoji label,
connection status badge, presence dot, unread count.

**How to check:**
- Avatar URL: `ChatData::account_sessions[account_id].user.avatar_url`
- Emoji label: `ChatData::account_sessions[account_id].icon_emoji`
- Connection status: `ClientManager::connection_statuses[account_id]`
- Presence dot: `ClientManager::presence_statuses[account_id]`
- Unread count: sum of `ChatData::dm_channels[*].unread_count` and
  `ChatData::notifications[*].read == false` for this account

**Data source assessment:**
- `account_sessions` is populated for all backends on signup/restore — **[ ]**
- `connection_statuses` is set to `Connected` on commit but never updated
  from real backend events — **[ ]**
- Presence uses `AccountPresence` (user-chosen, not backend-reported) — **[ ]**
- Unread badge is only as fresh as `dm_channels` / `notifications` in ChatData — **[ ]**

---

### 2. Sidebar — Favorited Server Icons (Bar 1, FavoritesBar)

**File:** `crates/core/src/ui/favorites_sidebar.rs` → `FavoriteServerIcon`

**What it shows:** Server icon, name, account badge overlay, unread/mention
badges, connection status overlay.

**How to check:**
- Server list comes from `ChatData::servers` filtered by
  `ChatData::favorited_server_ids`
- Icon URL, name, unread/mention counts come from the `Server` struct as
  loaded by `get_servers()` during activation
- Unread/mention counts are **never refreshed** after initial load — no
  event-stream polling updates `Server::unread_count` for real backends

**Data source assessment:** **[ ]**

---

### 3. Account Server Bar (Bar 2, AccountServerBar)

**File:** `crates/core/src/ui/account/common/account_server_bar.rs`

**What it shows:** Per-account DMs button, Friends button, Notifications
button with badge, then one server icon per server belonging to this account.

**How to check:**
- Server list: `ChatData::servers` filtered by `server.account_id == active_account_id`
- Unread badge on notifications button: count of `ChatData::notifications`
  where `!n.read && n.account_id == account_id`
- Server icons: same data as Bar 1 above

**Data source assessment:** **[ ]**

---

### 4. Channel List — Server Channel View

**File:** `crates/core/src/ui/account/common/channel_list.rs` → `ServerChannelView`

**What it shows:** Server categories, text/voice/forum channels with unread
dots, category collapse.

**How to check:**
- `ChatData::channels` is populated by `get_channels()` via `load_server_data`
  in `favorites_sidebar.rs` — this **is** called for all backends on server
  click. Trace: server icon click → `load_server_data()` → `backend.get_channels()`
- Category collapse state is local UI signal — not backend data
- `ChatData::channels` is cleared before every server switch, so stale data
  from a previous backend session should not persist **as long as a server
  click triggers a reload**

**Special gate (demo-only feature):** The "Channels & Roles" button in the
server banner header is only shown when `server.backend == BackendType::from("demo")`
(line 387 in channel_list.rs). This is an intentional demo-only feature but
is worth noting as a place where behavior differs by backend.

**Data source assessment:** **[ ]**

---

### 5. Channel List — DMs and Groups List (DMFriendsView)

**File:** `crates/core/src/ui/account/common/channel_list.rs` → `DMFriendsView`

**What it shows:** Sorted list of DM conversations and group DMs for the
active account.

**How to check:**
- `ChatData::dm_channels` filtered by `dm.account_id == active_account_id`
- `ChatData::groups` filtered by `g.account_id == active_account_id`
- These are loaded at signup/activation time; never refreshed live

**Known gap for poly-server restore path:** `ui/mod.rs → restore_poly_accounts`
loads `get_dm_channels()` and `get_friends()` but does **NOT** load
`get_groups()`. Groups will be empty for a restored poly-server account
until the user navigates away and reconnects.

**Data source assessment:** **[~]** (DMs wired for all paths, groups missing
from poly-server restore path)

---

### 6. Chat Messages View

**File:** `crates/core/src/ui/account/common/chat_view.rs`

**What it shows:** Chat history, compose box, message context menu, reactions,
pinned messages, slash-command autocomplete, emoji picker.

**How to check:**
- Messages: `ChatData::messages` populated by `get_messages()` in
  `load_channel_data()` for server channels, or `load_dm_messages()` for
  DMs — both call the active backend directly. Trace:
  `ChannelList` item click → `load_channel_data` → `backend.get_messages()`
- Real-time new messages: only pushed via `spawn_event_stream_listener` which
  is **only started for demo backends**. For all real backends, new messages
  only appear after a manual channel re-selection (full reload).
- Typing indicator: `TypingIndicator` component reads `ChatData::typing_users`
  which is never populated (the `TypingStarted` branch in demo.rs is a
  `TODO(phase-3)` comment stub). This means typing indicators never show
  for any backend including demo.

**Data source assessment:**
- Messages on channel open: **[ ]** (likely wired)
- Real-time message delivery for real backends: **[!]** (never happens — no event stream)
- Typing indicator: **[!]** (hardcoded empty — never populated for any backend)

---

### 7. Member List — Channel Member Sidebar (UserSidebar)

**File:** `crates/core/src/ui/account/common/user_sidebar.rs`

**What it shows:** Channel members grouped by presence status, with search/filter.

**How to check:**
- Reads `ChatData::members`
- Populated by `get_channel_members()` inside `load_channel_data()` on channel open
- The file header has `TODO(phase-2.5.7): Wire user sidebar to backend data`
- Presence within member list entries comes from `User::presence` as returned
  by `get_channel_members()` — this is a snapshot at load time, not live
- Real-time presence updates: only handled via `ClientEvent::PresenceChanged`
  in `spawn_event_stream_listener` — **only runs for demo**

**Confirmed known issue (user-reported):** Members list looks identical
across plugins because `ChatData::members` is populated once at channel open
and is never cleared until the next channel selection. If the user switches
backend accounts without changing the selected channel URL, the old members
list remains visible.

**Data source assessment:** **[~]** (wired at channel open for all backends;
real-time presence updates and automatic stale-clear are demo-only)

---

### 8. Member List — DM Group Member Sidebar (DmUserSidebar)

**File:** `crates/core/src/ui/account/common/dm_user_sidebar.rs`

**What it shows:** Members of the current group DM, with a remove-member button.

**How to check:**
- Reads `ChatData::active_group_members` (for group DMs) or `ChatData::members`
- `active_group_members` is populated from `Group::members` when a group
  conversation is opened in `DMFriendsView`
- Presence dots on each member come from `User::presence` at load time only

**Data source assessment:** **[ ]**

---

### 9. Notifications View

**File:** `crates/core/src/ui/account/common/notifications.rs`

**What it shows:** Aggregated notifications with per-kind filtering, mark-read,
accept/reject friend requests, server invites.

**How to check:**
- Reads `ChatData::notifications`
- File header: `TODO(phase-2.5.8): Wire notifications to backend data`
- Notifications are loaded from `get_notifications()` at signup/activation time
  for all backends including stoat's signup path
- Real-time new notification delivery: requires event stream — **not wired for
  real backends**
- The "Accept friend request" action correctly calls
  `backend.respond_to_friend_request()` and then refreshes the friends list —
  this appears wired for all backends

**Data source assessment:** **[~]** (snapshot at login works; live delivery
absent for real backends)

---

### 10. Friends Panel (FriendsPanel)

**File:** `crates/core/src/ui/account/common/friends_panel.rs`

**What it shows:** Friends tab (with grid of friends), Ignored tab (empty
placeholder), Blocked tab (blocked users grid).

**How to check:**
- Friends tab: `ChatData::friends[account_id]` — populated at login for all
  backends, refreshed after accepting friend requests
- Blocked tab: `ChatData::blocked_users` — **only ever populated by the demo
  path** via `poly_demo::data::demo_blocked_users()`. The trait method
  `get_blocked_users()` has a default impl returning `Ok(Vec::new())` and is
  never called for any real backend in core's loading paths.
- Ignored tab: always shows `IgnoredUsersPlaceholder` — hardcoded empty state

**Data source assessment:**
- Friends list: **[~]** (loaded at login for most backends; missing from
  poly-server restore, see section 5)
- Blocked users: **[!]** for all real backends (demo hardcode only; `get_blocked_users()`
  never called for any non-demo backend)
- Ignored users: **[!]** (always empty placeholder regardless of backend)

---

### 11. User Profile Modal

**File:** `crates/core/src/ui/account/common/user_profile_modal.rs`

**What it shows:** User avatar, banner, display name, presence, backend badge,
action buttons (Message, Call, Video), note text area.

**How to check:**
- Modal data comes from the `User` struct stored in `AppState::nav::profile_modal_user`
- This is set by `open_user_profile()` passing whatever `User` was in the
  member list or DM contact at click time — so the data is only as fresh as
  the most recent member list or DM load
- No live fetch of the user profile occurs on open (no `get_user()` call)
- The "Message" button calls `open_direct_message_from_active_account` which
  correctly calls `backend.open_direct_message_channel()` — wired for all
  backends that implement it

**Data source assessment:** **[~]** (data is from last backend fetch, not
live; `get_user()` is never called on profile open)

---

### 12. Voice Bar / Voice Latency

**File:** `crates/core/src/ui/account/common/voice_bar.rs`

**What it shows:** Connected participants, mute/deafen/video controls,
signal quality bars with latency popup.

**How to check:**
- `VoiceLatencyBar` hardcodes `latency_ms: u32 = 42` and `server_loc = "EU-West (demo)"`.
  This is called out in the code comment: `DECISION(V-5): CSS bars for signal
  quality; hardcoded demo latency.`
- Voice participant data in `voice_channel_participants` is loaded via
  `get_voice_participants()` in `load_channel_data` — wired for all backends

**Data source assessment:**
- Voice participants: **[ ]** (likely wired)
- Latency/signal quality: **[!]** (hardcoded demo values, no real backend data)

---

### 13. Search Results

**File:** `crates/core/src/ui/search.rs`

**What it shows:** Global tree-browse of servers/channels/groups/DMs across
all accounts; conversation search within a specific account.

**How to check (global search):**
- Reads `ChatData::servers`, `ChatData::dm_channels`, `ChatData::groups`
  directly — shows whatever is in the reactive store, filtered by text query
- Does not make live backend calls; only as fresh as last ChatData write
- `backend_search_messages` / `search_messages()` is called for
  account-scoped message search — wired per backend (returns `NotSupported`
  by default)

**File:** `crates/core/src/ui/account/common/conversation_search_view.rs`

**Data source assessment:** **[~]** (tree browse reads ChatData snapshot;
message search calls backend live)

---

### 14. Content & Social Policy Settings

**File:** `crates/core/src/ui/account/settings/content_social.rs`

**What it shows:** Sensitive content filters, spam filter, friend request
settings, blocked users list.

**How to check:**
- Reads and writes `ChatData::content_policy` and `ChatData::blocked_users`
- File comment explicitly states: `TODO(phase-3.x): call set_content_policy
  on the active backend`
- `content_policy` is only populated in the demo activation path via
  `poly_demo::data::demo_content_policy()` — never fetched via
  `get_content_policy()` for any real backend
- `blocked_users` same situation — see section 10

**Data source assessment:** **[!]** (demo hardcode only; never loaded or saved
for real backends)

---

### 15. Voice Connection Banner (VoiceBanner)

**File:** `crates/core/src/ui/voice_banner.rs`

**What it shows:** Banner across the top of the layout when in a voice call.

**How to check:**
- Reads `ChatData::voice_connection`
- Voice calls are initiated locally and stored in ChatData — not fetched from
  the backend
- Actual WebRTC integration is not yet implemented; this is UI-only

**Data source assessment:** **[ ]** (state is local, not backend-sourced)

---

### 16. Saved Items View

**File:** `crates/core/src/ui/account/common/saved_items_view.rs`

**What it shows:** Locally pinned/bookmarked messages.

**How to check:**
- Check if saved items are read from backend `get_pinned_messages()` or from
  local storage only
- Trace from the component to see what signal/store it reads

**Data source assessment:** **[ ]** (not yet investigated)

---

## Summary Table

| UI Section | Signal/Field | Loaded for all backends? | Real-time updates? |
|---|---|---|---|
| Account icons (Bar 1) | `account_sessions`, `connection_statuses` | Yes (sessions); connection never updated live | No |
| Favorited server icons (Bar 1) | `servers`, `favorited_server_ids` | Yes on login | No (no unread refresh) |
| Account server bar (Bar 2) | `servers` per account | Yes on login | No |
| Channel list (server) | `channels` | Yes on server click | No |
| DMs list | `dm_channels` | Yes on login | No |
| Groups list | `groups` | Yes on demo + signup; **NO on poly-server restore** | No |
| Chat messages | `messages` | Yes on channel open | **Demo only** (event stream) |
| Typing indicator | `typing_users` | **Never populated (TODO stub)** | N/A |
| Channel member list | `members` | Yes on channel open | Demo only (presence changes) |
| DM group member list | `active_group_members` | Yes on group open | No |
| Notifications | `notifications` | Yes on login | **No for real backends** |
| Friends list | `friends[account_id]` | Yes on login (except poly-server restore misses groups) | No |
| Blocked users | `blocked_users` | **Demo hardcode only** | N/A |
| Ignored users | n/a | **Empty placeholder always** | N/A |
| User profile modal | `AppState::profile_modal_user` | Snapshot from member list | No live fetch |
| Voice participants | `voice_channel_participants` | Yes on voice channel open | No |
| Voice latency/signal | n/a | **Hardcoded: 42 ms, EU-West** | N/A |
| Content & social policy | `content_policy` | **Demo hardcode only** | N/A |
| Search (tree browse) | `servers`, `dm_channels`, `groups` | ChatData snapshot only | No |
| Search (message search) | backend `search_messages()` | Live call per backend | N/A |

---

## High-Priority Findings

These are the items most likely to cause the "looks the same across plugins"
symptom the user reported:

1. **`spawn_event_stream_listener` is only wired for demo** (`ui/demo.rs:392`).
   No real backend ever receives real-time events. This means messages,
   presence, typing, notifications, and unread counts never update live for
   stoat/matrix/discord/teams/poly-server.

2. **`blocked_users` is demo-only hardcode** (`ui/demo.rs:349-350`).
   `get_blocked_users()` is never called for any real backend. The Blocked
   tab in FriendsPanel will always be empty for real backends.

3. **`content_policy` is demo-only hardcode** (`ui/demo.rs:349`).
   `get_content_policy()` is never called; Content & Social Settings shows
   the demo policy for all real backends.

4. **`typing_users` is never populated** (`ui/demo.rs:486-488` is a
   `TODO(phase-3)` stub). The `TypingIndicator` component always renders
   empty for all backends including demo.

5. **`groups` not loaded on poly-server account restore** (`ui/mod.rs`
   restore path loads `get_dm_channels()` and `get_friends()` but skips
   `get_groups()`). Group DMs vanish after restart for poly-server accounts.

6. **Member list stale across backend switches** — `ChatData::members` is
   only cleared when a new channel is actively opened. Switching account
   context without opening a channel leaves the old backend's member list
   visible. File header confirms: `TODO(phase-2.5.7): Wire user sidebar to
   backend data`.

7. **Notification count badge on server icons never refreshes** — the
   `Server::unread_count` and `Server::mention_count` fields are set once
   at `get_servers()` call time and never incremented via events for real
   backends.

---

## How to Audit Each Item

For each unchecked item above:

1. **Find the component file** (use the File column in the table above or
   the file paths in the checklist).

2. **Identify the `Signal<ChatData>` fields read.** Every component calls
   `use_context::<Signal<ChatData>>()` and reads specific fields. These are
   the observable sources.

3. **Trace the write path.** Search for `chat_data.write().<field> =` or
   `.extend()` / `.push()` targeting that field. The three loading paths are:
   - `ui/demo.rs → toggle_demo` (demo activation)
   - `ui/signup/mod.rs → on_complete` callback (new account sign-in)
   - `ui/mod.rs → restore_poly_accounts` (app restart restore)

4. **Check if all three loading paths write the field.** If a field is only
   written in one or two paths, it is a partial wire.

5. **Check event stream coverage.** Search `ui/demo.rs → spawn_event_stream_listener`
   for `ClientEvent::*` variants that update the field. Then verify whether
   `spawn_event_stream_listener` is ever called outside the demo path.

6. **Test cross-plugin by toggling backends.** With the desktop MCP tools,
   activate demo, screenshot the section, deactivate demo, activate a real
   backend (stoat or poly-server if available), navigate to the same section,
   screenshot again. If the screenshots look identical, it confirms the
   data is stale from the demo or simply not loaded.

---

## Files Most Relevant for Implementation Fixes

- `crates/core/src/ui/demo.rs` — `spawn_event_stream_listener` needs to be
  generalized and called for all backends, not just demo
- `crates/core/src/ui/mod.rs` — `restore_poly_accounts` is missing
  `get_groups()`, `get_notifications()`, `get_content_policy()`, and
  `get_blocked_users()` calls
- `crates/core/src/ui/signup/mod.rs` — `on_complete` callback is missing
  `get_blocked_users()` and `get_content_policy()` calls
- `crates/core/src/ui/account/common/user_sidebar.rs` — member list
  clearing on backend/account switch needs to be verified
- `crates/core/src/state/chat_data.rs` — the `ChatData` struct; verify
  fields that should be per-account vs. global
