# Phase 2.10 — Server Drag & Drop, Context Menus, Server Settings

> Created: 2026-03-03
> Status: In Progress

---

## Problem Statement

Three problems from user feedback after Phase 2.9:

1. **Drag & Drop broken** — Dragging a server from AccountServerBar to FavoritesBar does
   not work because the dragover event doesn't propagate correctly, and reordering within
   either bar is completely absent.
2. **No right-click menus** — Server icons in both Bar 1 and Bar 2 need a context menu
   matching the Discord-style menu in the screenshot.
3. **No server settings** — The "Notification Settings" context menu entry (and others)
   need a dedicated per-server settings page akin to account settings.

---

## Feature Scope

### A. Drag & Drop — complete rewrite

**FavoritesBar (Bar 1):**
- Account icons: drag to reorder within the accounts section only
- Favorite server icons: drag to reorder within the favorites section only
- Accept drops of server icons from AccountServerBar (Bar 2) → add to favorites

**AccountServerBar (Bar 2):**
- Server icons: drag to reorder (order persisted per account in `ChatData`)
- Drag to Bar 1 drops add to favorites

**Rules:**
- Accounts can only be reordered among accounts
- Favorites can only be reordered among favorites  
- AccountServerBar servers can be reordered within AccountServerBar
- AccountServerBar servers can be dropped onto FavoritesBar to favorite them

**Implementation approach:**
- Add `DragSource` enum + `drag_source: DragSource` to `ChatData`
- Add `drag_over_id: Option<String>` to `ChatData` (hovered drop target)
- Add `account_server_order: HashMap<String, Vec<String>>` to `ChatData` for per-account ordering
- Use `ondragover` with `evt.prevent_default()` on each item to track hover
- On `ondrop`: reorder or add-to-favorites depending on `drag_source`
- Show visual insert indicator via CSS class on the hover target

### B. Right-click Context Menu

Inline floating div (not native browser menu) rendered at the `MainLayout` level so
it is never clipped by sidebar `overflow: hidden`.

State: `AppState.context_menu: Option<ContextMenuState>` with x/y + server info.

Menu items (per Discord screenshot):
| Item | Icon | Action |
|---|---|---|
| Mark as Read | | Mark all channels in server as read |
| Invite to Server | | Copy invite URL (no-op for demo) |
| — separator — | | |
| Unmute Server | | Toggle server mute |
| Notification Settings | → | Navigate to **Server Settings › Notifications** |
| Hide Muted Channels | ☐ | Toggle |
| Show All Channels | ☑ | Toggle |
| — separator — | | |
| Privacy Settings | | Navigate to **Server Settings › Privacy** |
| Edit Per-server Profile | | Navigate to **Server Settings › Profile** |
| — separator — | | |
| Leave Server | red | Inline confirm dialog within context menu |
| — separator — | | |
| Copy Server ID | 🪪 | Copy to clipboard |

**Context menu closes:** clicking anywhere outside the menu div.

Global `onclick` on the `MainLayout` root div dismisses the menu.

### C. Server Settings Page

Route: `/:backend/:account_id/servers/:server_id/settings`

Module path: `crates/core/src/ui/account/server/settings/`

| File | Component | Contents |
|---|---|---|
| `mod.rs` | `ServerSettingsPage` | Nav sidebar + content outlet, identical layout to `AccountSettingsPage` |
| `general.rs` | `ServerGeneralSettings` | Per-server profile, leave server button + inline confirm |
| `notifications.rs` | `ServerNotificationsSettings` | All Messages / Only @mentions / Nothing, suppress @everyone, mobile push |
| `profile.rs` | `ServerProfileSettings` | Display name, avatar override for this server |

The server settings page shows the server name in the header.

Nav sidebar items (in order):
1. Notifications (default landing)
2. Profile
3. General (contains leave server at the bottom)

### D. Leave Server — Inline Confirm

In `ServerGeneralSettings`, a "Leave Server" button at the bottom shows an inline
confirm widget (not `window.confirm()`):

```
┌─ Leave "{server_name}"? ─────────────────────────┐
│ You won't be able to rejoin unless re-invited.   │
│                                                   │
│  [Cancel]          [Leave Server]←(danger button) │
└───────────────────────────────────────────────────┘
```

On confirm: navigate back to account DMs, remove server from `chat_data.servers`
and from favorites.

---

## Work Checklist

### State Changes
- [x] **S1**: Add `DragSource` enum to `chat_data.rs`
- [x] **S2**: Add `drag_source: DragSource` to `ChatData`
- [x] **S3**: Add `drag_over_id: Option<String>` to `ChatData`
- [x] **S4**: Add `account_server_order: HashMap<String, Vec<String>>` to `ChatData`
- [x] **S5**: Add `ContextMenuState` struct + `context_menu: Option<ContextMenuState>` to `AppState`

### Routes
- [x] **R1**: Add `ServerSettingsRoute { backend, account_id, server_id }` to route enum
- [x] **R2**: Add `sync_route_to_app_state` arm for `ServerSettingsRoute`
- [x] **R3**: Add `fn ServerSettingsRoute(...)` component that renders `ServerSettingsPage`

### Server Settings Module
- [x] **SS1**: Create `account/server/mod.rs`
- [x] **SS2**: Create `account/server/settings/mod.rs` — `ServerSettingsPage`
- [x] **SS3**: Create `account/server/settings/notifications.rs` — `ServerNotificationsSettings`
- [x] **SS4**: Create `account/server/settings/profile.rs` — `ServerProfileSettings`
- [x] **SS5**: Create `account/server/settings/general.rs` — `ServerGeneralSettings` + leave confirm
- [x] **SS6**: Add `pub mod server;` to `account/mod.rs`

### Context Menu
- [x] **CM1**: Create `account/server/context_menu.rs` — `ServerContextMenu` component
- [x] **CM2**: Add `oncontextmenu` handler to `FavoriteServerIcon` in `favorites_sidebar.rs`
- [x] **CM3**: Add `oncontextmenu` handler to server icons in `account_server_bar.rs`
- [x] **CM4**: Render `ServerContextMenu` in `main_layout.rs` (root level)
- [x] **CM5**: Add global `onclick` to `MainLayout` div to dismiss context menu

### Drag & Drop
- [x] **DD1**: Rewrite `FavoriteServerIcon` drag logic (reorder + receive from Bar 2)
- [ ] **DD2**: Add account icon drag reorder in `FavoritesBar` — deferred to later phase
- [x] **DD3**: Rewrite server icon drag logic in `AccountServerBar` (reorder + drag out to Bar 1)
- [x] **DD4**: Use `drag_over_id` for visual insert indicator via CSS class

### CSS
- [x] **CSS1**: `.drag-over-target` style — insertion line indicator
- [x] **CSS2**: `.context-menu` / `.context-menu-item` / `.context-menu-separator` styles
- [x] **CSS3**: `.server-settings-page` — reuse `.settings-page` (already works)
- [x] **CSS4**: `.leave-server-confirm` — inline confirm widget

### i18n
- [x] **I1**: Add locale keys for server settings nav labels
- [x] **I2**: Add locale keys for leave server confirm dialog (uses `t_args` for parameterized title)
- [x] **I3**: Add locale keys for context menu items

### Verification
- [x] `cargo check --workspace` — no errors
- [x] `cargo cranky --workspace` — zero warnings
- [x] `cargo check -p poly-web --target wasm32-unknown-unknown` — WASM clean
- [x] `cargo fmt --all`
- [x] Visual verification via Desktop DevTools MCP

---

## Session Summary (Phase 2.10 complete)

**What was built:**
- Full server context menu (10 items, position:fixed overlay, backdrop dismiss, checkbox toggles, danger-styled Leave Server)
- Context menu on BOTH Bar 1 (favorited servers) and Bar 2 (account server bar) icons
- Fixed drag-and-drop: Bar 1 favorites now reorder correctly via positional per-item drops
- Bar 2 servers reorder within the bar using `account_server_order` in ChatData
- Bar 2 → Bar 1 positional drops (insert before the hovered item, not just append to end)
- `DragSource` enum to distinguish FavoriteServer vs AccountServer vs AccountIcon drags
- New route: `/:backend/:account_id/servers/:server_id/settings` → `ServerSettingsPage`
- `ServerSettingsPage`: 3-section layout (Notifications, Profile, General) with search bar
- `ServerNotificationsSettings`: radio level selectors + 5 toggle rows
- `ServerProfileSettings`: nickname field + save button (in-memory)
- `ServerGeneralSettings`: server info + Danger Zone with inline leave-server confirm
- `LeaveServerConfirm`: inline RSX component (no JS confirm), uses `t_args` for parameterized FTL message, removes server from all ChatData collections, navigates to DMs
- CSS: context menu styles, drag-over-target indicator, notification level radioes, leave-server confirm, settings danger zone, monospace badge, saved badge

**Known deferred items:**
- Account icon reordering (DD2) — account icons in Bar 1 still can't be reordered
- Server settings are in-memory only — persistence to SurrealDB in a later phase
- `Notification Settings` from context menu lands on the Notifications tab (correct behavior)
- `Leave Server` from context menu navigates to General tab — user clicks Leave Server to reveal inline confirm

## Deferred to Phase 2.11

- Server-level notification persistence to storage
- Per-server profile photo upload
- Invite link generation
- Privacy settings (stub only for now)
