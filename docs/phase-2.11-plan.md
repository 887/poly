# Phase 2.11 — Per-Backend UI Abstraction Layer

> **Created:** 2026-03-03  
> **Status:** Complete  
> **Depends on:** Phase 2.9 (Dual Sidebar), Phase 2.10  
> **Goal:** Separate backend-specific UI from common UI so each messenger backend can have its own look, feel, and behaviour.

---

## Motivation

Poly is a **multi-backend** messenger client. Discord, Stoat, Matrix, Teams, and our own Poly server all have fundamentally different UI conventions:

- **Discord** has server context menus with "Invite People", "Server Boost", emoji management, role colours
- **Stoat** has different server settings, no boost concept, different channel types
- **Matrix** has Spaces (not servers), room directories, E2EE verification flows
- **Teams** has Teams/Channels/Tabs, meeting scheduling, file sharing UI
- **Demo** needs to exercise all common UI paths with mock data

A flat `ui/account/` directory that handles all backends in a single set of components will become unmaintainable as we add real backend implementations. We need the UI architecture to mirror the client architecture: **one directory per backend with concrete implementations, one common directory with shared abstractions.**

---

## Architecture

### Directory Structure After Refactor

```
src/ui/account/
├── mod.rs                          # Re-exports, BackendComponents dispatch
├── common/                         # ★ Shared abstractions — used by ALL backends
│   ├── mod.rs
│   ├── channel_list.rs             # Common ChannelList shell (delegates to backend)
│   ├── chat_view.rs                # Common ChatView (messages, input — mostly shared)
│   ├── user_sidebar.rs             # Common UserSidebar (member list)
│   ├── voice_bar.rs                # Common VoiceBar (voice connection status)
│   ├── voice_view.rs               # Common VoiceChannelView (participants grid)
│   ├── emoji_picker.rs             # Common EmojiPicker (shared emoji grid)
│   ├── account_bar.rs              # Common AccountBar (user info + controls)
│   ├── account_switcher.rs         # Common AccountSwitcher (DM view bar)
│   ├── account_server_bar.rs       # Common AccountServerBar (Bar 2)
│   ├── friends_panel.rs            # Common FriendsPanel (friends browser)
│   └── notifications.rs            # Common NotificationsView (aggregated feed)
│
├── server/                         # Server-scoped common components
│   ├── mod.rs
│   ├── context_menu.rs             # ★ Dispatch — delegates to backend-specific menu
│   └── settings/                   # Server settings (common shell, backend panels)
│       ├── mod.rs
│       ├── general.rs
│       ├── notifications.rs
│       └── profile.rs
│
├── settings/                       # Account settings (common shell)
│   ├── mod.rs
│   └── notifications.rs
│
├── demo/                           # ★ Demo backend UI overrides
│   ├── mod.rs                      # Implements BackendComponents for Demo
│   └── context_menu.rs             # Demo-specific context menu items
│
├── stoat/                          # ★ Stoat backend UI overrides
│   ├── mod.rs
│   └── context_menu.rs
│
├── discord/                        # ★ Discord backend UI overrides
│   ├── mod.rs
│   └── context_menu.rs
│
├── matrix/                         # ★ Matrix backend UI overrides
│   ├── mod.rs
│   └── context_menu.rs
│
├── teams/                          # ★ Teams backend UI overrides
│   ├── mod.rs
│   └── context_menu.rs
│
└── poly_native/                    # ★ Poly native server UI overrides
    ├── mod.rs
    └── context_menu.rs
```

### Key Principle: Dispatch by BackendType

The routing already carries `:backend` in every account-scoped URL. The UI uses `BackendType` to select which backend-specific components to render. This is done through a **`BackendComponents`** trait:

```rust
/// Trait defining backend-specific UI element renderers.
///
/// Each backend (Demo, Stoat, Discord, etc.) implements this trait
/// to provide its unique context menu items, settings panels, etc.
pub trait BackendComponents {
    /// Extra context menu items for right-clicking a server icon.
    fn server_context_menu_items(server_id: &str, ...) -> Element;
    
    /// Backend-specific server settings sections.
    fn server_settings_extra(server_id: &str, ...) -> Element;
    
    /// Backend-specific channel list decorations.
    fn channel_list_extras(...) -> Element;
}
```

### Dispatch Pattern (No Dynamic Dispatch Needed)

Since backends are feature-flagged at compile time, we use a simple match on `BackendType`:

```rust
fn render_server_context_menu(backend: BackendType, ...) -> Element {
    match backend {
        BackendType::Demo => demo::context_menu(...),
        BackendType::Stoat => stoat::context_menu(...),
        BackendType::Discord => discord::context_menu(...),
        // ...
    }
}
```

---

## Checklist

### 2.11.1 — Create per-backend UI directories
- [x] Create `ui/account/common/` with `mod.rs`
- [x] Create `ui/account/demo/` with `mod.rs`
- [x] Create `ui/account/stoat/` with `mod.rs`
- [x] Create `ui/account/discord/` with `mod.rs`
- [x] Create `ui/account/matrix/` with `mod.rs`
- [x] Create `ui/account/teams/` with `mod.rs`
- [x] Create `ui/account/poly_native/` with `mod.rs`

### 2.11.2 — Move existing components to common/
- [x] Move `channel_list.rs` → `common/channel_list.rs`
- [x] Move `chat_view.rs` → `common/chat_view.rs`
- [x] Move `user_sidebar.rs` → `common/user_sidebar.rs`
- [x] Move `voice_bar.rs` → `common/voice_bar.rs`
- [x] Move `voice_view.rs` → `common/voice_view.rs`
- [x] Move `emoji_picker.rs` → `common/emoji_picker.rs`
- [x] Move `account_bar.rs` → `common/account_bar.rs`
- [x] Move `account_switcher.rs` → `common/account_switcher.rs`
- [x] Move `account_server_bar.rs` → `common/account_server_bar.rs`
- [x] Move `friends_panel.rs` → `common/friends_panel.rs`
- [x] Move `notifications.rs` → `common/notifications.rs`

### 2.11.3 — Per-backend context menus
- [x] Create `BackendComponents` dispatch in `account/mod.rs`
- [x] Refactor `ServerContextMenu` to dispatch backend-specific items
- [x] Create `demo/context_menu.rs` with demo-specific items
- [x] Create stub `context_menu.rs` for stoat, discord, matrix, teams, poly_native

### 2.11.4 — Architecture documentation
- [x] Create `docs/multi-client-architecture.md`
- [x] Update `crates/core/agents.md` with new directory structure
- [x] Update `docs/overall-plan.md` Decision Registry with D20

### 2.11.5 — Verification
- [x] `cargo check --workspace` passes
- [x] `cargo cranky --workspace` passes
- [x] `cargo check -p poly-web --target wasm32-unknown-unknown` passes
- [x] `cargo fmt --all` passes
- [x] All existing functionality preserved

---

## Design Decisions

**D20: Per-backend UI directories under `ui/account/`**
- Each backend gets its own subdirectory for UI overrides
- Common components stay in `common/`
- Dispatch by `BackendType` at render time
- Feature-gated: if a backend isn't compiled, its UI module is excluded

**Why not dynamic dispatch / trait objects for UI?**
Dioxus components are functions returning `Element`, not trait objects. The natural Rust pattern is `match backend { ... }` which the compiler can optimize and verify exhaustively. Trait objects would add complexity without benefit for a compile-time-known set of backends.

---

## Notes

- This refactor must NOT break any existing functionality
- The demo client's UI should continue to work identically after the move
- Future phases (3.x) will flesh out per-backend components as real clients are implemented
- The routing already carries `:backend` in URLs — no routing changes needed
