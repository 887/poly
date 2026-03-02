# Poly — Routing & Notification Overhaul Plan

> Created: 2026-03-03  
> Status: **COMPLETE** ✅

---

## Problem Statement

The router was modelled on Discord's single-account URL scheme. Poly is a
**multi-account, multi-backend** client, so three things are wrong today:

1. **No account context in URLs.** `/channels/server-gaming` is ambiguous —
   which of the user's accounts owns that server?

2. **No backend context in URLs.** Discord and Teams may look fundamentally
   different even for the "same" view. We want the backend type encoded in the
   URL so we can eventually render backend-specific layouts/features while
   sharing common components.

3. **Notification settings are global.** Each account has its own preferences;
   global settings make no sense for a multi-account client.

---

## New Route Structure

```
/                                                    → redirect (see Root Redirect below)

/:backend/:account_id/dms                            → DM home for this account
/:backend/:account_id/dms/:channel_id                → DM or group conversation
/:backend/:account_id/friends                        → Friends list for this account
/:backend/:account_id/channels/:server_id            → Server home (auto first channel)
/:backend/:account_id/channels/:server_id/:channel_id → Specific server channel

/notifications                                       → Aggregated cross-account feed
/settings                                            → App-level settings (global)
```

### Backend Slug Values

| `BackendType` | URL segment |
|---|---|
| `Demo`    | `demo` |
| `Stoat`   | `stoat` |
| `Matrix`  | `matrix` |
| `Discord` | `discord` |
| `Teams`   | `teams` |

### Account ID Convention

| Account        | `account_id` in URL |
|---|---|
| Demo client    | `demo` |
| Real accounts  | The exact string key used in `ClientManager.backends` |

Demo URLs: `/demo/demo/dms`, `/demo/demo/channels/server-gaming/chan-mc`, etc.

### Root Redirect Logic

```
if demo_active && no_other_accounts → /demo/demo/dms
if any_real_account   → /:backend/:last_account_id/dms
else (fresh install)  → /settings  (Accounts tab)
```

Unknown `account_id` in a URL → redirect to `/` which re-evaluates above.

---

## Implementation Phases

### Phase B — AppState / NavigationState  ← START HERE

**File**: `crates/core/src/state/mod.rs`

- [ ] Add `active_account_id: Option<String>` to `NavigationState`
- [ ] Add `active_backend: Option<BackendType>` to `NavigationState`
- [ ] Remove the now-dead `nav_history`, `nav_history_index`, `push_nav_history()`,
      `nav_back()`, `nav_forward()`, `can_go_back()`, `can_go_forward()` — the
      Dioxus Router owns history now.

```rust
pub struct NavigationState {
    pub view: View,
    pub active_account_id: Option<String>,   // NEW
    pub active_backend: Option<BackendType>, // NEW
    pub selected_server: Option<String>,
    pub selected_channel: Option<String>,
    pub right_sidebar_visible: bool,
}
```

### Phase A — Route Enum Rewrite

**Files**: `crates/core/src/ui/routes.rs`, `crates/core/src/ui/mod.rs`

New `Route` enum (sketch):

```rust
#[derive(Routable, Clone, PartialEq, Debug)]
pub enum Route {
    #[layout(MainLayout)]

        // Root redirect
        #[route("/")]
        Root,

        // ── Account-scoped views ──
        #[layout(DmsLayout)]
            #[route("/:backend/:account_id/dms")]
            DmsHome { backend: String, account_id: String },
            #[route("/:backend/:account_id/dms/:channel_id")]
            DmChat { backend: String, account_id: String, channel_id: String },
        #[end_layout]

        #[layout(ServerLayout)]
            #[route("/:backend/:account_id/channels/:server_id")]
            ServerHome { backend: String, account_id: String, server_id: String },
            #[route("/:backend/:account_id/channels/:server_id/:channel_id")]
            ServerChat { backend: String, account_id: String, server_id: String, channel_id: String },
        #[end_layout]

        #[route("/:backend/:account_id/friends")]
        FriendsRoute { backend: String, account_id: String },

        // ── App-level (not account-scoped) ──
        #[route("/notifications")]
        NotificationsRoute,
        #[route("/settings")]
        SettingsRoute,

    #[end_layout]

    #[route("/:..segments")]
    PageNotFound { segments: Vec<String> },
}
```

`sync_route_to_app_state` extracts `backend` → `BackendType`, `account_id` and
writes them into `NavigationState.active_backend` / `active_account_id`.

`on_update` catch-all redirect: tries to pick best first-active-account route,
falls back to `/settings` if none.

### Phase C — Navigation Callsites

**Files**: `server_sidebar.rs`, `channel_list.rs`, `friends_panel.rs`,
`account_bar.rs`, `account_switcher.rs`, `voice_banner.rs`

Every `navigator().push(Route::XYZ)` needs to supply `backend` + `account_id`.
These values come from:
- The `Server` / `DmChannel` / `Group` struct which already carries `account_id`
  and `backend` fields.
- Or from `AppState.nav.active_account_id` / `active_backend` for actions like
  "go to Friends" that stay within the same account.

Helper to convert `BackendType → &'static str`:

```rust
pub fn backend_slug(b: BackendType) -> &'static str {
    match b {
        BackendType::Demo    => "demo",
        BackendType::Stoat   => "stoat",
        BackendType::Matrix  => "matrix",
        BackendType::Discord => "discord",
        BackendType::Teams   => "teams",
    }
}

pub fn slug_to_backend(s: &str) -> Option<BackendType> {
    match s {
        "demo"    => Some(BackendType::Demo),
        "stoat"   => Some(BackendType::Stoat),
        "matrix"  => Some(BackendType::Matrix),
        "discord" => Some(BackendType::Discord),
        "teams"   => Some(BackendType::Teams),
        _         => None,
    }
}
```

### Phase D — Backend-Aware Layout Hooks (Stubs)

Components can branch on `BackendType` using the active_backend from AppState
or by reading the route segment directly. No separate layout needed yet — this
lays the groundwork for rendering Discord-specific features (threads, boosts)
or Teams-specific features (channels within channels) in a later phase.

```rust
// Example: disable emoji reaction button for non-supporting backends
let backend = app_state.read().nav.active_backend;
let supports_reactions = !matches!(backend, Some(BackendType::Teams));
```

### Phase E — Per-Account Notification Settings

**Storage model** (new SurrealDB record type):

```rust
pub struct AccountNotificationSettings {
    pub notify_streams: bool,
    pub notify_friends_voice: bool,
    pub notify_reactions: bool,
    pub sound_new_message: bool,
    pub sound_dm: bool,
    pub sound_ring: bool,
    pub badge_unread: bool,
}
```

Stored at key `notif:{account_id}` in SurrealKV.

**Global settings** (remain in `AppSettings`):
- `desktop_notifications_enabled` — requires OS/browser permission, device-level

**Notifications settings UI** (`settings/notifications.rs`):

```
[Settings → Notifications]
  ┌─ Global ──────────────────────────────────────────┐
  │  Enable Desktop Notifications  [toggle]  [Allow]  │
  └───────────────────────────────────────────────────┘

  ┌─ Demo Account (🧪 Demo) ──────────────────────────┐
  │  People I know start streaming  [toggle]          │
  │  Friends join voice channels    [toggle]          │
  │  Someone reacts to my messages  [toggle]          │
  │  --- Sounds ---                                   │
  │  New Message                    [toggle]          │
  │  Direct Messages                [toggle]          │
  │  Incoming Ring                  [toggle]          │
  │  --- Badges ---                                   │
  │  Enable Unread Message Badge    [toggle]          │
  └───────────────────────────────────────────────────┘

  ┌─ Stoat Account (@username) ───────────────────────┐
  │  ... same toggles ...                             │
  └───────────────────────────────────────────────────┘
```

### Phase F — Server Sidebar Account Awareness

- Server icons already carry `account_id` + `backend` — use them when calling
  `navigator().push(Route::ServerHome { backend, account_id, server_id })`.
- Add small backend emoji badge to each server icon (already done via
  `backend_badge()`).
- Group server icons by account in the sidebar with a thin separator line (future
  visual polish, not blocking correctness).

---

## Work Order (Checklist)

- [x] Write this plan
- [x] **Phase B**: Update `NavigationState`, remove dead nav history code
- [x] **Phase A**: Rewrite `Route` enum + `sync_route_to_app_state`  
- [x] **Phase A**: Update `on_update` root redirect logic  
- [x] **Phase C**: Update all `navigator().push()` callsites with backend + account_id  
- [x] **Phase E**: Add `AccountNotificationSettings` to storage  
- [x] **Phase E**: Update notifications settings UI to per-account  
- [x] **Phase F**: Update server sidebar navigator calls  
- [x] Full build verification: `cargo check --workspace`
- [x] WASM check: `cargo check -p poly-web --target wasm32-unknown-unknown`
- [x] `cargo cranky --workspace`
- [x] `cargo fmt --all`

---

## What Is NOT Changing

- `ClientBackend` trait — no changes; `account_id` + `backend` already on types
- `poly-server` backend — client-side routing change only
- Per-server notification muting — deferred to a later phase
- Separate per-backend layouts (Discord threads, Teams channels) — architecture
  is in place after this refactor; features added incrementally
