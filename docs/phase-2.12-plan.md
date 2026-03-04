# Phase 2.12 — Account Status, Diagnostics, Offline Persistence & Startup Polish

> **Status:** In Progress — Session 2 complete  
> **Started:** 2026-03-05  
> **Owner:** AI agent (unattended)

---

## Goals

1. **Startup flicker elimination** — Discord-style opaque loading frame before WASM hydrates
2. **Connection status indicators** — per-account icon overlays: top-left = connected/connecting/error, bottom-left = presence/away
3. **Diagnostics page** — Settings › General shows WS count, storage bytes, connected accounts
4. **Persist favorited servers** — survive account disconnect; show last-known state offline
5. **Client cache trait** — optional `CachedBackend` in `poly-client` for backends to implement

---

## 1. Startup Flicker Elimination

### Problem
When the WASM binary loads (200–800 ms), the browser shows a white page then the fully-rendered UI snaps in. This produces a flash of unstyled/partial content.

### Solution — Discord-style pre-render
**Phase A** (pure HTML/CSS, zero Rust changes):
- Inject an inline `<style>` block in `index.html` that sets:
  ```css
  body { background: #1a1c1e; margin: 0; } /* neutral-dark bg */
  #pre-load { position:fixed; inset:0; background:#1a1c1e; z-index:9999; }
  ```
- Add a `<div id="pre-load"></div>` immediately after `<body>` — covers the blank white.
- Add a `<script>` at the bottom of `<body>` that removes `#pre-load` after the WASM dispatches the first render event:
  ```js
  document.addEventListener('dioxus-ready', () => {
    document.getElementById('pre-load')?.remove();
  });
  ```
- Dioxus dispatches `dioxus-ready` after the first VDOM patch lands in the DOM.

**Phase B** (optional, polish—do later):
- Show a mini skeleton: two vertical rounded rects (mimicking sidebar + content) in `#pre-load`, animated with a subtle shimmer CSS keyframe.
- Matches the neutral-dark preset dimensions exactly so the snap-in is invisible.

### Tasks
- [ ] Add `<div id="pre-load">` + inline styles to `apps/web/index.html`  
- [ ] Confirm `dioxus-ready` event fires (or use `MutationObserver` on `#main` as fallback)  
- [ ] Test with DevTools Network throttling (Slow 3G)  
- [ ] Document the fallback: if `dioxus-ready` never fires, `#pre-load` fades out after 3 s via CSS `animation-delay`

### Files
- `apps/web/index.html` — add pre-load div + CSS

---

## 2. Connection Status Model

### New enum in `poly-client`

```rust
/// The connection state of a backend to its remote server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Successfully authenticated and event stream is live.
    Connected,
    /// Attempting initial connection or reconnecting after a drop.
    Connecting,
    /// Explicitly disconnected by the user (appear-offline mode).
    Disconnected,
    /// Server returned 4xx/5xx or network is unreachable.
    Error(String),
}
```

### Storage in `ClientManager`

`ClientManager` gains a new field:
```rust
pub connection_statuses: HashMap<String, ConnectionStatus>,
```

Initialized to `Connecting` when a backend is activated, updated to `Connected`/`Error` by the event stream consumer (future phase) and by explicit disconnect.

For the **demo backend**: set `Connected` immediately on `activate_demo()`.  
For **real backends** (future): the WebSocket/SSE event loop writes `Connected` after handshake and `Error("…")` on failure.

### Files
- `clients/client/src/types.rs` — add `ConnectionStatus` enum + `PresenceStatus` enum  
- `crates/core/src/client_manager.rs` — add `connection_statuses` field; set demo as `Connected`

---

## 3. Presence / Away Status Model

### New enum in `poly-client`

```rust
/// The user-chosen availability/presence status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PresenceStatus {
    /// Fully online and receiving notifications.
    #[default]
    Online,
    /// Away (auto-set after inactivity or manually chosen).
    Away,
    /// Do not disturb — suppresses notifications.
    DoNotDisturb,
    /// Appears offline to others but still connected.
    AppearOffline,
    /// Truly offline — no backend connection is made.
    Offline,
}
```

`PresenceStatus::Offline` means: do not connect at all. The backend is never activated. The UI shows the account icon with cached server data from local storage.

### Storage in `ClientManager`

```rust
pub presence_statuses: HashMap<String, PresenceStatus>,
```

### Persistence  
Both `ConnectionStatus` and `PresenceStatus` per account are stored in `AppSettings` (or a new `AccountSettings` table in SurrealKV). On boot `init_storage` reads them and applies before connecting.

### Files
- `clients/client/src/types.rs` — `PresenceStatus` enum  
- `crates/core/src/client_manager.rs` — `presence_statuses` field  
- `crates/core/src/storage/` — `AccountSettings` table / field  

---

## 4. Account Icon Overlays

### Visual specification

Each account icon (in Bar 1 = FavoritesBar, and in the big icon button in Bar 2) gets **two small dot overlays**:

```
┌──────────────────────────┐
│ 🔴        account icon   │  ← top-left: ConnectionStatus dot
│     avatar/emoji         │
│                          │
│ 🟢                       │  ← bottom-left: PresenceStatus dot  
└──────────────────────────┘
```

#### Connection status → top-left dot

| Status        | Color  | Emoji | Tooltip            |
|---------------|--------|-------|--------------------|
| Connected     | green  | ✅    | "Connected"        |
| Connecting    | orange | 🟡    | "Connecting…"      |
| Disconnected  | gray   | ⬜    | "Disconnected"     |
| Error(_)      | red    | 🔴    | "Error: {message}" |

#### Presence status → bottom-left dot

| Status        | Color  | Emoji | Tooltip            |
|---------------|--------|-------|--------------------|
| Online        | green  | 🟢    | "Online"           |
| Away          | yellow | 🟡    | "Away"             |
| DoNotDisturb  | red    | 🔴    | "Do Not Disturb"   |
| AppearOffline | gray   | ⬜    | "Appear Offline"   |
| Offline       | gray   | ⬛    | "Offline"          |

### Implementation
- New `AccountIconDot` sub-component in `favorites_sidebar.rs` and `account_server_bar.rs`
- CSS: `position: absolute; width: 10px; height: 10px; border-radius: 50%; border: 2px solid var(--sidebar-bg)`
  - Top-left: `top: -2px; left: -2px`
  - Bottom-left: `bottom: -2px; left: -2px`
- The icon `div` gets `position: relative` and `overflow: visible`

### Files
- `crates/core/src/ui/favorites_sidebar.rs` — add status dots to account icons  
- `crates/core/src/ui/account/common/account_server_bar.rs` — add status dots to main account switcher icon  
- `crates/core/assets/styles/` — add dot CSS classes  
- `locales/en/main.ftl` — add tooltip strings (`status-connected`, `status-connecting`, etc.)

---

## 5. Diagnostics Settings Page

New settings section `SettingsSection::Diagnostics` added to the sidebar and mapped to a new page.

### Content

```
┌─────────────────────────────────────────┐
│  Diagnostics                            │
├─────────────────────────────────────────┤
│  Accounts                               │
│  ● demo (cat) — Demo — ✅ Connected     │
│  ● demo2 (dog) — Demo — ✅ Connected    │
│                                         │
│  Connections                            │
│  WebSocket / SSE streams open: 2        │
│  (future: list each with URL + latency) │
│                                         │
│  Local Storage                          │
│  SurrealKV database: 4.2 MB             │
│  Cached server count: 6                 │
│  Cached message count: 142              │
│                                         │
│  App                                    │
│  Poly version: 0.1.0                    │
│  Build target: wasm32                   │
│  Locale: en                             │
└─────────────────────────────────────────┘
```

### Tasks
- [ ] Add `Diagnostics` variant to `SettingsSection` enum in `state/mod.rs`
- [ ] Create `crates/core/src/ui/settings/diagnostics.rs`
- [ ] Add sidebar entry in `settings/mod.rs`
- [ ] Expose `connection_statuses` from `ClientManager` to the page  
- [ ] Storage size: read via `navigator.storage.estimate()` (web) or `fs::metadata` (native)
- [ ] Add i18n strings

### Files
- `crates/core/src/state/mod.rs` — add `Diagnostics` to `SettingsSection`  
- `crates/core/src/ui/settings/diagnostics.rs` — new page component  
- `crates/core/src/ui/settings/mod.rs` — add sidebar entry + match arm  

---

## 6. Persist Favorited Servers

### Problem
`favorited_server_ids` and `ChatData::servers` live only in memory. On reload with no network, they are empty — even for known-offline accounts.

### Solution — per-account server cache in SurrealKV

New storage table `cached_servers`:
```
Key:   "{account_id}:{server_id}"
Value: CachedServer { server: Server, cached_at: DateTime<Utc> }
```

**Write**: whenever `toggle_demo` (or future real backends) loads servers, write them to `cached_servers`.

**Read on startup** (in `init_storage`): load all `cached_servers` into `ChatData::servers`. This way Bar 1 favorites show immediately on startup even before the network check completes.

**Eviction policy**: entries older than 7 days are pruned on startup.

The field `ChatData::favorited_server_ids` is already persisted (we set it during `toggle_demo`). We also need to persist it to SurrealKV on every change — add a `use_effect` watcher in `FavoritesBar` that saves it.

### Tasks
- [ ] Add `Storage::get_cached_servers()` / `set_cached_server()` / `prune_old_cached_servers()` to storage module  
- [ ] Call `set_cached_server` from `toggle_demo` and future backend loaders  
- [ ] Call `get_cached_servers` in `init_storage` to pre-populate `ChatData::servers`  
- [ ] Persist `favorited_server_ids` to storage in `favorites_sidebar.rs` `use_effect`  
- [ ] Load `favorited_server_ids` from storage in `init_storage`

### Files
- `crates/core/src/storage/mod.rs` — add cached server methods  
- `crates/core/src/storage/web.rs` — localStorage impl  
- `crates/core/src/storage/native.rs` — SurrealKV impl  
- `crates/core/src/ui/favorites_sidebar.rs` — persist favorites on change  
- `crates/core/src/ui/mod.rs` (`init_storage`) — load cached servers + favorites on startup

---

## 7. Client Cache Trait (poly-client)

### Optional extension trait

```rust
/// Optional extension: backends that support local caching implement this.
/// The cache layer sits between the UI and the live backend, returning
/// stale data instantly while a network fetch is in-flight.
pub trait CachedBackend: ClientBackend {
    /// Write a server snapshot to the local cache.
    async fn cache_server(&self, server: &Server) -> ClientResult<()>;
    /// Read all cached servers for this account (may be stale).
    async fn get_cached_servers(&self) -> ClientResult<Vec<Server>>;
    /// Clear all cached data for this account.
    async fn clear_cache(&self) -> ClientResult<()>;
    /// Timestamp of the last successful sync.
    fn last_synced_at(&self) -> Option<DateTime<Utc>>;
}
```

For Phase 2.12, only the `DemoClient` implements this (trivially: returns the in-memory demo data). Real backends implement it in Phase 3.

### Files
- `clients/client/src/lib.rs` — add `CachedBackend` trait  
- `clients/demo/src/lib.rs` — trivial impl for `DemoClient`

---

## Progress Checklist

### Startup Flicker
- [x] `apps/web/public/index.html` — dark pre-render body + MutationObserver overlay (💬 spinner)

### Connection & Presence Models
- [x] `ConnectionStatus` enum in `poly-client` (`clients/client/src/types.rs`)
- [x] `AccountPresence` enum in `poly-client` (renamed from `PresenceStatus` to avoid conflict with the existing `PresenceStatus` on `User.presence`)
- [x] `ClientManager` fields: `connection_statuses: HashMap<String, ConnectionStatus>`, `presence_statuses: HashMap<String, AccountPresence>`
- [x] Demo activates with `Connected` + `Online` status for both demo-cat and demo-dog

### Account Icon Overlays
- [x] New CSS classes: `.status-dot`, `.connection-dot.{connected|connecting|disconnected|error}`, `.presence-dot.{online|away|dnd|appear-offline|offline}` — in `tailwind.css`
- [x] `FavoritesBar` account icons (`AccountIcon` component): `connection-dot` (top-right) + `presence-dot` (bottom-right) span overlays
- [ ] `AccountServerBar` — no account icons in Bar 2; both dots are only in Bar 1

### Diagnostics Page
- [x] `Diagnostics` variant added to `SettingsSection` enum in `state/mod.rs`
- [x] `crates/core/src/ui/settings/diagnostics.rs` — per-account connection/presence table component
- [x] Sidebar entry in `settings/mod.rs` + match arm in content section
- [x] i18n strings in `locales/en/main.ftl`
- [x] CSS for diagnostics table in `tailwind.css`

### Server Persistence (favorited_server_ids)
- [x] `AppSettings.favorited_server_ids: Vec<String>` added to storage model (with `#[serde(default)]`)
- [x] Load `favorited_server_ids` from storage in `init_storage` (before demo toggle runs)
- [x] `persist_favorites(ids: Vec<String>)` helper in `favorites_sidebar.rs`
- [x] Persist on drag-drop drop (nav background + per-item reorder/insert handlers)
- [x] Persist on context menu "Add to favorites" and "Remove from favorites"
- [x] Persist on leave server (server settings general.rs)
- [x] Persist on demo toggle-on and toggle-off (includes updated favorites after demo server removal)
- [x] `persist_setup_completion()` updated to include `favorited_server_ids: Vec::new()` for new installs
- [ ] `cached_servers` storage methods — deferred to later session (servers themselves not yet cached)

### Client Cache Trait
- [ ] `CachedBackend` trait in `poly-client` — deferred
- [ ] Demo impl — deferred

---

## Session Log

### 2026-03-05 (Session 1)
- Identified root cause of AccountServerBar vanish: `on_update` not firing for initial browser URL in Dioxus 0.7 web. Fixed by calling `sync_route_to_app_state` in `MainLayout` via `use_route()`.
- Created this plan.
- Started implementation: `ConnectionStatus`, `PresenceStatus` enums added to types.rs.

### 2026-03-05 (Session 2)
- Completed `ConnectionStatus` + `AccountPresence` enums in `poly-client/types.rs`.
  - NOTE: named `AccountPresence` (not `PresenceStatus`) because `PresenceStatus` already exists on `User.presence` with different variants (Idle/Invisible).
- Added `connection_statuses` and `presence_statuses` fields to `ClientManager`; demo accounts initialized to `Connected`/`Online`.
- Added CSS status dot classes to `tailwind.css`.
- Added status overlay dots to `AccountIcon` in `favorites_sidebar.rs` (connection dot top-right, presence dot bottom-right).
- Added `SettingsSection::Diagnostics` variant; created `settings/diagnostics.rs` with AccountDiagnosticsRow per account; wired in sidebar + match arm; added i18n strings + CSS.
- Added `favorited_server_ids: Vec<String>` to `AppSettings`; restored on startup in `init_storage`; added `persist_favorites()` helper; called at all 6 mutation points.
- Created `apps/web/public/index.html` with dark pre-load CSS and MutationObserver overlay to eliminate white flash during WASM init.
- All checks pass: `cargo check -p poly-core`, WASM target, `cargo cranky --workspace` — zero warnings/errors.
