# poly-core — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-06 (Phase 2.14 — WASM Plugin System + E2E Tests)

---

## Purpose

`poly-core` is **THE** shared library crate. It contains ALL shared UI components, state management, database logic, i18n, theming, crypto, and backup sync. Every app entry point (`apps/*`) depends on this crate.

**This is where you spend 90%+ of your development time.**

---

## CRITICAL: Hot Reload

This crate **MUST** support Dioxus subsecond hot-reload:
- Test with: `dx serve --hotpatch --package poly-desktop` from workspace root
- If hot-reload breaks, STOP all other work and fix it
- Use `subsecond::call()` for function-level hot-patching where needed
- All Dioxus components in `src/ui/` are automatically hot-patched
- Ensure the crate is a proper `lib` crate (not bin) — hot-reload only works on library code

### Hot Reload Verified (2026-02-28)

Tested and confirmed working:
- `dx serve --hotpatch --package poly-desktop` from workspace root
- Modified `poly-core/src/ui/mod.rs` → hot-patched in ~1.9 seconds
- App stays running, no restart needed
- **Note:** Must use `--package poly-desktop` flag — running `dx serve` from `apps/desktop/` alone doesn't work in workspace mode

## Module Structure

```
src/
├── lib.rs              # Library entry — exports all public modules
├── ui/                 # All Dioxus UI components
│   ├── mod.rs
│   ├── setup_wizard.rs # First-launch key generation flow
│   ├── main_layout.rs  # 4-column desktop layout shell
│   ├── favorites_sidebar.rs # Left server icon list
│   ├── voice_banner.rs  # Top-spanning voice connection banner
│   ├── account/         # Account-scoped UI components (multi-backend)
│   │   ├── mod.rs               # Re-exports + BackendType dispatch
│   │   ├── common/              # ★ Shared UI — used by ALL backends
│   │   │   ├── mod.rs
│   │   │   ├── account_bar.rs       # User info + mic/deafen controls
│   │   │   ├── account_server_bar.rs # Bar 2: DMs/Notifications/Servers nav
│   │   │   ├── account_switcher.rs  # Multi-account switcher in DM view
│   │   │   ├── channel_list.rs      # Channel/DM list
│   │   │   ├── chat_view.rs         # Message list + input
│   │   │   ├── emoji_picker.rs      # Emoji grid (reactions + input)
│   │   │   ├── friends_panel.rs     # Friends browser
│   │   │   ├── notifications.rs     # Aggregated notification feed
│   │   │   ├── user_sidebar.rs      # Right member list
│   │   │   ├── voice_bar.rs         # Persistent voice status bar
│   │   │   └── voice_view.rs        # Voice/video participant tiles
│   │   ├── demo/                # Demo backend UI overrides (#[cfg(feature="demo")])
│   │   │   ├── mod.rs
│   │   │   └── context_menu.rs
│   │   ├── stoat/               # Stoat backend UI overrides (#[cfg(feature="stoat")])
│   │   │   ├── mod.rs
│   │   │   └── context_menu.rs
│   │   ├── discord/             # Discord backend UI overrides (#[cfg(feature="discord")])
│   │   │   ├── mod.rs
│   │   │   └── context_menu.rs
│   │   ├── matrix/              # Matrix backend UI overrides (#[cfg(feature="matrix")])
│   │   │   ├── mod.rs
│   │   │   └── context_menu.rs
│   │   ├── teams/               # Teams backend UI overrides (#[cfg(feature="teams")])
│   │   │   ├── mod.rs
│   │   │   └── context_menu.rs
│   │   ├── poly_native/         # Poly native server overrides (always compiled)
│   │   │   ├── mod.rs
│   │   │   └── context_menu.rs
│   │   ├── server/              # Server-scoped components
│   │   │   ├── mod.rs
│   │   │   ├── context_menu.rs  # Dispatches to per-backend menus
│   │   │   └── settings/
│   │   └── settings/            # Account-scoped settings (notifications only)
│   │       ├── mod.rs           # AccountSettingsPage
│   │       └── notifications.rs # Per-account notification toggles
│   ├── settings/        # App-level settings page
│   │   ├── mod.rs       # SettingsPage (accounts/backup/identity/theme/language/general)
│   │   ├── accounts.rs  # Account management
│   │   ├── backup.rs    # Backup server config
│   │   ├── common.rs    # PolySelect, SelectOption helpers
│   │   ├── general.rs   # Reset / nuke
│   │   ├── identity.rs  # Key/mnemonic management
│   │   ├── language.rs  # Locale selector
│   │   ├── theme.rs     # Theme editor + presets
│   │   └── voice_video.rs # Voice & video device settings
│   └── routes.rs        # Dioxus router definition
│
├── state/              # App state management (Dioxus Stores)
│   ├── mod.rs
│   ├── app_state.rs    # Global app state
│   ├── accounts.rs     # Account state per backend
│   ├── servers.rs      # Favorites, server data
│   ├── messages.rs     # Message cache/state
│   └── navigation.rs   # Current view, selected server/channel
│
├── db/                 # SurrealDB abstraction
│   ├── mod.rs
│   ├── init.rs         # SurrealKV initialization
│   ├── settings.rs     # Settings CRUD
│   ├── accounts.rs     # Account credential storage
│   ├── favorites.rs    # Favorites storage
│   └── migrations.rs   # Schema migration system
│
├── i18n/               # Internationalization
│   ├── mod.rs
│   ├── engine.rs       # fluent-bundle wrapper
│   └── macros.rs       # t!() macro
│
├── theme/              # Theme engine
│   ├── mod.rs
│   ├── engine.rs       # CSS variable management
│   ├── presets.rs      # Built-in theme presets
│   └── editor.rs       # Custom CSS model
│
├── crypto/             # Cryptography
│   ├── mod.rs
│   ├── identity.rs     # Ed25519/X25519 key generation
│   ├── mnemonic.rs     # BIP39 mnemonic encode/decode
│   └── encrypt.rs      # Encrypt/decrypt helpers
│
└── sync/               # Backup server sync client
    ├── mod.rs
    ├── client.rs       # HTTP client for backup server API
    ├── pow.rs          # Proof-of-work challenge solver
    └── protocol.rs     # Push/pull encrypted blobs
```

## Key Dependencies

- `dioxus = "0.7.3"` — UI framework
- `surrealdb = "3.0.1"` (feature: kv-surrealkv) — local database
- `fluent-bundle` — i18n engine
- `ed25519-dalek` — identity key generation
- `x25519-dalek` — key exchange / encryption derivation
- `bip39` — mnemonic seed phrases
- `serde`, `serde_json` — serialization
- `reqwest` — HTTP client for backup server sync
- `tokio` — async runtime

## Feature Flags (in this crate's Cargo.toml)

```toml
[features]
default = ["demo"]
stoat = ["dep:poly-stoat"]
matrix = ["dep:poly-matrix"]
discord = ["dep:poly-discord"]
teams = ["dep:poly-teams"]
demo = ["dep:poly-demo"]
all-backends = ["stoat", "matrix", "discord", "teams", "demo"]
```

## Design Rules

1. **All UI components here** — apps are thin wrappers calling `poly_core::App`
2. **All strings through i18n** — use `t!("key")`, never hardcode English
3. **State via Dioxus Stores** — derive `Store` on state structs
4. **Async via Tokio** — all backend operations are async
5. **Client backends loaded via `poly-client` trait** — don't import concrete client types directly; use the trait interface

## ABSOLUTE PROHIBITION — Never Hardcode Demo/Test Data in UI

**NEVER** create inline `User`, `VoiceParticipant`, `Message`, `Server`, or any other data
struct directly inside UI component code (including RSX event handlers). This includes:
- Constructing a fake user struct to "test" a feature (`User { id: "demo-user-self", ... }`)
- Calling `poly_demo::data::*` functions directly from UI components
- Hardcoding backend-specific IDs like `"demo-user-self"` in UI disconnect handlers

**All data must flow from the `ClientBackend` trait.** If a real backend would fetch data from
a server API, the demo client must also implement that API method — returning static demo data.
UI components should never know whether they're talking to a real or demo backend.

**Correct pattern:**
```rust
// In ClientBackend trait:
async fn get_voice_participants(&self, channel_id: &str) -> ClientResult<Vec<VoiceParticipant>>;

// In DemoClient:
async fn get_voice_participants(&self, channel_id: &str) -> ClientResult<Vec<VoiceParticipant>> {
    Ok(data::demo_voice_participants(channel_id)) // data lives in poly-demo, not UI
}

// In UI (voice_view.rs):
let participants = backend.get_voice_participants(&channel_id).await?; // API call, not hardcoded
```

**When you need to add a new data type/query to the UI:**
1. Add the method to `ClientBackend` trait in `clients/client/src/lib.rs`
2. Implement it in all existing clients (stubs return empty/defaults for real backends)
3. Implement it in `DemoClient` using `data::*` functions in `clients/demo/src/data.rs`
4. Call it from the UI through `ClientManager` — never directly from `poly_demo`

**Rationale:** This was violated in March 2026 when `VoiceParticipant` was hardcoded in `voice_view.rs`.
The fix added `get_voice_participants` to the trait and moved all demo data to `poly-demo::data`.

## Dioxus Component Size Limits — MANDATORY

**NEVER create RSX components larger than 150 lines of code.** This is a hard limit, not a guideline.

When a component approaches 150 lines:
- **Extract sub-components immediately** — split rendering logic into smaller, testable components
- **Move event handlers to separate helper functions** — async logic should live outside RSX
- **Use const helper functions for repeated rendering patterns** — `const fn render_status_chip(...) -> String`
- **Max nesting depth is 4 levels** — if your RSX has `div > div > div > div > div`, extract a component
- **Inline conditionals (`if/else` in RSX) should be short** — complex logic belongs in Rust, not interpolated in markup

**Why this matters:**
- Unmaintainable components hide bugs and make hot-reload harder to debug
- Large RSX with nested loops + signal updates = silent state sync bugs
- Developers cannot maintain giant RSX blobs
- Easier testing: small components have testable inputs/outputs

**Bad example:**
```rust
#[component]
fn BackupSettings() -> Element {
    let mut servers = use_signal(Vec::new);
    rsx! {
        div { /* ... 500+ lines of forms, lists, loops, conditionals ... */ }
    }
}
```

**Good example:**
```rust
#[component]
fn BackupSettings() -> Element {
    rsx! {
        div { class: "settings-section",
            h2 { "Backup Servers" }
            ServerList { }
            AddServerButton { }
        }
    }
}

#[component]
fn ServerList() -> Element {
    rsx! { /* ~30 lines */ }
}

#[component]
fn ServerRow(record: ServerRecord) -> Element {
    rsx! { /* ~20 lines */ }
}
```

## CRITICAL: `#[rustfmt::skip]` on ALL `#[component]` Functions

⚠️ **EVERY `#[component]` function MUST have `#[rustfmt::skip]` on the line immediately before it.**

```rust
#[rustfmt::skip]  // <- REQUIRED: prevents fmt from mangling RSX macros
#[component]
fn MyComponent() -> Element {
    rsx! { /* ... */ }
}
```

**Why:** `cargo fmt` corrupts Dioxus RSX macros by mangling multi-line closures in event handlers.
It creates invalid Rust syntax with duplicated conditional branches and unmatched braces. Example:

```rust
// BEFORE fmt (correct):
onchange: move |e: Event<FormData>| {
    let val = e.value();
    chat_data.write().item = if val.is_empty() { None } else { Some(val) };
},

// AFTER fmt (BROKEN):
onchange: move |e: Event<FormData>| {
    let val = e.value();
    chat_data.write().item =
        if val.is_empty() { None } else { Some(val) };
        None          // <- CORRUPTED: duplicated if/else logic
    } else {
        Some(val)
    };  // <- Syntax error: unmatched closing brace
},
```

**What to do:**
1. Always write `#[rustfmt::skip]` before `#[component]`
2. Save the file — fmt will leave RSX alone
3. If you forget and fmt corrupts a component, fix it by: (a) restoring from git, (b) adding `#[rustfmt::skip]`, (c) re-making your changes
4. If a component's RSX is too complex for fmt to handle even with `#[rustfmt::skip]`, it's a sign the component is > 150 lines and needs refactoring

**Status:** As of 2026-03-08, this is being rolled out across all poly-core components.
**Enforcement:** `cargo cranky` already enforces 100-line limits per component.

## Testing

- Unit tests for crypto, db, i18n modules
- Integration tests with demo client for UI state flows
- Hot-reload smoke test: modify a component, verify it updates

## Chat Shell Layout Rule (2026-03-07)

- The chat header must span the full width of the chat shell, including when the right-side
  member/thread/pinned/contact rail is open.
- Implement this by keeping the header above a dedicated `.chat-body-shell` split. The right rail
  (`.chat-side-column`) must be a sibling of the message/content column inside the body, **not** a
  sibling of the entire `.chat-main-column`.
- Reason: if the rail is attached to the outer shell, opening it shrinks the header and pulls the
  inline search box left, which diverges from the Discord-style layout Poly is matching.

## Chat History Loading Rule (2026-03-08)

- Text chats must open on a **bounded recent window**, not the full history.
- Use `MessageQuery { limit: Some(...) }` for initial loads, scaling the limit high enough to
  include unread context (`unread_count + small buffer`) but still keeping the window recent.
- Scrolling near the top of `.message-list` must fetch older history with `before: first_loaded_id`
  and prepend it while preserving the user's viewport offset.
- The unread UX for server/DM text chats should mirror Discord closely:
  - sticky top unread banner
  - inline unread divider at the first unread message
  - initial open lands at the bottom of the recent window
- `chat_history.rs` is the shared helper module for these rules; do not reintroduce raw
  `MessageQuery::default()` initial loads for chat entry points.

## MANDATORY: Visual Testing with MCP desktop-devtools

**After every change that touches `rsx!` blocks** (UI layout, component structure, new
components, theme changes, etc.), you MUST visually verify the result using the
desktop-devtools MCP:

```
1. Ensure desktop app is running (hot-reload or launch fresh via MCP)
2. Call mcp_poly-desktop_screenshot() to capture the current state
3. Inspect the screenshot for correctness (layout, text, colors)
4. If resetting to a clean state is useful: call mcp_poly-desktop_reset_app()
   then mcp_poly-desktop_launch_app() to walk through the setup wizard fresh
5. Navigate to the changed area: mcp_poly-desktop_navigate("/path")
6. Take another screenshot to confirm the change looks correct
```

**If the visual test reveals problems**: fix them before declaring the task complete.
A change is only "done" when it looks correct in the actual running app.

This applies especially to:
- New settings pages / merged pages
- Theme editor components (color pickers, CSS editor, preset buttons)
- Layout changes (does it still work on narrow viewports aka "mobile"?)
- Any component that was split from a large one

---

## Storage Abstraction — `src/storage/` (Implemented 2025-03-01)

### Architecture

```
src/storage/
├── mod.rs          # Storage newtype + typed helpers (AppSettings, AccountToken, etc.)
├── native.rs       # Native backend: SurrealDB 3.0 + SurrealKV (non-WASM)
└── web.rs          # WASM backend: gloo-storage LocalStorage
```

A global `STORAGE: OnceLock<Storage>` in `lib.rs` is initialized once at app startup
via a `use_future` in the `App` component. All storage access goes through it.

### Critical SurrealDB 3.0 Query Patterns (HARD WON LESSONS)

**DO NOT** use the typed SDK (`db.select()`, `db.upsert()`, `db.delete()`) with custom
structs — these require `#[derive(SurrealValue)]` from `surrealdb-types-derive`, an
**internal** proc-macro crate not exposed to downstream users.

**USE** raw `.query()` with careful `take` calls:

```rust
// Correct bind pattern — serde_json::Value: SurrealValue → inferred as IntoVariables
db.query("UPSERT poly_kv:key SET payload = $payload")
  .bind(serde_json::json!({ "payload": "value_string" }))
  .await?;

// Correct take pattern — must use turbofish, usize literal for index
let raw: Option<String> = resp.take::<Option<String>>("payload")?;
let result: Option<serde_json::Value> = resp.take::<Option<serde_json::Value>>(0usize)?;
```

**Key caveats:**
- Field named `payload` (NOT `value`) — `VALUE` is a SurrealQL keyword, using it as a
  field name in queries causes silent failures
- `.bind(("key", reference))` FAILS if the reference type doesn't implement `SurrealValue`
  (`&String` does NOT, `String` DOES, `serde_json::Value` DOES)
- `take(0)` fails with type inference — always turbofish: `take::<Option<T>>(0usize)`
- `.query()` returning a `Response` does NOT propagate SurrealQL errors via `?` — you
  MUST call `.take()` on the response to surface any query-level errors
- `IntoVariables` is only implemented for `T: SurrealValue` — passing `("key", T)` only
  works if the tuple produces a `Value::Array` → entries treated as K-V pairs

### Storage Schema

Table `poly_kv` in SurrealDB namespace `poly` / database `main`:
- Record ID: `poly_kv:<key>` (e.g. `poly_kv:app_settings`, `poly_kv:account_tokens`)
- Field `payload`: `String` — double-serialized JSON (matches WASM localStorage approach)

### Platform Path

- Linux: `$XDG_DATA_HOME/poly/storage.db` or `~/.local/share/poly/storage.db`
- macOS: `~/Library/Application Support/poly/storage.db`
- Windows: `%APPDATA%\poly\storage.db`

### Persistence Verified

MCP self-test (2025-03-01): wizard completion → kill → relaunch → wizard skipped ✓
WAL grew from 1592 bytes (init-only) to 3925 bytes (init + data write), then read back on new session.

## Phase 2.4 Additions (2026-03-01)

### Crypto — `src/crypto/mod.rs`

Real ChaCha20-Poly1305 encryption (replaced placeholder base64):

```rust
// Encrypt: nonce(12 bytes) || ciphertext+tag
pub fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, CryptoError>

// Decrypt: strips nonce, decrypts, verifies auth tag
pub fn decrypt(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, CryptoError>
```

Key points:
- Uses `chacha20poly1305` crate (RustCrypto ecosystem)
- Nonce is 96-bit random (OsRng), 12 bytes prepended to ciphertext
- `.get(..12)` / `.get(12..)` used instead of indexing to satisfy `clippy::indexing_slicing`

### Storage — Backup Server Records + Identity Key

New methods on `Storage` in `src/storage/mod.rs`:

```rust
// Backup server records (keyed by URL)
pub async fn get_backup_servers(&self) -> Result<Vec<BackupServerRecord>, StorageError>
pub async fn upsert_backup_server(&self, record: &BackupServerRecord) -> Result<(), StorageError>
pub async fn remove_backup_server(&self, url: &str) -> Result<(), StorageError>

// Identity key (Ed25519 private key bytes, 32 bytes, hex-encoded in DB)
pub async fn get_identity_key(&self) -> Result<Option<[u8; 32]>, StorageError>
pub async fn set_identity_key(&self, key: &[u8; 32]) -> Result<(), StorageError>
pub async fn delete_identity_key(&self) -> Result<(), StorageError>
```

`BackupServerRecord` fields: `url`, `label`, `enabled`, `last_sequence`, `token` (Option),
`token_expires_at` (Option<String> RFC3339), `last_synced_at` (Option<String> RFC3339).

### Sync Client — `src/sync/mod.rs`

Protocol-aligned with actual backup server:

```rust
pub struct SyncClient { base_url: String, public_key_hex: String, private_key: [u8; 32] }

impl SyncClient {
    // Full PoW auth: challenge → mine SHA-256 → submit → receive token
    pub async fn authenticate(&self, passphrase: &str, device_name: &str) -> Result<String, SyncError>
    // Push encrypted blob → returns sequence number
    pub async fn push(&self, token: &str, data: &[u8]) -> Result<i64, SyncError>
    // Pull blobs since sequence → returns Vec<(sequence, data)>
    pub async fn pull(&self, token: &str, since: i64) -> Result<Vec<(i64, Vec<u8>)>, SyncError>
    // Get account status
    pub async fn status(&self, token: &str) -> Result<SyncStatus, SyncError>
}
```

PoW: `SHA-256(nonce + counter.to_string())`, check leading zero bits with `difficulty` count.

### Settings UI — `src/ui/settings.rs`

Two new components:
- `BackupSettings` — server list, add form (URL + label + passphrase), inline auth flow,
  status chips (connected/auth-required/syncing/unreachable), sync-now, re-auth, remove
- `IdentitySettings` — public key display with copy, "Export Recovery Phrase" modal (24-word grid)

### Setup Wizard — `src/ui/setup_wizard.rs`

Key generation step added:
- `Identity::generate()` → `(public_key, private_key: [u8; 32])`
- Stores `private_key_bytes: Signal<Option<[u8; 32]>>` during wizard
- On wizard complete: `spawn(async { storage.set_identity_key(&key).await })`
- Recovery phrase step shows all 24 words; copy-to-clipboard via `document::eval()` + JS

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.

## CRITICAL: Dioxus Asset Path Symlink (DECISION D14)

The `asset!("assets/tailwind.css")` macro uses the Cargo **package name** (`poly-core`)
to build the serve URL, not the directory name (`core`). This means:

- URL generated: `dioxus://…/crates/poly-core/assets/tailwind.css`
- Physical path: `crates/core/assets/tailwind.css`

The symlink `crates/poly-core -> crates/core` MUST exist and be committed to git.
Without it, the desktop app loads 0 CSS rules and renders as a white page.

Web app uses a hashed asset URL served from its `dist/` tree — not affected by this, but
both apps share the same `poly-core` package so the symlink fixes both build paths.

## CRITICAL: `storage/web.rs` — Unit struct, no unsafe Send/Sync needed

`StorageInner` is a zero-size unit struct. Rust automatically implements `Send + Sync`
for unit structs. **Never** add `unsafe impl Send`/`Sync` — it is denied by `unsafe_code`
and is redundant.

## CRITICAL: `ui/settings.rs` Brace Matching

The backup settings UI is deeply nested (Dioxus RSX inside async closures inside onclick
handlers). Brace mismatches only show up in the **WASM build** — `cargo cranky --workspace`
(which targets the host) may pass while `cargo build -p poly-web --target wasm32-unknown-unknown`
fails. **Always run both checks after editing settings.rs.**

When fixing brace issues, prefer `.map()` over `if let Some(x) = ...` + `drop(x)` when
you need to mutate a vec element and then move the vec.

## MANDATORY: Visual Verification with Desktop DevTools MCP

**After EVERY change to this crate**, you MUST verify the changes using the Desktop DevTools MCP.
Do NOT declare any change complete without visual confirmation.

**Verification checklist:**
1. `cargo check --workspace` — error-free
2. `cargo cranky --workspace` — zero warnings
3. `cargo check -p poly-web --target wasm32-unknown-unknown` — WASM compat
4. `dx build --platform desktop` in `apps/desktop-devtools/` — build must succeed
5. `mcp_poly-desktop_launch_app` → `mcp_poly-desktop_connect_cdp`
6. `mcp_poly-desktop_screenshot` — enable 🧪 demo, navigate to affected views
7. Click interactive elements (buttons, pickers, navigation) to verify behavior
8. Fix any visual/layout issues before declaring done

**Lesson learned (2025-03-01):** RSX macro syntax errors cause misleading Rust diagnostics.
Two syntax bugs (a `},` instead of `;` and a misplaced closing brace before `else`) passed
`cargo cranky --workspace` but would have produced broken runtime behavior. Always verify visually.

## Phase 2.5 New Components (Verified 2025-03-01)

| Component | File | Purpose |
|---|---|---|
| `VoiceChannelView` | `ui/account/voice_view.rs` | Full voice channel view with participant tiles |
| `VoiceBar` | `ui/account/voice_bar.rs` | Persistent voice connection bar (bottom of channel list) |
| `EmojiPicker` | `ui/account/emoji_picker.rs` | Emoji grid picker (reactions + input) |
| `AccountBar` | `ui/account/account_bar.rs` | User info + mic/deafen controls at bottom |

**State additions:**
- `ChatData`: `voice_channel_participants: HashMap<String, Vec<VoiceParticipant>>`, `voice_connection: Option<VoiceConnection>`
- `AppState`: nav history stack with `push_nav_history()`, `nav_back()`, `nav_forward()`, `can_go_back()`, `can_go_forward()`
- `NavigationState`: now derives `PartialEq, Eq`

**Visually confirmed working (2025-03-01):**
- Voice participant tiles (muted 🔇, deafened 🔕, streaming 🖥, video 📹 icons)
- Join Voice / Disconnect toggle + voice bar persistence across navigation
- Emoji picker opens above input, emoji selection inserts into textarea
- Reaction pills on messages, input toolbar (😀 GIF 📎)
- Voice participants listed in channel list under voice channels

## UI Account Module Refactor (Session 2025)

**Architectural decision:** All account-scoped UI components were moved from flat
`src/ui/` to `src/ui/account/`. App-level chrome (FavoritesBar, VoiceBanner,
MainLayout, SetupWizard) stays at `src/ui/`.

**Key changes:**
- `src/ui/account/` — new home for 11 account-scoped components
- `src/ui/account/settings/` — account-scoped settings (notifications ONLY)
- `src/ui/account/settings::AccountSettingsPage` — replaces the `is_account_scoped`
  flag that previously parameterized `SettingsPage`
- `src/ui/settings::SettingsPage` now takes **no props** and is app-level only
- `AccountSettingsRoute` in routes.rs uses `AccountSettingsPage`, not `SettingsPage`
- `settings/account/` subdirectory removed (content moved to `account/settings/`)

**Rule:** When adding new account-scoped components, put them in `src/ui/account/`.
When adding app-level chrome, put it at `src/ui/`. Never mix the two.

## WASM Plugin System — D21/D22 (2026-03-06)

### D21: All Backends Are WASM Plugins

All 6 messenger backends now build as WASM Component Model plugins. poly-core no longer contains
the plugin implementation code directly — backends are loaded at runtime from `.wasm` files.

### D22: Plugin Host Extraction (Dynamic Linking)

The plugin host (wasmtime runtime + WIT bindings + host-api implementation) was extracted to
`crates/plugin-host/` as a `crate-type = ["dylib"]`. poly-core re-exports it:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub use poly_plugin_host as plugin_host;
```

**Impact on poly-core development:**
- Changes to poly-core **never recompile wasmtime** (saves ~2 minutes per iteration)
- `crates/core/src/plugin_host/` directory was deleted — now lives in `crates/plugin-host/`
- Web builds (wasm32-unknown-unknown) exclude the re-export via cfg gate

### E2E Test Coverage

**77 tests** in `crates/plugin-host-tests/` validate the full ClientBackend interface through
the WASM plugin host:

```sh
cargo test -p poly-plugin-loader-tests --all-features -- --nocapture
```

## Session Notes — 2026-03-07

### ServerBanner Rewrite (Phase 2 — UI polish)

Rewrote `ServerBanner` in `channel_list.rs` to Discord-style:
- Optional `banner_url` image at top (full-width)
- Clickable server name button with ▾/▴ chevron toggling a dropdown  
- Dropdown includes: Server Settings (navigates to `Route::ServerSettingsRoute`), Invite People (placeholder), Notification Settings (placeholder), Leave Server (placeholder)
- Invite `+` icon button on the right of the header bar (placeholder)
- Uses `.context-menu-backdrop` pattern from phase-2.10 for click-outside-close
- CSS classes: `.server-banner-header`, `.server-name-trigger`, `.server-name-chevron`, `.server-invite-btn`, `.server-dropdown-menu`, `.server-dropdown-item`, `.server-dropdown-item-danger`

### F5 URL Restoration Fix

`ServerChat` and `ServerHome` components in `routes.rs` now have `use_effect` that:
- Detects when `chat_data.current_channel/server` doesn't match the URL (F5/deep-link scenario)
- Calls `restore_server_channel()` / `load_server_data()` from `favorites_sidebar.rs`
- These async fns load server info, channels, messages, and members from the backend

### Chat Scroll Fix

`ChatView` scroll-to-bottom effect now:
1. Reads `chat_data` signal INSIDE the closure (creates reactive dependency)
2. Wraps scroll in `requestAnimationFrame` to ensure DOM is painted first

### Demo Event Streaming

- `toggle_demo` now accepts `app_state: Signal<AppState>` parameter (updated call sites in `favorites_sidebar.rs` and `mod.rs`)
- `spawn_event_stream_listener` launched for both demo-cat and demo-dog after demo activation
- Live presence updates and messages appear in real-time (~4-8s intervals)

### Server.banner_url Field

Added `banner_url: Option<String>` with `#[serde(default)]` to `Server` struct in `poly-client`. Updated bridge.rs, server-client/backend.rs, and all 7 demo Server constructors with `banner_url: None`.
