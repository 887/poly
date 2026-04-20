# Plan — Discord Forum Channels & Thread Support

> **Created:** 2026-04-05
> **Status:** Phase 1–5 Done (2026-04-19); Phase 6 Done (2026-04-19) except 6.5 (gateway events — Phase 7 follow-up)
> **Crate:** `poly-discord` (`clients/discord/`)
> **Depends on:** Phase 3.3 (Discord client base implementation)
> **Goal:** Support Discord forum channels and threads in the Poly unified chat UI.

---

## Overview

Discord has two features that don't map to simple text channels:

1. **Forum channels** — a channel type where every "message" is actually a thread
   with a title, starter post, tags, and a full conversation inside it.
2. **Threads** — lightweight sub-conversations spawned from any message in a text
   channel, or created as posts in a forum channel.

Both need explicit support in the data model, backend client, and UI.

---

## Discord API Reference

### Channel Types

| Type | Value | Description |
|---|---|---|
| `GUILD_TEXT` | 0 | Standard text channel |
| `GUILD_VOICE` | 2 | Voice channel |
| `GUILD_CATEGORY` | 4 | Category (folder) |
| `GUILD_ANNOUNCEMENT` | 5 | Announcement / news channel |
| `ANNOUNCEMENT_THREAD` | 10 | Thread in an announcement channel |
| `PUBLIC_THREAD` | 11 | Public thread |
| `PRIVATE_THREAD` | 12 | Private thread (server boosted) |
| `GUILD_FORUM` | 15 | Forum channel |
| `GUILD_MEDIA` | 16 | Media channel (forum variant, requires media) |

### Forum Channel Object (relevant fields)

```
available_tags: [{ id, name, moderated, emoji_id, emoji_name }]
default_reaction_emoji: { emoji_id, emoji_name }
default_sort_order: 0 (Latest Activity) | 1 (Creation Date)
default_forum_layout: 0 (Not set) | 1 (List) | 2 (Gallery)
default_thread_rate_limit_per_user: int (slowmode for new posts)
```

### Thread Object (relevant fields)

```
id, name, type (10/11/12)
guild_id, parent_id (the forum or text channel it belongs to)
owner_id (user who created the thread)
message_count, member_count
thread_metadata: {
  archived: bool,
  auto_archive_duration: 60 | 1440 | 4320 | 10080 (minutes),
  archive_timestamp: ISO8601,
  locked: bool,
  invitable: bool (private threads only),
  create_timestamp: ISO8601
}
applied_tags: [tag_id, ...] (forum threads only)
total_message_sent: int (includes deleted messages, unlike message_count)
```

### API Endpoints

| Endpoint | Description |
|---|---|
| `GET /channels/{channel.id}` | Get channel (includes forum metadata) |
| `GET /guilds/{guild.id}/threads/active` | List all active threads in a guild |
| `GET /channels/{channel.id}/threads/archived/public` | Archived public threads |
| `GET /channels/{channel.id}/threads/archived/private` | Archived private threads |
| `POST /channels/{channel.id}/threads` | Create a thread (in text channel) |
| `POST /channels/{forum.id}/threads` | Create a forum post (thread with message) |
| `GET /channels/{thread.id}/messages` | Get messages in a thread (standard messages endpoint) |
| `PUT /channels/{thread.id}/thread-members/@me` | Join a thread |
| `DELETE /channels/{thread.id}/thread-members/@me` | Leave a thread |

### Gateway Events

| Event | When |
|---|---|
| `THREAD_CREATE` | Thread created or user joins a thread |
| `THREAD_UPDATE` | Thread metadata changed (name, archived, locked) |
| `THREAD_DELETE` | Thread deleted |
| `THREAD_LIST_SYNC` | Sent on READY or when gaining access to a channel; lists active threads |
| `THREAD_MEMBER_UPDATE` | Current user's thread member object updated |
| `THREAD_MEMBERS_UPDATE` | Members added/removed from a thread (privileged intent) |

---

## Data Model Changes

### 1. ChannelType Extension

Add to `poly_client::ChannelType`:

```rust
enum ChannelType {
    Text,
    Voice,
    Video,
    Forum,       // NEW — Discord GUILD_FORUM / GUILD_MEDIA
    Thread,      // NEW — a thread within a text or forum channel
    Announcement // NEW — Discord GUILD_ANNOUNCEMENT
}
```

### 2. New Types

```rust
/// A tag available in a forum channel.
struct ForumTag {
    id: String,
    name: String,
    emoji: Option<String>,   // Unicode emoji or custom emoji ID
    moderated: bool,         // Only mods can apply this tag
}

/// A forum post (thread within a forum channel).
struct ForumPost {
    /// The thread channel backing this post.
    thread: Channel,
    /// Title of the post (= thread name).
    title: String,
    /// The starter message (first message in the thread).
    starter_message: Option<Message>,
    /// Tags applied to this post.
    tags: Vec<String>,       // tag IDs
    /// Number of messages in the post thread.
    message_count: u32,
    /// Whether the post is pinned in the forum.
    pinned: bool,
    /// Whether the post is archived.
    archived: bool,
    /// Whether the post is locked (no new messages).
    locked: bool,
    /// When the post was created.
    created_at: DateTime<Utc>,
    /// When the last message was sent (for sort-by-activity).
    last_activity: DateTime<Utc>,
}

/// Metadata for a thread attached to a regular text channel message.
struct ThreadInfo {
    /// The thread channel ID.
    thread_id: String,
    /// Thread name / title.
    name: String,
    /// Number of messages in the thread.
    message_count: u32,
    /// Whether the thread is archived.
    archived: bool,
}
```

### 3. Channel Additions

Add optional fields to `Channel`:

```rust
struct Channel {
    // ... existing fields ...

    /// For Forum channels: available tags.
    pub forum_tags: Vec<ForumTag>,
    /// For Forum channels: default sort order.
    pub forum_sort_order: Option<ForumSortOrder>,  // LatestActivity | CreationDate
    /// For threads: parent channel ID.
    pub parent_channel_id: Option<String>,
    /// For threads: thread metadata.
    pub thread_metadata: Option<ThreadMetadata>,
}

struct ThreadMetadata {
    pub archived: bool,
    pub locked: bool,
    pub auto_archive_duration_minutes: u32,
    pub archive_timestamp: DateTime<Utc>,
}
```

### 4. Message Additions

Add optional field to `Message`:

```rust
struct Message {
    // ... existing fields ...

    /// If this message spawned a thread, metadata about it.
    pub thread: Option<ThreadInfo>,
}
```

---

## ClientBackend Changes

### New Methods

Add to `ClientBackend` trait (with default `NotSupported` implementations so other
backends are unaffected):

```rust
/// Get forum posts (threads) in a forum channel.
///
/// Returns posts sorted by the forum's default sort order.
/// `before` and `limit` support pagination.
async fn get_forum_posts(
    &self,
    channel_id: &str,
    limit: Option<u32>,
    before: Option<String>,
) -> ClientResult<Vec<ForumPost>> {
    let _ = (channel_id, limit, before);
    Err(ClientError::NotSupported("get_forum_posts".to_string()))
}

/// Get active threads in a server or channel.
async fn get_active_threads(
    &self,
    server_id: &str,
) -> ClientResult<Vec<Channel>> {
    let _ = server_id;
    Err(ClientError::NotSupported("get_active_threads".to_string()))
}

/// Get archived threads in a channel.
async fn get_archived_threads(
    &self,
    channel_id: &str,
    limit: Option<u32>,
    before: Option<String>,
) -> ClientResult<Vec<Channel>> {
    let _ = (channel_id, limit, before);
    Err(ClientError::NotSupported("get_archived_threads".to_string()))
}
```

### Existing Method Behavior

- **`get_channels(server_id)`** — Must return `Forum`-type channels for `GUILD_FORUM`
  (type 15) and `GUILD_MEDIA` (type 16). Do NOT return individual threads as channels;
  threads are fetched separately.
- **`get_messages(channel_id, query)`** — Works unchanged for thread channels. A
  thread ID is a valid channel ID in Discord's API, so `get_messages(thread_id, ...)`
  returns the thread's messages.
- **`get_channel(id)`** — Must handle thread channel IDs and return `ChannelType::Thread`
  with populated `parent_channel_id` and `thread_metadata`.

---

## Discord Client Implementation

### Channel Mapping

```
Discord type 15 (GUILD_FORUM)        -> ChannelType::Forum
Discord type 16 (GUILD_MEDIA)        -> ChannelType::Forum  (same treatment)
Discord type 10 (ANNOUNCEMENT_THREAD) -> ChannelType::Thread
Discord type 11 (PUBLIC_THREAD)       -> ChannelType::Thread
Discord type 12 (PRIVATE_THREAD)      -> ChannelType::Thread
Discord type 5  (GUILD_ANNOUNCEMENT)  -> ChannelType::Announcement
```

### get_forum_posts Implementation

1. Call `GET /guilds/{guild_id}/threads/active` to get all active threads
2. Filter to threads whose `parent_id` matches the forum channel ID
3. For each thread, fetch the starter message via `GET /channels/{thread_id}/messages?limit=1&after=0`
4. Build `ForumPost` structs with tag resolution against the forum's `available_tags`
5. Sort by `default_sort_order` (latest activity or creation date)

### Gateway Event Handling

| Event | Action |
|---|---|
| `THREAD_CREATE` | Emit `ClientEvent::ChannelCreated` with `ChannelType::Thread` |
| `THREAD_UPDATE` | Emit `ClientEvent::ChannelUpdated` (update metadata, archived state) |
| `THREAD_DELETE` | Emit `ClientEvent::ChannelDeleted` |
| `THREAD_LIST_SYNC` | Bulk update thread cache for affected channels |
| `MESSAGE_CREATE` in thread | Standard `ClientEvent::MessageReceived` (thread ID = channel ID) |

---

## UI Integration

### Sidebar

- Forum channels render with a forum icon (speech-bubble-with-lines or hashtag-with-post
  icon, distinct from text channel `#`).
- Forum channels do NOT expand to show threads in the sidebar. Clicking a forum channel
  opens the forum post list in the main content area.
- Active threads from text channels appear in a collapsible "Threads" section below
  the parent channel in the sidebar, or as an indicator count.

### Forum Post List View (Main Content Area)

When a Forum channel is selected:

1. **Header** — Forum channel name + description + tag filter bar.
2. **Post list** — Each post shows: title, author avatar + name, tag pills, message
   count, last activity timestamp. Two layout modes:
   - **List view** — compact rows (default for `GUILD_FORUM`)
   - **Gallery view** — card grid with media preview (default for `GUILD_MEDIA`)
3. **Sort controls** — "Latest Activity" / "Creation Date" toggle.
4. **Tag filter** — clickable tag pills to filter posts by applied tags.
5. **New Post button** — opens a compose dialog with title field + tag selector + message body.

Clicking a post opens the thread message view (identical to opening a thread from
a text channel).

### Thread View

When a thread (from forum or text channel) is opened:

1. **Thread header** — Thread name, close button, member count, pinned/archived badges.
2. **Message list** — Standard message view (same component as channel messages).
3. **Composer** — Standard message composer at the bottom.

Thread view can render in two modes:
- **Panel mode** — right-side panel alongside the parent channel (like Discord's
  thread panel). Parent channel stays visible on the left.
- **Full mode** — replaces the main content area (for mobile / narrow viewports,
  or when opened from forum post list).

### Message Thread Indicators

In a regular text channel message list, messages that have spawned threads show:

- A "View Thread" button below the message with thread name and reply count.
- Clicking the button opens the thread in panel mode.

### Active Threads Bar

At the top of a text channel (below the channel header), if the channel has active
threads, show a compact bar: "N active threads" with a dropdown listing thread
names. Clicking a thread opens it in panel mode.

---

## Implementation Checklist

### 1. Data Model

- [x] **1.1** Add `Forum`, `Thread`, `Announcement` to `ChannelType` enum
- [x] **1.2** Add `ForumTag`, `ForumPost`, `ThreadInfo`, `ThreadMetadata` structs to `types.rs`
- [x] **1.3** Add optional `forum_tags`, `parent_channel_id`, `thread_metadata` fields to `Channel`
- [x] **1.4** Add optional `thread: Option<ThreadInfo>` field to `Message`
- [x] **1.5** Add `ForumSortOrder` enum (`LatestActivity`, `CreationDate`)

### 2. ClientBackend Trait

- [x] **2.1** Add `get_forum_posts()` with default `NotSupported`
- [x] **2.2** Add `get_active_threads()` with default `NotSupported`
- [x] **2.3** Add `get_archived_threads()` with default `NotSupported`
- [x] **2.4** Verify all existing backends still compile (default impls, no breakage)

### 3. Discord Client

- [x] **3.1** Map Discord channel types 15/16 to `ChannelType::Forum` in `get_channels()`
- [x] **3.2** Map Discord channel types 10/11/12 to `ChannelType::Thread` in `get_channel()`
- [x] **3.3** Implement `get_forum_posts()` using active threads endpoint + message fetch
- [x] **3.4** Implement `get_active_threads()` using `GET /guilds/{id}/threads/active`
- [x] **3.5** Implement `get_archived_threads()` using archived threads endpoint
- [x] **3.6** Parse `thread_metadata` from Discord thread objects
- [x] **3.7** Parse `available_tags` from forum channel objects
- [x] **3.8** Handle `THREAD_CREATE`, `THREAD_UPDATE`, `THREAD_DELETE` gateway events
- [x] **3.9** Handle `THREAD_LIST_SYNC` for bulk thread cache population
- [x] **3.10** Populate `Message.thread` field when message has a thread spawned from it

### 4. UI — Forum Channel

- [x] **4.1** Forum icon in sidebar channel list
- [x] **4.2** Forum post list view (list layout)
- [x] **4.3** Forum post list view (gallery layout for media channels)
- [x] **4.4** Tag filter bar with clickable tag pills
- [x] **4.5** Sort order toggle (Latest Activity / Creation Date)
- [x] **4.6** New Post compose dialog (title + tags + body)

### 5. UI — Threads

- [x] **5.1** "View Thread" button on messages that have spawned threads
- [x] **5.2** Thread panel (right-side, panel mode)
- [x] **5.3** Thread full-page view (mobile / narrow viewport)
- [x] **5.4** Active threads bar at top of text channels
- [x] **5.5** Thread header with name, member count, archived/pinned badges
- [x] **5.6** Thread close button (returns to parent channel)

### 6. Test Server

- [x] **6.1** Add forum channel to mock Discord test server seed data
  - Forum channel 500 (`general-discussion`, GUILD_FORUM type 15) in `servers/test-discord/src/state.rs`
  - 3 tags: question, show-and-tell, announcement; `default_forum_layout=1`
  - Media channel 600 (`media-gallery`, GUILD_MEDIA type 16) in guild 101; `default_forum_layout=2`
- [x] **6.2** Add threads (forum posts + text channel threads) to seed data
  - Forum posts 501 (active/question), 502 (active/show-and-tell), 503 (archived+locked/announcement)
  - Inline text thread 511 (parent=510, spawned from msg 520)
  - Media thread 601 (parent=600, tag=photos, has attachment)
  - Full message sets in channels 501/502/503/511/601 with proper timestamps
- [x] **6.3** Mock `GET /guilds/{id}/threads/active` endpoint
  - `routes::get_guild_active_threads` — filters non-archived threads by guild_id
  - Returns `{ threads: [...], has_more: false }`
- [x] **6.4** Mock archived threads endpoints
  - `routes::get_channel_archived_threads` — filters archived public threads by parent_id
  - Returns `{ threads: [...], has_more: false }`
  - Note: `/private` endpoint not added (no seeded private threads; Discord API rarely used)
- [ ] **6.5** Mock `THREAD_CREATE` / `THREAD_UPDATE` / `THREAD_DELETE` gateway events
  - **PUNTED**: The mock server has no WebSocket gateway layer (gateway URL returns `ws://localhost:9102` stub).
  - The client's `parse_gateway_event()` is fully unit-tested standalone in `clients/discord/tests/mapping.rs`.
  - Emitting gateway events would require implementing a WS server in `test-discord` — Phase 7 scope.
  - The `DiscordEvent` enum already has `GuildCreate`/`MessageCreate` variants; adding thread variants is a small follow-up once the WS layer is built.

---

## Completion Criteria

- [x] Forum channels appear in the sidebar with distinct icon
  — `crates/core/src/ui/account/common/channel_list.rs:1359`: `ChannelType::Forum => "📋"` (distinct from `#` text channels)
- [x] Clicking a forum channel shows the post list with tags and sort
  — `crates/core/src/ui/routes.rs:1721–1731`: `is_discord_forum` flag dispatches to `DiscordForumView {}`
  — `crates/core/src/ui/account/common/discord_forum_view.rs:86`: `DiscordForumView` renders `ForumHeader` (sort toggle + New Post) + `ForumTagBar` + `ForumPostList`/`ForumPostGallery`
- [x] Clicking a forum post opens the thread message view
  — `crates/core/src/ui/account/common/discord_forum_view.rs`: `ForumPostRow` links to `Route::ServerChat { channel_id: thread_id }` — thread IDs are valid channel IDs in Discord's API, so `ChatView` renders the thread's messages
- [x] Text channel messages with threads show "View Thread" indicator
  — `crates/core/src/ui/account/common/chat_view.rs:3900–3903`: `if let Some(thread_info) = msg.thread.clone() { ViewThreadButton { thread: thread_info } }`
  — `crates/core/src/ui/account/common/thread_view.rs:80`: `ViewThreadButton` renders "💬 N replies" button
- [x] Thread panel opens alongside parent channel on desktop
  — `crates/core/src/ui/account/common/chat_view.rs`: `render_chat_body_shell` renders `ThreadPanel {}` when `nav.thread_panel_open.is_some()` and not mobile
  — `ViewThreadButton` on desktop sets `nav.thread_panel_open = Some(thread_id)` to open the panel
- [x] Thread full view works on mobile viewport
  — `crates/core/src/ui/routes.rs:372–383`: `Route::ThreadView { ..thread_id }` renders `ThreadFullView { thread_id }`
  — `crates/core/src/ui/account/common/thread_view.rs:421`: `ThreadFullView` shows back button + header + message list
  — On mobile `ViewThreadButton` navigates to `Route::ThreadView` instead of setting panel state
- [x] Active threads bar shows at top of channels with threads
  — `crates/core/src/ui/account/common/chat_view.rs:3147–3155`: `render_chat_content_column` adds `ActiveThreadsBar {}` above message list
  — `crates/core/src/ui/account/common/thread_view.rs:149`: `ActiveThreadsBar` calls `get_active_threads` and shows chip bar when threads exist
- [x] All other backends compile unchanged (default trait impls)
  — `cargo check --workspace` passes with zero errors; all backends use default `NotSupported` impls for forum/thread methods
- [x] Mock test server covers forum + thread flows
  — 13 unit tests in `servers/test-discord/tests/forum_threads.rs` all pass
  — 9 integration tests in `clients/discord/tests/integration.rs` cover forum/thread flows (6.3–6.4 endpoints, forum posts, archived threads, message thread field, active threads)
  — **Gateway events (6.5) not exercised at server level** — WS gateway layer not implemented in mock server (Phase 7 follow-up)
