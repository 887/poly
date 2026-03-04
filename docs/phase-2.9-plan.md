# Phase 2.9 — Dual Sidebar: Accounts & Favorites / Account Server Bar

> Created: 2026-03-03
> Status: Complete

---

## Problem Statement

The current single `ServerSidebar` mixes concerns:
- Account switching (DMs icon, Notifications icon)
- Favorited servers from all backends
- App-level controls (Demo toggle, Settings)

With multiple backends/accounts this becomes confusing:
1. **No visual account identity** — there's no way to "jump to an account" or see which account you're inside
2. **DMs and Notifications are global** — but they should be per-account when you're viewing an account
3. **No quick way to see all servers for one account** — favorited servers from all accounts are mixed together

---

## Solution: Two Sidebar Bars

### Bar 1 — **Favorites Bar** (leftmost, always visible)

A narrow icon-only column showing:
1. **Account icons** (top) — one per active backend account, click to switch into that account
   - The currently active account gets an "active" indicator
   - Show unread badge (total DMs + friend requests + mentions for that account)
2. **Separator line**
3. **Favorited server icons** — servers from ANY account the user has favorited (cross-account)
   - Clicking a favorited server opens its route AND focuses it in Bar 2 (the account bar auto-switches)
4. **Spacer**
5. **Demo toggle** (🧪)
6. **App Settings** (⚙) — app-level settings

### Bar 2 — **Account Server Bar** (second column, shown when an account is active)

A second narrow icon-only column showing everything for the *currently active account*:
1. **DMs/Friends button** — DMs home for THIS account
2. **Notifications button** — notifications for THIS account (with unread badge)
3. **Separator**
4. **All servers** for this account — not just favorites, ALL joined servers
5. **Spacer**
6. **Account Settings** (⚙) — settings for this specific account

### When Bar 2 is NOT shown

- App Settings route (`/settings`) — no account context, only Bar 1
- When no account is active (fresh install) — only Bar 1

### Clicking a Favorited Server (Bar 1)

When the user clicks a favorited server in Bar 1:
1. The router navigates to `/:backend/:instance_id/:account_id/channels/:server_id`
2. `sync_route_to_app_state` sets `active_backend` + `active_instance_id` + `active_account_id`
3. Bar 2 appears showing that account's servers, with this server highlighted
4. The channel list shows server channels as normal

This works perfectly with our routing since every URL is `/:backend/:instance_id/:account_id/...`.

### Drag & Drop (Future)

Having two bars enables drag-and-drop from Bar 2 (account servers) to Bar 1 (favorites) —
a natural "star this server" gesture. Not implementing drag-and-drop now, but the architecture
supports it cleanly.

---

## Implementation Plan

### A. Rename `ServerSidebar` → `FavoritesBar`

The existing `server_sidebar.rs` becomes the Favorites Bar (Bar 1).
- Remove DMs and Notifications buttons (those move to Bar 2)
- Add **Account icons** at the top (before separator)
- Keep favorited server icons
- Keep Demo toggle and App Settings at bottom

### B. Create `AccountServerBar` component

New file: `account_server_bar.rs`
- Shows only when `active_account_id` is set
- DMs/Friends button (account-scoped)
- Notifications button (account-scoped, with unread badge)
- Separator
- All servers for the active account (not just favorites)
- Spacer
- Account Settings gear at bottom

### C. Update `MainLayout`

```
main-layout-body:
  NavBar (native only)
  FavoritesBar (Bar 1 — always)
  AccountServerBar (Bar 2 — when account active)
  Outlet (channel list + chat + user sidebar)
```

### D. Per-Account Notification Badges

Each account icon in Bar 1 shows a total unread count:
- Unread DMs
- Pending friend requests
- Unread mentions

This count is computed from `ChatData` filtered by account.

### E. Fix Desktop Empty Page

The desktop client starts at `/` with memory history. The `on_update` redirect
should fire, but there may be a timing issue with `demo_active` not being set yet.
Fix: ensure the root redirect always fires immediately.

### F. CSS Updates

- `.favorites-bar` — same width as current server-sidebar (72px)
- `.account-server-bar` — same 72px width, slightly different background shade
- Both bars sit side by side in `main-layout-body` (flex row)
- Total sidebar width when account active: 144px (72 + 72)

### G. i18n Keys

Add new locale keys for:
- Account icon tooltip
- Account settings label
- "All servers" vs "Favorites" labels

---

## Work Checklist

- [x] **A**: Convert `ServerSidebar` into `FavoritesBar` (remove DMs/Notifs, add account icons)
- [x] **B**: Create `AccountServerBar` component
- [x] **C**: Update `MainLayout` to render both bars
- [x] **D**: Per-account notification badge computation
- [x] **E**: Fix desktop empty page on launch (Root component `use_effect` redirect + auto-navigate after demo toggle)
- [x] **F**: CSS for dual sidebar layout
- [x] **G**: i18n locale keys for new UI elements

### Session 2 Polish (2026-03-03)

- [x] **H**: Fix `/settings` route clearing server/account context (app settings gear left server open)
- [x] **I**: Add `Route::AccountSettingsRoute` at `/:backend/:instance_id/:account_id/settings` (account gear in Bar 2 navigates here)
- [x] **J**: `Session.icon_emoji: Option<String>` field for per-account visual icon
- [x] **K**: `DemoClient2` — second demo backend (🐶 dog account) with 4 servers (Open Source Hub, Book Club, Cooking Corner, Fitness Crew)
- [x] **L**: `ClientManager.sessions: HashMap<String, Session>` — store sessions per account for UI lookup
- [x] **M**: `ChatData.account_sessions: HashMap<String, Session>` — reactive store for session data in components
- [x] **N**: `AccountIcon` shows `icon_emoji` (🐱/🐶) instead of first letter of account ID
- [x] **O**: `FavoriteServerIcon` shows account's `icon_emoji` as source badge instead of generic backend emoji
- [x] **P**: `--favorites-bar-bg` and `--account-bar-bg` CSS custom properties in all 10 theme CSS files
- [x] **Q**: Distinct default colors for Bar 1 and Bar 2 via new CSS vars
- [x] **R**: Bar color pickers added to Theme Settings UI (`ThemeColorCustomizer`)
- [x] **S**: i18n keys `color-favorites-bar` / `color-account-bar` in all 4 locale files (en, de, fr, es)
- [x] Build verification: `cargo check --workspace`
- [x] WASM check: `cargo check -p poly-web --target wasm32-unknown-unknown`
- [x] `cargo cranky --workspace` — zero warnings/errors
- [x] `cargo fmt --all`
- [x] Visual verification via DevTools MCP — 🐱/🐶 icons appear, source badges correct, Theme settings shows new color pickers

### Deferred

- [ ] **Drag & Drop**: Bar2 → Bar1 (favorite a server by dragging) — planned for Phase 2.10

## Post-Phase Addendum — Federated Route Scope (2026-03-04)

Routing moved from backend+account to backend+instance+account to support multiple
accounts on the same backend across different federated instances.

- Account-scoped routes are now `/:backend/:instance_id/:account_id/...`
- Demo account IDs were renamed from `demo` / `demo2` to `demo-cat` / `demo-dog`
- Demo uses `instance_id = "demo"` for both accounts
- Sidebar/account context syncing now includes `active_instance_id`

---

## What Is NOT Changing

- Route structure (`/:backend/:instance_id/:account_id/...`) — stays as-is
- `ChannelList`, `ChatView`, `UserSidebar` — no changes
- `AccountBar` / `AccountSwitcher` at bottom of channel list — stays
- Storage / database — no schema changes
- `ClientManager` API — already supports multi-account

---

## Visual Mockup

```
┌───────────────────────────────────────────────────────────────────────────┐
│                                                                           │
│ ┌──────┐ ┌──────┐ ┌────────────┐ ┌────────────────────────┐ ┌──────────┐│
│ │  B1  │ │  B2  │ │  Channel   │ │  Chat / Content        │ │  Users   ││
│ │      │ │      │ │  List      │ │                        │ │  Sidebar ││
│ │ 🧪 3 │ │ 💬   │ │            │ │                        │ │          ││
│ │      │ │ 🔔 2 │ │  #general  │ │  Messages...           │ │  @user1  ││
│ │ ──── │ │      │ │  #random   │ │                        │ │  @user2  ││
│ │      │ │ ──── │ │  🔊voice   │ │                        │ │          ││
│ │ ★srv │ │      │ │            │ │                        │ │          ││
│ │ ★srv │ │ srv1 │ │            │ │                        │ │          ││
│ │ ★srv │ │ srv2 │ │            │ │                        │ │          ││
│ │      │ │ srv3 │ │            │ │                        │ │          ││
│ │      │ │      │ │            │ │                        │ │          ││
│ │      │ │      │ │            │ │                        │ │          ││
│ │ 🧪   │ │      │ │            │ │                        │ │          ││
│ │ ⚙    │ │ ⚙   │ │            │ │                        │ │          ││
│ └──────┘ └──────┘ └────────────┘ └────────────────────────┘ └──────────┘│
│                                                                           │
│  B1 = Favorites Bar (accounts + favorited servers + demo + app settings)  │
│  B2 = Account Server Bar (DMs + Notifs + all servers + account settings)  │
└───────────────────────────────────────────────────────────────────────────┘
```
