# poly-core — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-01 (Phase 2.4)

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
│   ├── app.rs          # Root App component
│   ├── setup_wizard.rs # First-launch key generation flow
│   ├── main_layout.rs  # 4-column desktop layout shell
│   ├── mobile_layout.rs # 3-panel swipe mobile layout
│   ├── server_sidebar.rs # Left server icon list
│   ├── channel_list.rs  # Channel list for selected server
│   ├── chat_view.rs     # Message list + input
│   ├── user_sidebar.rs  # Right user list
│   ├── dm_view.rs       # DMs/Friends aggregated view
│   ├── notifications.rs # Notification feed
│   ├── settings/        # Settings page components
│   │   ├── mod.rs
│   │   ├── accounts.rs  # Account management
│   │   ├── backup.rs    # Backup server config
│   │   ├── identity.rs  # Key/mnemonic management
│   │   ├── theme.rs     # Theme editor + presets
│   │   ├── language.rs  # Locale selector
│   │   └── appearance.rs # Dark/light mode
│   └── components/      # Reusable UI primitives
│       ├── mod.rs
│       ├── message.rs   # Single message component
│       ├── server_icon.rs # Server icon with badges
│       ├── user_avatar.rs # User avatar with status
│       └── search_bar.rs # Reusable search input
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

## Testing

- Unit tests for crypto, db, i18n modules
- Integration tests with demo client for UI state flows
- Hot-reload smoke test: modify a component, verify it updates

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
