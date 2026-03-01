# Phase 2.5 Plan — Demo Client Integration & Discord-Style Chat UI

> **Status:** 🔲 Not Started  
> **Parent:** [Phase 2 Plan](phase-2-plan.md)  
> **Depends On:** Phase 2.4 ✅ (crypto, sync, client trait, demo client all implemented)  
> **Blocks:** Phase 3 (real client implementations need this UI wiring in place)  
> **Last Updated:** 2026-03-01

---

## Overview

**The problem:** The demo client (`poly-demo`) generates rich mock data — 3 servers, 12
channels, 10 users, channel-specific messages, DMs, groups, notifications — but the UI
components in `crates/core/src/ui/` have hardcoded placeholder data and no connection
to the `ClientBackend` trait. The state module only tracks navigation, not data.

**The goal:** Wire everything together so the demo client powers the UI end-to-end.
A user launches the app, clicks a "Demo" button, and gets a fully populated Discord-style
experience: servers in the sidebar, channels that load, messages that render like Discord
(avatar + username + timestamp + content + inline images), and a message input that works.

**Scope:**
1. **Client manager** — reactive store that holds `Box<dyn ClientBackend>` instances
2. **Demo toggle** — button above the settings gear to activate/deactivate demo mode
3. **Data stores** — `Signal<Vec<Server>>`, `Signal<Vec<Message>>`, etc. populated from backends
4. **UI wiring** — replace all hardcoded placeholders with backend-powered data
5. **Discord-style message rendering** — avatar, username, timestamp, multi-line content, inline images, message grouping, date separators
6. **Functional message sending** — type a message, hit Enter/Send, see it appear
7. **Demo event stream** — periodic fake messages and presence changes
8. **i18n** — new translation keys for all new UI text

---

## Architecture

### Client Manager

A new module `crates/core/src/client_manager.rs` that manages active backends:

```
ClientManager {
    backends: HashMap<String, Arc<dyn ClientBackend>>    // keyed by account ID
    demo_active: bool                                     // is demo client loaded?
    active_account: Option<String>                        // which account is actively selected
}
```

Provided as a Dioxus context (`Signal<ClientManager>`) at the `App` level, alongside
`Signal<AppState>`. Components read data from the client manager's backends.

### Data Flow

```
User clicks server → AppState.nav.selected_server = "server-poly-dev"
                   → ChatData.load_channels(server_id) via ClientManager
                   → Signal<Vec<Channel>> updated → ChannelList re-renders

User clicks channel → AppState.nav.selected_channel = "ch-general"
                    → ChatData.load_messages(channel_id) via ClientManager
                    → Signal<Vec<Message>> updated → ChatView re-renders

User sends message → ClientManager.send_message(channel_id, content)
                   → Message returned → prepend to Signal<Vec<Message>>
                   → ChatView re-renders with new message
```

### Key Design Decisions

- **No global mutable client state** — all client calls go through `ClientManager` which
  dispatches to the right `ClientBackend` based on which account owns the selected server
- **Reactive loading** — use `use_effect` hooks that watch `selected_server` / `selected_channel`
  changes and trigger async data loads
- **Message list is append-only** — new messages prepend; scroll-up loads more via `MessageQuery.before`
- **Demo client is opt-in** — not always loaded. Toggle button creates/destroys the `DemoClient`

---

## 2.5.1 Client Manager Module

> **New file:** `crates/core/src/client_manager.rs`

- [ ] **2.5.1.1** Define `ClientManager` struct:
  ```rust
  pub struct ClientManager {
      pub backends: HashMap<String, Arc<dyn ClientBackend + Send + Sync>>,
      pub demo_active: bool,
  }
  ```
- [ ] **2.5.1.2** `ClientManager::new()` — empty manager, no backends loaded
- [ ] **2.5.1.3** `ClientManager::activate_demo()` — create `DemoClient`, authenticate it,
  add to backends map with key `"demo"`. Mark `demo_active = true`.
- [ ] **2.5.1.4** `ClientManager::deactivate_demo()` — remove demo backend from map,
  set `demo_active = false`. Clear any demo-related data from UI state.
- [ ] **2.5.1.5** `ClientManager::get_backend_for_server(server_id: &str) -> Option<Arc<dyn ClientBackend>>`
  — looks up which backend owns a given server (iterate backends, check `get_servers()` cache)
- [ ] **2.5.1.6** `ClientManager::all_servers() -> Vec<Server>` — aggregates `get_servers()`
  from all active backends
- [ ] **2.5.1.7** Provide `Signal<ClientManager>` in `use_context_provider` in `App` component
- [ ] **2.5.1.8** Register module in `crates/core/src/lib.rs`

---

## 2.5.2 Reactive Data Stores (ChatData)

> **New file:** `crates/core/src/state/chat_data.rs`  
> **Or extend:** `crates/core/src/state/mod.rs`

The chat data store holds the currently loaded data for the active view.

- [ ] **2.5.2.1** Define `ChatData` struct:
  ```rust
  pub struct ChatData {
      pub servers: Vec<Server>,           // all servers from all backends
      pub channels: Vec<Channel>,         // channels for selected server
      pub messages: Vec<Message>,         // messages for selected channel
      pub members: Vec<User>,             // members of selected channel
      pub notifications: Vec<Notification>,
      pub loading: bool,                  // true while fetching data
  }
  ```
- [ ] **2.5.2.2** Provide `Signal<ChatData>` in `App` via `use_context_provider`
- [ ] **2.5.2.3** `load_servers()` — call `ClientManager::all_servers()`, update `Signal<ChatData>.servers`
- [ ] **2.5.2.4** `load_channels(server_id)` — find backend for server, call `get_channels()`,
  update `ChatData.channels`
- [ ] **2.5.2.5** `load_messages(channel_id)` — find backend, call `get_messages()` with default
  query (latest 50), update `ChatData.messages`
- [ ] **2.5.2.6** `load_members(channel_id)` — find backend, call `get_channel_members()`,
  update `ChatData.members`
- [ ] **2.5.2.7** `send_message(channel_id, content)` — find backend, call `send_message()`,
  prepend returned `Message` to `ChatData.messages`
- [ ] **2.5.2.8** `load_more_messages(channel_id, before_id)` — for scroll-up pagination,
  `get_messages()` with `MessageQuery { before: Some(oldest_id), limit: Some(50) }`,
  append to end of `ChatData.messages`
- [ ] **2.5.2.9** `load_notifications()` — aggregate from all backends

---

## 2.5.3 Demo Toggle Button

> **File:** `crates/core/src/ui/server_sidebar.rs`

The user wants a "Demo" button above the settings gear in the server sidebar.
Clicking it activates/deactivates the demo client. Not always active — only when
the user explicitly clicks it.

- [ ] **2.5.3.1** Add "Demo" toggle button above the settings gear (between spacer and ⚙):
  - When demo is inactive: shows "Demo" label/icon, muted appearance
  - When demo is active: shows "Demo" with active/highlighted state, demo server icons appear above
- [ ] **2.5.3.2** On click: if demo inactive → `ClientManager::activate_demo()` + `load_servers()`;
  if demo active → `ClientManager::deactivate_demo()` + clear demo servers from sidebar
- [ ] **2.5.3.3** i18n key: `nav-demo = Demo` / `nav-demo-active = Demo (Active)`
- [ ] **2.5.3.4** Visual indicator: small colored dot or badge on the Demo button when active

---

## 2.5.4 Wire Server Sidebar to Backend Data

> **File:** `crates/core/src/ui/server_sidebar.rs`

Replace the 3 hardcoded server icons with data from `ChatData.servers`.

- [ ] **2.5.4.1** Read `Signal<ChatData>` from context — iterate `chat_data.servers` to render icons
- [ ] **2.5.4.2** Each server icon: first letter of name as fallback, or `<img>` if `icon_url` is `Some`
- [ ] **2.5.4.3** Source badge overlay (top-left corner): small backend icon/emoji based on
  `server.backend` — 🟣 Stoat, 🔵 Matrix, 🟢 Discord, 🟡 Teams, 🧪 Demo
- [ ] **2.5.4.4** Unread badge: show `server.unread_count` if > 0
- [ ] **2.5.4.5** Click handler: set `nav.selected_server = server.id`, `nav.view = View::Server`,
  trigger `load_channels(server.id)` + `load_messages(first_channel_id)`
- [ ] **2.5.4.6** Active selection indicator: highlight bar/pill on the currently selected server

---

## 2.5.5 Wire Channel List to Backend Data

> **File:** `crates/core/src/ui/channel_list.rs`

Replace hardcoded channels with data from `ChatData.channels`.

- [ ] **2.5.5.1** Read `Signal<ChatData>` — iterate `chat_data.channels` to render
- [ ] **2.5.5.2** Group channels by category: match `channel.server_id` categories from the
  `Server` object, render collapsible category headers
- [ ] **2.5.5.3** Channel type icons: `#` for Text, `🔊` for Voice, `📹` for Video
- [ ] **2.5.5.4** Unread indicator: show `channel.unread_count` badge if > 0
- [ ] **2.5.5.5** Active channel highlight: current `nav.selected_channel` gets `.active` class
- [ ] **2.5.5.6** Click handler: set `nav.selected_channel = channel.id`, trigger
  `load_messages(channel.id)` + `load_members(channel.id)`
- [ ] **2.5.5.7** Server header: show server name + backend badge + account info
  (from the `Server` struct)
- [ ] **2.5.5.8** `use_effect` on `nav.selected_server` — when server changes, auto-load channels
  and select the first text channel

---

## 2.5.6 Discord-Style Chat View

> **File:** `crates/core/src/ui/chat_view.rs` (rewrite)

The most substantial change. Replace the 3 hardcoded messages with a proper
Discord-style message renderer.

### 2.5.6.A Message Rendering (Discord Layout)

Each message renders as:

```
┌──────────────────────────────────────────────────┐
│ [Avatar]  Username                    12:34 PM   │
│           Message content goes here, and it can  │
│           span multiple lines with proper word   │
│           wrapping.                              │
│                                                  │
│           [Inline Image Preview]                 │
│           filename.png — 1.2 MB                  │
│                                                  │
│           [😂 3] [❤️ 1]                          │
└──────────────────────────────────────────────────┘
```

**Message grouping:** Consecutive messages from the same author within 7 minutes
are grouped — only the first shows the avatar + username + timestamp header. Subsequent
messages in the group show just the content with a hover-revealed timestamp.

**Date separators:** When the date changes between messages, insert a divider:
```
——————————— February 28, 2026 ———————————
```

- [ ] **2.5.6.1** Read messages from `Signal<ChatData>.messages`
- [ ] **2.5.6.2** **Message component** — `MessageItem` component:
  - Avatar: circular, 40px, shows first letter of username as fallback (colored by user ID hash),
    or `<img>` if `avatar_url` is `Some`
  - Username: bold, colored by role/user-specific color (hash-based for demo)
  - Timestamp: relative if today ("12:34 PM"), absolute if older ("02/28/2026 12:34 PM")
  - Content: render `MessageContent::Text` as paragraph(s), split on `\n` for multi-line
  - Edited indicator: small "(edited)" text if `message.edited`
- [ ] **2.5.6.3** **Message grouping logic** — iterate messages, compare consecutive author IDs
  and timestamps (< 7 min gap = same group). First message in group gets full header;
  subsequent messages get compact layout (no avatar/name, indent content to align)
- [ ] **2.5.6.4** **Date separator component** — when `message[i].timestamp.date() != message[i-1].timestamp.date()`,
  insert a date divider with localized date string (long format from chrono)
- [ ] **2.5.6.5** **Inline image rendering** — for each `Attachment` where `content_type`
  starts with `"image/"`:
  - Render `<img>` with `max-width: 400px`, `max-height: 300px`, `border-radius: 8px`
  - Below image: filename + file size in human-readable format
  - Click to open full-size (future: lightbox)
- [ ] **2.5.6.6** **Non-image attachments** — render as download link with file icon + name + size
- [ ] **2.5.6.7** **Reactions row** — below message content, render each `Reaction` as a pill:
  `[emoji count]` with `.me` highlighted if user has reacted
- [ ] **2.5.6.8** **Scroll to bottom** on new messages (auto-scroll if user is at bottom,
  otherwise show "New messages ↓" floating button)
- [ ] **2.5.6.9** **Scroll-up loading** — detect scroll near top, trigger `load_more_messages()`
  with pagination; show loading spinner at top while fetching
- [ ] **2.5.6.10** **Empty state** — when no messages: centered "No messages yet" with
  wave emoji, invite to start the conversation

### 2.5.6.B Message Input

- [ ] **2.5.6.11** Replace `<input>` with `<textarea>` for multi-line message input
  - Auto-resize height to content (up to ~5 lines max, then scroll)
  - `Shift+Enter` = newline, `Enter` = send
- [ ] **2.5.6.12** Send button: always visible, disabled when input is empty
- [ ] **2.5.6.13** On send: call `ChatData.send_message(channel_id, MessageContent::Text(text))`,
  clear input, scroll to bottom
- [ ] **2.5.6.14** Disable input + show placeholder when no channel is selected

### 2.5.6.C Channel Header

- [ ] **2.5.6.15** Show `# channel-name` from `ChatData.channels` (find by selected_channel ID)
- [ ] **2.5.6.16** Member count next to channel name (from `ChatData.members.len()`)
- [ ] **2.5.6.17** Toggle right sidebar (user list) button

---

## 2.5.7 Wire User Sidebar to Backend Data

> **File:** `crates/core/src/ui/user_sidebar.rs`

Replace hardcoded user list with `ChatData.members`.

- [ ] **2.5.7.1** Read `Signal<ChatData>.members` from context
- [ ] **2.5.7.2** Group by presence: "Online" section, "Idle" section, "Do Not Disturb",
  "Offline" — with count headers like Discord ("ONLINE — 4")
- [ ] **2.5.7.3** User entry: avatar (same style as message avatars), display name,
  presence indicator dot (green/yellow/red/gray), backend badge
- [ ] **2.5.7.4** `use_effect` on `nav.selected_channel` — when channel changes, auto-load members

---

## 2.5.8 Wire Notifications View

> **File:** `crates/core/src/ui/notifications.rs`

Replace hardcoded notifications with `ChatData.notifications`.

- [ ] **2.5.8.1** Read `Signal<ChatData>.notifications` from context
- [ ] **2.5.8.2** Render each notification with: source icon (backend badge), title, preview text,
  relative timestamp ("2 hours ago"), read/unread styling
- [ ] **2.5.8.3** Mark as read on click (update notification state)
- [ ] **2.5.8.4** "Mark all as read" button at top
- [ ] **2.5.8.5** Empty state when no notifications

---

## 2.5.9 DMs / Friends View Wiring

> **File:** `crates/core/src/ui/` — either extend `notifications.rs` or create
> `crates/core/src/ui/dms_view.rs`

Currently the DMs/Friends view doesn't exist beyond the navigation entry.

- [ ] **2.5.9.1** Create `DmsFriendsView` component
- [ ] **2.5.9.2** Load DM channels from backends via `ClientManager` → `get_dm_channels()`
- [ ] **2.5.9.3** Render DM list: user avatar, name, last message preview, timestamp, backend badge
- [ ] **2.5.9.4** Click a DM → load messages for that DM channel in the chat view
- [ ] **2.5.9.5** Friends tab: show friend list from `get_friends()`, grouped by online status
- [ ] **2.5.9.6** Search bar at top to filter DMs/friends

---

## 2.5.10 Demo Data Enhancements

> **File:** `clients/demo/src/data.rs`

Enhance the demo data to showcase the full UI.

- [ ] **2.5.10.1** Add image attachments to some demo messages:
  - Use placeholder image URLs (picsum.photos, placekitten, or bundled demo images in assets)
  - Mix of small and large images
  - Include a non-image attachment (e.g., `document.pdf`)
- [ ] **2.5.10.2** Add multi-line messages: code blocks, multi-paragraph messages, emoji-heavy messages
- [ ] **2.5.10.3** Add edited messages: set `edited: true` on some messages
- [ ] **2.5.10.4** Add reactions to some messages: `["😂", "❤️", "👍"]` with varied counts
- [ ] **2.5.10.5** Add more realistic timestamps: spread messages across several days to
  trigger date separators
- [ ] **2.5.10.6** Add demo avatar URLs (use UI Faces or DiceBear API for generated avatars,
  or simple colored-initial SVGs via a data URI generator)

---

## 2.5.11 Demo Event Stream (Fake Real-Time)

> **File:** `clients/demo/src/lib.rs`

Replace `stream::empty()` with a fake event stream.

- [ ] **2.5.11.1** Implement `event_stream()` that produces `ClientEvent`s at intervals:
  - Every 15-30 seconds: `MessageReceived` with a random new message on a random channel
  - Every 60 seconds: `PresenceChanged` for a random user (toggle online ↔ idle ↔ offline)
  - Every 45 seconds: `TypingStarted` for a random channel (auto-clears after 3 seconds)
- [ ] **2.5.11.2** Wire event stream consumer in `App` or `ClientManager`:
  - `spawn()` a task that reads from `event_stream()`, dispatches events to `ChatData`
  - New messages on the currently viewed channel → prepend to messages list
  - Presence changes → update member list if visible
- [ ] **2.5.11.3** Typing indicator in chat view: "Alice is typing..." bar above message input

---

## 2.5.12 CSS & Theme Enhancements for Chat

> **File:** `crates/core/assets/tailwind.css` and/or scoped component styles

The chat view needs Discord-style styling that works with all 3 theme presets.

- [ ] **2.5.12.1** Message layout: flexbox with avatar fixed-width left column, content
  expanding right. Proper spacing between message groups.
- [ ] **2.5.12.2** Avatar styling: 40px circular, background color from user-hash, white text
  centered (for initial-based avatars)
- [ ] **2.5.12.3** Username colors: deterministic color from user ID hash (6-8 distinct colors
  from theme palette) — Discord uses role colors; we use hash-based for demo
- [ ] **2.5.12.4** Timestamp styling: small, muted text, right-aligned on same line as username
- [ ] **2.5.12.5** Grouped message compact spacing: messages in the same group have minimal
  vertical gap (2-4px); new groups have larger gap (16px)
- [ ] **2.5.12.6** Image styling: rounded corners, max-width 400px, hover shadow, cursor pointer
- [ ] **2.5.12.7** Reaction pill styling: rounded, border, emoji + count, `.me` gets accent highlight
- [ ] **2.5.12.8** Date separator: horizontal rule with centered date text (translucent background)
- [ ] **2.5.12.9** Message hover: subtle background highlight on hover (shows timestamp for grouped msgs)
- [ ] **2.5.12.10** Message input area: border-top separator, auto-resize textarea, send button aligned right
- [ ] **2.5.12.11** Scrollbar styling: thin, themed scrollbar for message list (webkit + Firefox)
- [ ] **2.5.12.12** Verify all 3 theme presets (neutral-dark, purple, red) look good with new chat styling

---

## 2.5.13 i18n — New Translation Keys

> **Files:** `locales/{en,de,fr,es}/main.ftl`

- [ ] **2.5.13.1** Add keys for demo toggle:
  - `nav-demo = Demo`
  - `nav-demo-active = Demo (Active)`
  - `nav-demo-tooltip = Toggle demo mode with sample data`
- [ ] **2.5.13.2** Add keys for chat view:
  - `chat-no-messages = No messages yet`
  - `chat-start-conversation = Start the conversation!`
  - `chat-new-messages = New messages`
  - `chat-loading = Loading messages...`
  - `chat-typing = { $user } is typing...`
  - `chat-typing-multiple = { $count } people are typing...`
  - `chat-message-edited = (edited)`
  - `chat-members = { $count } Members`
- [ ] **2.5.13.3** Add keys for user sidebar:
  - `users-online = Online — { $count }`
  - `users-idle = Idle — { $count }`
  - `users-dnd = Do Not Disturb — { $count }`
  - `users-offline = Offline — { $count }`
- [ ] **2.5.13.4** Add keys for notifications:
  - `notifications-empty = No new notifications`
  - `notifications-mark-all-read = Mark All as Read`
- [ ] **2.5.13.5** Add keys for DMs view:
  - `dms-search = Search conversations...`
  - `dms-no-conversations = No conversations yet`
  - `dms-friends = Friends`
- [ ] **2.5.13.6** Add all above in DE, FR, ES translations

---

## Execution Order (Recommended)

Work in this order to get incremental visible progress:

1. **2.5.1** Client Manager — foundational, everything depends on it
2. **2.5.2** Chat Data stores — data layer for all components
3. **2.5.3** Demo toggle button — activates the demo client
4. **2.5.4** Server sidebar wiring — servers appear from demo
5. **2.5.5** Channel list wiring — channels load per server
6. **2.5.6** Chat view rewrite — messages render from backend (biggest task)
7. **2.5.6.B** Message input — type and send works
8. **2.5.7** User sidebar wiring — members load per channel
9. **2.5.10** Demo data enhancements — images, reactions, multi-line
10. **2.5.12** CSS polish — Discord-style look
11. **2.5.8** Notifications wiring
12. **2.5.9** DMs/Friends view
13. **2.5.11** Demo event stream — fake real-time updates
14. **2.5.13** i18n — translations for all new strings

**Milestone checkpoints:**
- After step 5: Can click Demo → see servers → click server → see channels → click channel ✓
- After step 7: Can type a message and see it appear in chat ✓
- After step 10: Messages look like Discord with images and reactions ✓
- After step 14: Full feature-complete demo experience ✓

---

## Completion Criteria

- [ ] Demo button above settings gear toggles demo mode on/off
- [ ] Server sidebar populated by demo client servers (with backend badges)
- [ ] Clicking a server loads its channels in the channel list
- [ ] Clicking a channel loads its messages in Discord-style rendering
- [ ] Messages show: avatar, username (colored), timestamp, content, images inline, reactions
- [ ] Consecutive same-author messages are grouped (compact layout)
- [ ] Date separators appear between different-day messages
- [ ] Typing a message and pressing Enter/Send makes it appear in the chat
- [ ] User sidebar shows channel members grouped by online status
- [ ] Notifications view populated from demo client
- [ ] DMs/Friends view shows demo DM conversations
- [ ] Demo event stream produces periodic new messages and presence changes
- [ ] All 3 theme presets render the chat nicely
- [ ] i18n: all new strings have EN/DE/FR/ES translations
- [ ] `cargo cranky --workspace` — zero warnings
- [ ] `cargo check -p poly-web --target wasm32-unknown-unknown` — WASM clean
- [ ] `dx serve --hotpatch` hot-reload still works

---

## Technical Notes

### ClientBackend trait object safety

`ClientBackend` uses `async fn` in trait → requires `async-trait` or trait object workaround.
Check if `poly-client` already has `#[async_trait]` or if we need `Box<dyn ClientBackend>`
with a manual vtable. Dioxus 0.7 works well with `spawn()` + `async` blocks reading from
`Arc<dyn ClientBackend>`.

### WASM compatibility

All client backends are behind feature flags. The `demo` feature is default-enabled.
`DemoClient` uses only `chrono` + `rand` — both WASM-safe. No network calls, no filesystem.
Ensure `ClientManager` and `ChatData` have no platform-specific dependencies.

### Hot-reload

`ChatData` and `ClientManager` are reactive Dioxus signals. Adding new signals doesn't
break hot-reload. However, changing the `AppState` struct shape may require an app restart.
Test after major state changes.

### Performance

Demo client returns data synchronously (all in-memory). For real backends in Phase 3,
consider:
- Debouncing rapid channel switches (cancel pending load if user clicks another channel)
- Virtual scrolling for the message list (only render visible messages)
- Caching: store loaded channels/messages in `ChatData` by channel ID to avoid re-fetching
