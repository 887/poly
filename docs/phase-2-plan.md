# Phase 2 Plan — Project Structure + UI + Backup Infrastructure

> **Status:** 🔄 In Progress  
> **Target Start:** After Phase 1 completion  
> **Parent:** [Overall Plan](overall-plan.md)  
> **Depends On:** [Phase 1](phase-1-plan.md)

---

## 2.1 Workspace & Build Setup

- [x] **2.1.1** Initialize root `Cargo.toml` workspace with all member crates
- [x] **2.1.2** Set up workspace-level dependency versions (`[workspace.dependencies]`)
  - dioxus = "0.7.3"
  - surrealdb = "3.0.1" (feature: kv-surrealkv)
  - tokio (multi-threaded runtime)
  - serde, serde_json
  - ed25519-dalek, x25519-dalek, bip39
  - fluent-bundle
  - All other shared deps
- [x] **2.1.3** Create `Cargo.toml` for every crate with feature flags
  - poly-core features: `stoat`, `matrix`, `discord`, `teams`, `demo`
  - Each client crate conditionally included
- [x] **2.1.4** Configure `Dioxus.toml` for each app target
  - apps/desktop/Dioxus.toml (platform: desktop, renderer: webview)
  - apps/desktop-blitz/Dioxus.toml (platform: desktop, renderer: native/blitz)
  - apps/web/Dioxus.toml (platform: web, fullstack with Axum)
  - apps/android/Dioxus.toml (platform: android)
  - apps/ios/Dioxus.toml (platform: ios)
- [x] **2.1.5** Set up `.gitignore` files
  - Root: target/, node_modules/, .env, *.db
  - Per-crate: build artifacts specific to that crate
  - apps/desktop-electron/: electron build output
- [x] **2.1.6** Set up TailwindCSS
  - `assets/tailwind.css` entry file
  - Theme CSS variable system in `assets/styling/themes/`
  - Verify Dioxus auto-detection works in monorepo
- [x] **2.1.7** **CRITICAL: Validate subsecond hot-reload for poly-core**
  - Create minimal poly-core component
  - Run `dx serve --hotpatch` from apps/desktop
  - Modify poly-core component, verify hot-patch works
  - Document setup in poly-core/agents.md
  - **THIS IS A BLOCKING TASK — NOTHING PROCEEDS UNTIL CONFIRMED**
- [ ] **2.1.8** Set up Electron wrapper project
  - apps/desktop-electron/electron/package.json
  - apps/desktop-electron/electron/main.js (loads WASM build)
  - Build script: compile web target, then bundle with Electron

## 2.2 VSCode Configuration

- [x] **2.2.1** Create `.vscode/launch.json` — all launch profiles
  - Desktop Wry (Linux)
  - Desktop Wry (macOS)
  - Desktop Blitz (Linux)
  - Desktop Blitz (macOS)
  - Desktop Electron (Linux)
  - Desktop Electron (macOS)
  - Web (fullstack)
  - Android (via ADB)
  - iOS (simulator)
  - Backup Server
  - Debug poly-core library tests
- [x] **2.2.2** Create `.vscode/tasks.json` — build tasks
  - Build poly-core (library check)
  - Build desktop-wry
  - Build desktop-blitz
  - Build desktop-electron (compile + package)
  - Build web
  - Build android APK
  - Build iOS
  - Build backup server
  - Run all tests
  - Run clippy
  - Run cargo fmt
- [x] **2.2.3** Create `.vscode/settings.json` — workspace settings
  - Rust analyzer configuration
  - Default formatter
  - File associations

## 2.3 GitHub Actions CI/CD

- [x] **2.3.1** CI: `build-library.yml` — build poly-core only (fast feedback)
- [x] **2.3.2** CI: `build-all.yml` — cascading build of all crates
- [x] **2.3.3** CI: `build-desktop.yml` — Linux, macOS, Windows desktop binaries
- [x] **2.3.4** CI: `build-android.yml` — Android APK
- [x] **2.3.5** CI: `build-ios.yml` — iOS (macOS runner)
- [x] **2.3.6** CI: `build-web.yml` — Web (WASM + Axum server)
- [x] **2.3.7** CI: `build-backup-server.yml` — Backup server Docker image
- [x] **2.3.8** CI: `lint-test.yml` — cargo clippy + cargo test + cargo fmt check
- [x] **2.3.9** Release workflow — tagged releases build all targets

## 2.4 Core Infrastructure — poly-core

### 2.4.1 i18n System
- [x] **2.4.1.1** Create i18n wrapper module (`poly-core/src/i18n/`) ✓
- [x] **2.4.1.2** Implement `t!()` macro with key + named arguments (`#[macro_export] macro_rules! t!`) ✓
- [x] **2.4.1.3** Implement `use_locale()` hook + `provide_locale_context()` for reactive locale switching ✓
- [x] **2.4.1.4** Load `.ftl` files from `locales/` directory (embedded via `include_str!`) ✓
- [x] **2.4.1.5** Fallback chain: user locale → English (sys_locale + fallback in `t_args`) ✓
- [x] **2.4.1.6** Write English `.ftl` files for all UI strings ✓
- [x] **2.4.1.7** Write German `.ftl` files ✓
- [x] **2.4.1.8** Write French `.ftl` files ✓
- [x] **2.4.1.9** Write Spanish `.ftl` files ✓

### 2.4.2 Theme Engine
- [x] **2.4.2.1** Define CSS custom properties for all themeable colors (in `neutral-dark.css`) ✓
- [x] **2.4.2.2** Create `neutral-dark.css` preset (default) ✓
- [x] **2.4.2.3** Create `purple.css` preset (Discord-inspired) ✓
- [x] **2.4.2.4** Create `red.css` preset (Stoat-inspired) ✓
- [x] **2.4.2.5** Implement theme state management — `ThemeConfig` + reactive `Signal<String>` in App context ✓
- [ ] **2.4.2.6** Implement custom CSS editor model (get/set user CSS, preview) — future
- [x] **2.4.2.7** Theme import/export — `export_theme()` + storage `get/set_theme_config()` ✓
- [ ] **2.4.2.8** Dark/light mode: follow device preference by default, user override — future

### 2.4.3 Storage Abstraction (cross-platform KV store)

> **Refactored from "SurrealDB Abstraction"** — see Decision DX-STORAGE-1 below.

- [x] **2.4.3.1** SurrealKV embedded database initialization (native: `crates/core/src/storage/native.rs`)
- [x] **2.4.3.2** Settings CRUD operations — `get_app_settings()` / `set_app_settings()` persisted across restarts ✓
- [x] **2.4.3.3** Account storage — `get_account_tokens()` / `upsert_account_token()` / `remove_account_token()`
- [x] **2.4.3.4** WASM / Web backend — `gloo-storage` LocalStorage (`crates/core/src/storage/web.rs`)
- [x] **2.4.3.5** Platform-transparent `Storage` newtype — same `get()`/`set()`/`delete()` API on both platforms
- [x] **2.4.3.6** Global `STORAGE: OnceLock<Storage>` initialized at app startup via `use_future` in `App`
- [x] **2.4.3.7** **Persistence verified by MCP self-test**: wizard completion → kill → relaunch → wizard skipped ✓
- [x] **2.4.3.8** Favorites storage — `FavoriteItem` + `get/upsert/remove_favorite()` ✓
- [x] **2.4.3.9** Theme preferences storage — `get/set_theme_config()` ✓
- [x] **2.4.3.10** Migration system — `run_migrations()` with `storage_version` tracking ✓

#### Decision DX-STORAGE-1: Storage abstraction design

| Aspect | Decision | Rationale |
|---|---|---|
| Trait approach | `Storage(StorageInner)` newtype (not a dyn trait) | Avoids object-safety issues with async methods; zero-cost monomorphization |
| Native backend | SurrealDB 3.0 + SurrealKV via raw `.query()` | TypedAPI excluded: `SurrealValue` derive macro not exposed downstream |
| WASM backend | `gloo-storage` LocalStorage | Simple, battle-tested, matches IndexedDB semantics for KV use-case |
| Field naming | `payload` (not `value`) | Avoids SurrealQL keyword collision with `VALUE` expression keyword |
| Bind args | `serde_json::json!({ "payload": serialized })` | `serde_json::Value: SurrealValue` → implements `IntoVariables` as object |
| Take calls | `resp.take::<Option<String>>("field")` | Turbofish required — compiler can't infer `R` through `map_err()?` chain |

### 2.4.4 Crypto Module

> Lives in `crates/core/src/crypto/`. Pure Rust, no FFI, no platform divergence.
> See overall-plan.md §6 for algorithm choices and rationale.

- [ ] **2.4.4.1** Ed25519 keypair generation (`ed25519-dalek`) — returns `SigningKey` + `VerifyingKey`
- [ ] **2.4.4.2** X25519 key derivation from Ed25519 private key (`x25519-dalek`) — for DH key exchange
- [ ] **2.4.4.3** BIP39 mnemonic generation from Ed25519 private key bytes (`bip39`) — 24-word phrase
- [ ] **2.4.4.4** BIP39 mnemonic recovery → Ed25519 keypair (reverse: mnemonic → entropy bytes → keypair)
- [ ] **2.4.4.5** Symmetric encryption key derivation — HKDF-SHA256 from X25519 static keypair or passphrase
- [ ] **2.4.4.6** Encrypt helper: `encrypt(plaintext: &[u8], key: &SymmetricKey) -> Vec<u8>` — XSalsa20-Poly1305 with random nonce prepended
- [ ] **2.4.4.7** Decrypt helper: `decrypt(ciphertext: &[u8], key: &SymmetricKey) -> Result<Vec<u8>>` — strips nonce, decrypts, authenticates
- [ ] **2.4.4.8** Public key hex encoding/decoding — `pubkey_to_hex()` / `hex_to_pubkey()` (Account ID format)
- [ ] **2.4.4.9** Mnemonic export to file (`.txt`, user-chosen path via file dialog)
- [ ] **2.4.4.10** Store keypair in SurrealKV on first launch — `set_identity()` / `get_identity()` in storage module

### 2.4.5 Backup Sync Client

> Lives in `crates/core/src/sync/`. See overall-plan.md §5 for detailed auth flow,
> passphrase auth, token lifecycle, and per-server status model.
> See [phase-2.3-plan.md](phase-2.3-plan.md) for the server-side implementation.

#### 2.4.5.A Server Record Model
```rust
struct BackupServer {
    url: String,          // e.g. "https://backup.example.com"
    label: String,        // User-provided friendly name
    enabled: bool,        // On/off slider — disabled servers skipped during sync
    public_key: String,   // Our Ed25519 pubkey (which identity to use)
    // Derived at runtime — not stored:
    status: ServerStatus, // Connected | AuthRequired | Unreachable | Syncing
    last_synced: Option<DateTime>,
    last_sequence: u64,
    token_expires_at: Option<DateTime>,
}

enum ServerStatus { Connected, AuthRequired, Unreachable, Syncing, Disabled }
```

#### 2.4.5.B Tasks
- [ ] **2.4.5.1** `BackupServer` storage model — `get/upsert/remove_backup_server()` in storage module
- [ ] **2.4.5.2** PoW challenge solver — `solve_pow(nonce: &str, difficulty: u32) -> u64` — SHA-256 mining loop
- [ ] **2.4.5.3** Full auth flow — `authenticate(server: &BackupServer, passphrase: &str) -> Result<Token>`:
  - POST `/api/challenge` with public key
  - Mine PoW solution
  - POST `/api/auth` with solution + passphrase
  - Store resulting token in SurrealKV under `backup_token:{server_url}`
- [ ] **2.4.5.4** Token retrieval + expiry check — `get_valid_token(server_url)`: returns stored token if valid, triggers re-auth if expired or within 30-day proactive window
- [ ] **2.4.5.5** Encrypt settings blob — serialize `AppSettings` → JSON → encrypt with derived symmetric key
- [ ] **2.4.5.6** Push encrypted settings to one server — `push_settings(server, token, encrypted_blob) -> Result<u64>` (returns new sequence)
- [ ] **2.4.5.7** Pull encrypted settings delta — `pull_settings(server, token, since_sequence) -> Result<Vec<EncryptedChange>>`
- [ ] **2.4.5.8** Decrypt + merge pulled changes into local storage
- [ ] **2.4.5.9** Multi-server sync — iterate all `enabled` servers, push then pull; collect per-server status
- [ ] **2.4.5.10** Proactive token refresh — on sync, check if token expires within 30 days; if so, re-auth in background
- [ ] **2.4.5.11** Handle 401 Unauthorized — clear stored token, set server status to `AuthRequired`, surface to UI
- [ ] **2.4.5.12** Sync status signal — `Signal<HashMap<server_url, ServerStatus>>` consumed by backup settings UI
- [ ] **2.4.5.13** Manual "Sync now" trigger from settings UI

## 2.5 Client Trait System — poly-client

- [ ] **2.5.1** Define `ClientBackend` trait
  ```rust
  trait ClientBackend {
      // Authentication
      async fn authenticate(&mut self, credentials: AuthCredentials) -> Result<Session>;
      async fn logout(&mut self) -> Result<()>;
      
      // Servers / Communities
      async fn get_servers(&self) -> Result<Vec<Server>>;
      async fn get_server(&self, id: &ServerId) -> Result<Server>;
      
      // Channels
      async fn get_channels(&self, server_id: &ServerId) -> Result<Vec<Channel>>;
      async fn get_channel(&self, id: &ChannelId) -> Result<Channel>;
      
      // Messages
      async fn send_message(&self, channel_id: &ChannelId, content: MessageContent) -> Result<Message>;
      async fn get_messages(&self, channel_id: &ChannelId, options: MessageQuery) -> Result<Vec<Message>>;
      
      // Users
      async fn get_user(&self, id: &UserId) -> Result<User>;
      async fn get_friends(&self) -> Result<Vec<User>>;
      async fn get_channel_members(&self, channel_id: &ChannelId) -> Result<Vec<User>>;
      
      // Groups (multi-user DMs)
      async fn get_groups(&self) -> Result<Vec<Group>>;
      
      // Direct Messages
      async fn get_dm_channels(&self) -> Result<Vec<DmChannel>>;
      
      // Notifications
      async fn get_notifications(&self) -> Result<Vec<Notification>>;
      
      // Presence
      async fn get_presence(&self, user_id: &UserId) -> Result<PresenceStatus>;
      async fn set_presence(&self, status: PresenceStatus) -> Result<()>;
      
      // Real-time event stream
      fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent>>>;
      
      // Backend info
      fn backend_type(&self) -> BackendType;
      fn backend_name(&self) -> &str;
  }
  ```
- [ ] **2.5.2** Define shared data types
  - `Server` { id, name, icon_url, categories: Vec<Category> }
  - `Category` { id, name, channels: Vec<ChannelId> }
  - `Channel` { id, name, channel_type: Text|Voice|Video, unread_count }
  - `Message` { id, author, content, timestamp, attachments, reactions }
  - `User` { id, display_name, avatar_url, presence }
  - `Group` { id, members, name, last_message }
  - `DmChannel` { id, user, last_message }
  - `Notification` { id, kind, source, timestamp, read }
  - `BackendType` enum { Stoat, Matrix, Discord, Teams, Demo }
  - `AuthCredentials` enum { Token, EmailPassword, OAuth, DeviceCode }
- [ ] **2.5.3** Define event types
  - `ClientEvent` enum { MessageReceived, MessageEdited, MessageDeleted, PresenceChanged, NotificationReceived, TypingStarted, ChannelUpdated, ServerUpdated, FriendRequestReceived }
- [ ] **2.5.4** Feature flag system in poly-core's Cargo.toml

## 2.6 Demo Client — poly-demo

- [ ] **2.6.1** Implement `ClientBackend` for `DemoClient`
- [ ] **2.6.2** Generate random demo users (avatars, names, statuses)
- [ ] **2.6.3** Generate demo servers with categories and channels (text, voice, video)
- [ ] **2.6.4** Generate demo messages (various content types, timestamps)
- [ ] **2.6.5** Generate demo friend list and friend requests
- [ ] **2.6.6** Generate demo group chats (multi-user DMs)
- [ ] **2.6.7** Generate demo notifications
- [ ] **2.6.8** Fake event stream (periodic new messages, presence changes, etc.)
- [ ] **2.6.9** Demo "typing" indicators and other real-time effects

## 2.7 UI Implementation (~90%)

### 2.7.1 Setup Wizard (First Launch)
- [ ] **2.7.1.1** Welcome screen with Poly branding
- [ ] **2.7.1.2** Key generation step — generate Ed25519 keypair, show public key as user ID
- [ ] **2.7.1.3** Recovery phrase display — show BIP39 mnemonic, copy/export buttons
- [ ] **2.7.1.4** Recovery phrase confirmation step (optional)
- [ ] **2.7.1.5** Initialize SurrealKV, store key material
- [ ] **2.7.1.6** Redirect to main app

### 2.7.2 Main Layout Shell
- [ ] **2.7.2.1** Responsive layout: 4-column desktop (servers | channels | chat | users)
- [ ] **2.7.2.2** Mobile layout: 3 swipeable panels
- [ ] **2.7.2.3** Bottom user bar (avatar, username, settings gear)
- [ ] **2.7.2.4** Top bar: channel name, search, settings gear

### 2.7.3 Server Sidebar
- [ ] **2.7.3.1** DMs/Friends icon (top, always present)
- [ ] **2.7.3.2** Notifications icon (below DMs)
- [ ] **2.7.3.3** Server icon list (favorited servers)
- [ ] **2.7.3.4** Server icon with source badge overlay (top-left: backend logo)
- [ ] **2.7.3.5** Server icon with account badge overlay (bottom-right: account avatar)
- [ ] **2.7.3.6** Notification badges (unread count per server)
- [ ] **2.7.3.7** Server selection state (active indicator)
- [ ] **2.7.3.8** "Add Server to Favorites" action

### 2.7.4 DMs/Friends View
- [ ] **2.7.4.1** Search bar at top (search across all accounts/backends)
- [ ] **2.7.4.2** Favorited friends section
- [ ] **2.7.4.3** All conversations list (ordered by last message date)
- [ ] **2.7.4.4** Per-conversation: avatar, name, last message preview, timestamp, source badge
- [ ] **2.7.4.5** Multi-user groups (Discord groups, Teams chats, Matrix multi-DMs)
- [ ] **2.7.4.6** Friend request notifications

### 2.7.5 Channel List
- [ ] **2.7.5.1** Server name header with source icon + account info
- [ ] **2.7.5.2** Categories (collapsible)
- [ ] **2.7.5.3** Text channels (# icon)
- [ ] **2.7.5.4** Voice channels (🔊 icon, show connected users)
- [ ] **2.7.5.5** Video channels (📹 icon)
- [ ] **2.7.5.6** Unread indicators per channel
- [ ] **2.7.5.7** Channel selection state

### 2.7.6 Chat View
- [ ] **2.7.6.1** Message list (scrollable, loads more on scroll-up)
- [ ] **2.7.6.2** Message component: avatar, username, timestamp, content
- [ ] **2.7.6.3** Image/attachment rendering in messages
- [ ] **2.7.6.4** Message reactions
- [ ] **2.7.6.5** Typing indicator ("User is typing...")
- [ ] **2.7.6.6** Date separators between message groups
- [ ] **2.7.6.7** Message input area with send button
- [ ] **2.7.6.8** Message input: basic text, multiline support
- [ ] **2.7.6.9** Emoji picker (basic)
- [ ] **2.7.6.10** File/image upload button (stub)

### 2.7.7 User Sidebar (Right)
- [ ] **2.7.7.1** Channel member list
- [ ] **2.7.7.2** User entries: avatar, name, status, role/badge
- [ ] **2.7.7.3** Online/offline grouping
- [ ] **2.7.7.4** User profile popup on click

### 2.7.8 Notifications View
- [ ] **2.7.8.1** Aggregated notifications from all accounts/backends
- [ ] **2.7.8.2** Per-notification: source icon, account, message preview, timestamp
- [ ] **2.7.8.3** Mark as read, dismiss actions
- [ ] **2.7.8.4** Filter by backend/account

### 2.7.9 Settings Page
- [ ] **2.7.9.1** Settings navigation sidebar
- [ ] **2.7.9.2** **Accounts section**: list all accounts grouped by backend
- [ ] **2.7.9.3** **Per-account view**: server browser, favorite management, friend list (searchable with icons)
- [ ] **2.7.9.4** **Add account flow**: backend selector → login/auth flow
- [ ] **2.7.9.5** **Backup servers section**: list, add, remove backup servers
    - Per-server: URL, label, enabled/disabled on/off slider
    - Per-server status chip: Connected ✓ / Auth Required / Unreachable / Syncing…
    - Per-server: last synced timestamp, sequence number, token expiry countdown
    - Actions per server: Sync Now, Re-authenticate, Remove
    - Add server form: URL + label + passphrase input → trigger auth flow inline
- [ ] **2.7.9.6** **Identity section**: show public key (user ID), export recovery phrase
- [ ] **2.7.9.7** **Theme section**: preset selector, per-color editor, CSS editor with live preview, import/export
- [ ] **2.7.9.8** **Language section**: locale dropdown, immediate switch
- [ ] **2.7.9.9** **Appearance section**: dark/light mode, follow device toggle
- [ ] **2.7.9.10** **General section**: notification preferences, startup behavior

## 2.8 Backup Server — poly-backup-server

> See [phase-2.3-plan.md](phase-2.3-plan.md) for the full detailed sub-plan covering
> auth, token system, REST API (with utoipa/Swagger), Dioxus admin UI, and storage model.

**Summary checklist** (detail in phase-2.3-plan.md):

- [ ] **2.8.1** Axum + SurrealKV server setup, env-based config (`POLY_PASSPHRASE`, `POLY_MAX_ACCOUNTS`, etc.)
- [ ] **2.8.2** SurrealDB schema: accounts, tokens, sync_blobs tables
- [ ] **2.8.3** REST API: `POST /api/challenge` — issue PoW nonce
- [ ] **2.8.4** REST API: `POST /api/auth` — verify PoW + passphrase, issue token
- [ ] **2.8.5** REST API: `POST /api/sync/push` — store encrypted blob + sequence number
- [ ] **2.8.6** REST API: `GET /api/sync/pull?since={seq}` — return changes since sequence
- [ ] **2.8.7** REST API: `GET /api/sync/status` — return account info, token metadata
- [ ] **2.8.8** REST API: `DELETE /api/auth/token/{id}` — revoke a specific token (admin)
- [ ] **2.8.9** Token management: SHA-256 hash storage, last-seen update on each call, rolling expiry
- [ ] **2.8.10** Account management: enforce `POLY_MAX_ACCOUNTS`, track public keys
- [ ] **2.8.11** Rate limiting: per-IP counter, exponential backoff, `429 + Retry-After` on exceeded limit
- [ ] **2.8.12** utoipa + Swagger UI at `/swagger-ui` — full OpenAPI 3.1 spec for all endpoints
- [ ] **2.8.13** Dioxus web admin UI at `/` — accounts list, active sessions, server stats, revoke tokens
- [ ] **2.8.14** Docker image: `Dockerfile` + `docker-compose.yml` with env var documentation

---

## Phase 2 Completion Criteria

All of these must be true before moving to Phase 3:

- [ ] `dx serve --hotpatch` works with poly-core library changes (CRITICAL)
- [ ] All GitHub Actions pass on CI
- [ ] Demo client populates full UI with fake data
- [ ] Can navigate: servers → channels → messages → users
- [ ] Settings page: can add/view demo accounts, configure theme/language
- [ ] Setup wizard generates keys and shows recovery phrase
- [ ] Backup server launches and responds to sync API calls
- [ ] Encrypted settings round-trip: encrypt → push → pull → decrypt
- [ ] i18n works: can switch between EN/DE/FR/ES
- [ ] Theme switching works: neutral-dark, purple, red presets + custom CSS
- [ ] Mobile layout responsive with swipeable panels
- [ ] All .vscode launch profiles work on Linux
