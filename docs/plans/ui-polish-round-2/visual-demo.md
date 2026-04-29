# Demo Backend — Visual Audit Report

**Accounts:** Cat (demo), Dog (demo), Platypus (demo_forum)
**Date:** 2026-04-21
**Screenshots:** `screenshots/demo/`

---

## Cat (demo)

### Layout / Structure
- Standard 3-column layout: account rail (72px) | server/DM nav (72px) | channel list (220px) | content area
- Account bar at bottom correctly shows Cat (demo) with Online status, mic/headset/settings icons
- Second nav shows server icons (Poly Development, Gaming Lounge, Music Enthusiasts) as colored squares with letter initials, then DM, Friends, Notifications icons at top

### Chat / Messages (cat-03-channel.png)
- Message list renders correctly with avatar, username, timestamp, and message body
- Code blocks render with dark background — `Err(ClientError::AuthFailed(message.to_string()))` styled inline
- Reaction button (👍) appears after messages
- "NEW" badge on unread divider is clearly visible
- Message input bar at bottom with emoji picker and send button works visually
- "Message Alice" placeholder text present

### DMs (cat-04-dms.png)
- DM list shows: New Conversation, Saved Messages, then individual contacts (Alice, Dog (demo), Bob, Iris, Charlie, Diana, Jack, Eve) and group chats (Poly Core Team (4), Rust Study Group (3), Weekend Warriors (3), Midnight Jams (2))
- Unread badge counts (1) visible on Alice, Dog (demo), Bob, Iris
- Right panel shows "Select a conversation" when no conversation selected — shows a chat bubble icon and text

### Friends (cat-05-friends.png)
- Friends panel shows "People" with Friends/Ignored/Blocked Users tabs
- No friends listed — "No friends found" placeholder
- Search box present and focused

### Notifications (cat-06-notifications.png)
- Notifications list shows categorized tabs: All notifications (0), Mentions (0), Other (0)
- "No new notifications" placeholder in right panel
- Clean empty state

### Settings (cat-07-settings.png)
- Per-account settings modal opens from ⚙ gear button in account bar
- Shows: Notifications section (People I know start streaming, Friends join voice channels, etc.) and Content & Social section
- Settings are labeled DEMO-CAT at the top

### Issues
- Friends panel shows generic "No friends found" — no invite/add button visible; demo accounts can't add friends to themselves so this is expected
- Empty state right panel for DMs shows a chat bubble icon (good)

---

## Dog (demo)

### Layout / Structure
- Same 3-column layout as Cat
- Account bar shows "Dog (demo) / Online"

### Landing (dog-01-landing.png)
- After initial click, app showed "App not responding" overlay with Reload button — a boot hang watchdog triggered
- Required page_reload to recover — this is a **UX bug**: boot hang watchdog fires too eagerly on demo accounts with many loaded conversations

### Server/Channel (dog-02-server.png, dog-03-channel.png)
- Server list shows same demo servers as Cat
- Channel view shows conversations correctly

### DMs (dog-04-dms.png)
- Similar DM list to Cat; Dog (demo) appears as a contact in other demo accounts

### Friends (dog-05-friends.png)
- Same empty "No friends found" state as Cat

### Notifications (dog-06-notifications.png)
- No notifications, same empty state

### Issues
- **Boot hang watchdog fires on Dog account switch** — "App not responding" overlay with Reload button appears ~18s after clicking Dog avatar; the overlay button doesn't work as a standard button (requires page_reload)

---

## Platypus (demo_forum)

### Landing (platypus-01-landing.png)
- Platypus uses demo_forum backend — second nav shows rust_lang, linux, programming community icons
- Second nav shows forum community icons (rust_lang, linux, programming) with badge counts

### Server/Channel (platypus-02-server.png, platypus-03-channel.png)
- Forum-style channel list with community names
- Channel view shows forum posts with title, author, post content, reaction counts

### DMs (platypus-04-dms.png)
- DM list similar to standard demo accounts

### Friends / Notifications / Settings
- Same patterns as Cat/Dog demo accounts

### Issues
- demo_forum community icons in second nav show numeric badge counts (7, 4, 2 etc.) which appear to be unread counts — visually distinct and informative

---

## Cross-account Issues (Demo Backend)

1. **Boot hang watchdog triggers on Dog account switch** — timing too aggressive for accounts with many contacts/conversations
2. **Empty state: right panel "Select a conversation"** shows a generic placeholder with chat bubble icon — functional but minimal; could show recent conversations
3. **Friends panel "No friends found"** has no visible way to add friends — the Add Friend button, if any, is absent from the demo account settings
4. **Per-account settings (⚙ gear)** opens an inline panel, not a modal — the second nav is replaced by account settings which is slightly confusing

---

## Console Errors
No critical console errors observed during demo backend navigation.

---

## Phase-5 Code Audit (2026-04-27)

### Status: pass

### Account Login
All three clients use hardcoded in-memory sessions — `authenticate()` returns `Ok(session)` immediately. Cannot fail under normal conditions.

### Overview Page
- Cat/Dog (DemoClient/DemoClient2): `get_account_overview_view` returns `NotSupported` — host falls back to server-list grid.
- Platypus (DemoClient3): returns `ViewKind::CardGrid` with `plugin-demo-forum-overview-title`.

### Messaging
All three: `send_message`, `send_reply_message`, `send_typing` all return `Ok`. `search_messages` is implemented (the only backends in the codebase with this). Cat+Dog share an in-memory message store — cross-account chat functional since commit `1b35f0bc` (arena server + mutual friends seed).

### 14 New Backend Ops (commit 5b142e67)
All 14 return `Ok(())` as in-memory stubs across all three demo clients.

### Context-Menu
`get_context_menu_items(Server, _)` returns `"regenerate-demo-data"` item. All other targets return empty.

### Known Gaps
- `get_pinned_messages` not overridden — trait default `NotSupported`.
- Boot hang watchdog fires on Dog account switch.
- Friends panel "No friends found" may be stale — Cat+Dog seeded as friends in `1b35f0bc`.
