# Poly — Multi-Client Architecture

> **Last Updated:** 2026-03-03  
> **Audience:** Developers, AI agents, contributors

---

## 1. Overview

Poly is a **multi-account, multi-backend** messenger client. A single running instance can simultaneously connect to:

- **Stoat** (formerly Revolt) — custom REST/WebSocket client
- **Matrix** — via `matrix-sdk`
- **Discord** — TBD approach (Phase 3.3)
- **Microsoft Teams** — via Microsoft Graph API
- **Poly Native** — our own server protocol
- **Demo** — mock client for UI testing

Each backend can have **multiple simultaneous accounts** (e.g., 3 Discord accounts + 2 Matrix accounts).

---

## 2. Client Layer (`clients/`)

### 2.1 The `ClientBackend` Trait

All backends implement `poly_client::ClientBackend` (defined in `clients/client/src/lib.rs`):

```
clients/
├── client/         # poly-client — trait + shared types
│   └── src/
│       ├── lib.rs      # ClientBackend trait
│       ├── types.rs    # BackendType, Server, Channel, Message, User, etc.
│       └── events.rs   # ClientEvent stream types
│
├── demo/           # poly-demo — mock data for UI testing
├── stoat/          # poly-stoat — Stoat (Revolt) implementation
├── matrix/         # poly-matrix — Matrix SDK wrapper
├── discord/        # poly-discord — Discord implementation
└── teams/          # poly-teams — Microsoft Teams (Graph API)
```

The trait provides a uniform async API for:
- Authentication / session management
- Server/channel listing
- Message send/receive
- User profiles, friends, presence
- Voice/video participant tracking
- Real-time event streaming

### 2.2 `ClientManager`

`ClientManager` (in `poly-core/src/client_manager.rs`) holds `Arc<RwLock<Box<dyn ClientBackend>>>` instances keyed by account ID. It:

- Activates/deactivates backend connections
- Maps server IDs → account IDs (which account owns which server)
- Stores authenticated sessions for UI access
- Is provided as `Signal<ClientManager>` context to all UI components

### 2.3 Feature Flags

Each backend is compile-time gated in `poly-core/Cargo.toml`:

```toml
[features]
default = ["demo"]
stoat = ["dep:poly-stoat"]
matrix = ["dep:poly-matrix"]
discord = ["dep:poly-discord"]
teams = ["dep:poly-teams"]
demo = ["dep:poly-demo"]
```

A build with only `discord + teams` excludes all Stoat/Matrix/Demo code entirely.

---

## 3. UI Layer (`crates/core/src/ui/account/`)

### 3.1 The Problem

Different backends have fundamentally different UI needs:

| Feature | Discord | Stoat | Matrix | Teams |
|---|---|---|---|---|
| Server context menu | Invite, Boost, Emoji, Roles | Invite, Settings | Room Directory, Space settings | Schedule Meeting, Files |
| Channel types | Text, Voice, Stage, Forum | Text, Voice | Room (encrypted/unencrypted) | Channel, Chat, Meeting |
| Settings panels | Nitro, Boost, Integrations | Custom bots, Webhooks | E2EE verification, Cross-signing | Apps, Connectors |
| User cards | Roles, Badges, Nitro | Badges | Verification status | Org chart, Presence |

A single monolithic `channel_list.rs` or `context_menu.rs` handling all backends via `if/else` branches would become unmaintainable.

### 3.2 The Solution: Per-Backend UI Directories

```
src/ui/account/
├── mod.rs                    # Re-exports + BackendComponents dispatch
│
├── common/                   # ★ Shared UI — used by ALL backends
│   ├── mod.rs
│   ├── channel_list.rs       # Channel list shell
│   ├── chat_view.rs          # Message list + input
│   ├── user_sidebar.rs       # Member list
│   ├── voice_bar.rs          # Voice connection bar
│   ├── voice_view.rs         # Voice/video participant grid
│   ├── emoji_picker.rs       # Emoji grid
│   ├── account_bar.rs        # User info + mute/deafen
│   ├── account_switcher.rs   # DM view account bar
│   ├── account_server_bar.rs # Bar 2 navigation
│   ├── friends_panel.rs      # Friends browser
│   └── notifications.rs      # Aggregated notification feed
│
├── server/                   # Server-scoped common + dispatch
│   ├── mod.rs
│   ├── context_menu.rs       # Dispatches to per-backend menus
│   └── settings/
│
├── settings/                 # Account settings (common shell)
│
├── demo/                     # Demo backend overrides
│   ├── mod.rs
│   └── context_menu.rs
│
├── stoat/                    # Stoat backend overrides
│   ├── mod.rs
│   └── context_menu.rs
│
├── discord/                  # Discord backend overrides
│   ├── mod.rs
│   └── context_menu.rs
│
├── matrix/                   # Matrix backend overrides
│   ├── mod.rs
│   └── context_menu.rs
│
├── teams/                    # Teams backend overrides
│   ├── mod.rs
│   └── context_menu.rs
│
└── poly_native/              # Poly native server overrides
    ├── mod.rs
    └── context_menu.rs
```

### 3.3 Dispatch Pattern

The routing already carries `:backend` in every account-scoped URL (`/:backend/:account_id/...`). The `BackendType` enum is available via `AppState.nav.active_backend`.

**Dispatch is a simple `match` on `BackendType`**, not dynamic trait dispatch:

```rust
/// Render backend-specific context menu items for a server.
pub fn backend_server_context_menu_items(
    backend: BackendType,
    server_id: &str,
    account_id: &str,
) -> Element {
    match backend {
        BackendType::Demo => demo::server_context_menu_extras(server_id, account_id),
        BackendType::Stoat => stoat::server_context_menu_extras(server_id, account_id),
        BackendType::Discord => discord::server_context_menu_extras(server_id, account_id),
        BackendType::Matrix => matrix::server_context_menu_extras(server_id, account_id),
        BackendType::Teams => teams::server_context_menu_extras(server_id, account_id),
    }
}
```

Feature-gated backends use `#[cfg(feature = "...")]` to exclude their UI code:

```rust
#[cfg(feature = "demo")]
pub mod demo;

#[cfg(feature = "stoat")]
pub mod stoat;
```

When a backend is not compiled, its match arm returns an empty `rsx! {}`.

### 3.4 What Goes in `common/` vs Per-Backend

**`common/`** — Components that are structurally identical across backends:
- Message rendering (text, images, reactions, timestamps)
- Voice/video participant grid
- Emoji picker
- Account bar (user info, mute/deafen controls)
- Notification feed
- Friends panel

**Per-backend** — Components that differ meaningfully:
- Server context menu items (Invite, Boost, Leave, etc.)
- Server settings (backend-specific configuration)
- Channel list decorations (badges, icons, special channel types)
- User profile cards (roles, badges, verification status)
- Connection/auth UI (login flows differ per backend)

### 3.5 Evolution Path

**Phase 2.11** (now): Create the structure, move existing code to `common/`, create stub per-backend modules with context menus.

**Phase 3.x** (future): As each backend is implemented, grow its per-backend directory:
- `stoat/channel_list.rs` — Stoat-specific channel decorations
- `discord/user_card.rs` — Discord role colors, Nitro badges
- `matrix/verification.rs` — E2EE verification UI
- `teams/meeting.rs` — Meeting scheduling UI

---

## 4. Routing

Every account-scoped URL carries `/:backend/:account_id/`:

```
/:backend/:account_id/dms                           → DM home
/:backend/:account_id/dms/:channel_id               → DM conversation
/:backend/:account_id/friends                       → Friends list
/:backend/:account_id/channels/:server_id            → Server home
/:backend/:account_id/channels/:server_id/:channel_id → Server channel
/:backend/:account_id/settings                       → Account settings
/:backend/:account_id/servers/:server_id/settings    → Server settings
```

The `:backend` slug maps to `BackendType` via `BackendType::from_slug()` and is synced to `AppState.nav.active_backend` by `sync_route_to_app_state()`.

This means **every component in the render tree has access to which backend** is active, enabling per-backend UI dispatch at any level.

---

## 5. Data Flow

```
┌─────────────────────────────────────────────────┐
│                   UI Layer                       │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
│  │ common/  │  │  demo/   │  │  stoat/  │ ...  │
│  │ (shared) │  │(overrides)│  │(overrides)│      │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘      │
│       │              │              │             │
│       └──────────────┼──────────────┘             │
│                      ▼                            │
│          Signal<ChatData> + Signal<AppState>      │
│                      │                            │
└──────────────────────┼────────────────────────────┘
                       ▼
              ┌─────────────────┐
              │  ClientManager  │
              │  (account_id →  │
              │   BackendHandle)│
              └────────┬────────┘
                       │
        ┌──────────────┼──────────────┐
        ▼              ▼              ▼
   ┌─────────┐   ┌─────────┐   ┌─────────┐
   │poly-demo│   │poly-stoat│   │poly-disc│ ...
   └─────────┘   └─────────┘   └─────────┘
```

All backends feed data through the **same** `ClientBackend` trait into `ChatData`. The UI reads `ChatData` uniformly. Backend-specific UI components are selected at render time based on `BackendType`, but they still read from the same shared state signals.

---

## 6. Adding a New Backend

To add a new messenger backend (e.g., Slack):

### Client layer
1. Create `clients/slack/` with `Cargo.toml`, `agents.md`, `README.md`
2. Implement `ClientBackend` trait in `clients/slack/src/lib.rs`
3. Add `BackendType::Slack` variant to `clients/client/src/types.rs`
4. Add `slack = ["dep:poly-slack"]` feature to `crates/core/Cargo.toml`

### UI layer
5. Create `crates/core/src/ui/account/slack/mod.rs`
6. Create `crates/core/src/ui/account/slack/context_menu.rs`
7. Add `#[cfg(feature = "slack")] pub mod slack;` to `ui/account/mod.rs`
8. Add `BackendType::Slack` arm to all dispatch match blocks
9. Add Slack-specific UI components as needed

### Routing
10. Add `"slack"` to `BackendType::from_slug()` / `slug()`
11. No route enum changes needed — `:backend` is already a generic `String`

---

## 7. Feature Gate Patterns

Backend UI modules are feature-gated to match the client feature flags:

```rust
// In ui/account/mod.rs
#[cfg(feature = "demo")]
pub mod demo;

#[cfg(feature = "stoat")]
pub mod stoat;

// Dispatch helper with cfg fallbacks
pub fn backend_context_menu_extras(backend: BackendType, ...) -> Element {
    match backend {
        #[cfg(feature = "demo")]
        BackendType::Demo => demo::server_context_menu_extras(...),
        #[cfg(not(feature = "demo"))]
        BackendType::Demo => rsx! {},
        // ...
    }
}
```

This ensures:
- Backend UI code is excluded from builds that don't need it
- The dispatch match is always exhaustive
- No dead code warnings
