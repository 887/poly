# Phase 2.4 Plan — Backup Client + Settings UI + Crypto

> **Status:** ✅ Complete
> **Parent:** [Phase 2 Plan](phase-2-plan.md)
> **Depends On:** [Phase 2.3 Plan](phase-2.3-plan.md) — backup server must be running
> **Overall Context:** [Overall Plan §5](overall-plan.md#5-backup-server-architecture)

---

## Overview

This phase completes:
1. **Real cryptography** — replace placeholder base64 with ChaCha20-Poly1305
2. **Backup sync client alignment** — align `poly-core` sync client protocol with actual `poly-backup-server` API
3. **`BackupServer` storage model** — store per-server state in SurrealKV
4. **Full backup settings UI** — server list, add form, inline auth, status chips, sync-now
5. **Identity settings UI** — show public key, export recovery phrase
6. **Setup wizard key generation** — Ed25519 keygen, mnemonic display in wizard
7. **End-to-end protocol test** — integration test that exercises the full sync flow

---

## 2.4.1 Real Encryption (ChaCha20-Poly1305)

> **Crate:** `crates/core/src/crypto/mod.rs`
> **Dependency:** `chacha20poly1305` crate (RustCrypto)

- [x] **2.4.1.1** Add `chacha20poly1305` to workspace `Cargo.toml` (latest version)
- [x] **2.4.1.2** Add `chacha20poly1305` to `crates/core/Cargo.toml` dependencies
- [x] **2.4.1.3** Implement `encrypt(plaintext: &[u8], key: &[u8; 32]) -> Vec<u8>`:
  - Generate 96-bit random nonce (12 bytes) via `OsRng`
  - Encrypt with `ChaCha20Poly1305` using the key and nonce
  - Return `nonce (12 bytes) || ciphertext+tag` concatenated
- [x] **2.4.1.4** Implement `decrypt(ciphertext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, CryptoError>`:
  - Split first 12 bytes as nonce
  - Decrypt remainder with `ChaCha20Poly1305`
  - Return error if auth tag fails
- [x] **2.4.1.5** Update `test_encrypt_decrypt_roundtrip` to verify real encryption
- [x] **2.4.1.6** Verify `cargo test -p poly-core` passes

---

## 2.4.2 Sync Client Protocol Alignment

> **Crate:** `crates/core/src/sync/mod.rs`
>
> The existing sync client used a different PoW protocol than the actual backup server.
> The backup server uses:
> - `POST /api/challenge` with `{ public_key }` → `{ nonce, difficulty, expires_at }`
> - PoW hash: `SHA-256(nonce_str + counter_decimal_str)` with leading zero bits check
> - `POST /api/auth` with `{ public_key, nonce, counter, passphrase, device_name }`
> - `GET /api/sync/pull?since=<seq>` and `POST /api/sync/push`

- [x] **2.4.2.1** Replace `PowChallenge` struct with `{ nonce: String, difficulty: u32, expires_at: String }`
- [x] **2.4.2.2** Replace `PowSolution` struct with `{ nonce: String, counter: u64 }`
- [x] **2.4.2.3** Replace `AuthRequest` struct to match server: `{ public_key, nonce, counter, passphrase, device_name }`
- [x] **2.4.2.4** Fix `solve_pow()` to use `SHA-256(nonce + counter.to_string())` matching `verify_pow()` on server
- [x] **2.4.2.5** Fix `SyncClient::request_challenge()` to `POST /api/challenge` with `{ public_key }`
- [x] **2.4.2.6** Fix `SyncClient::authenticate()` to send aligned `AuthRequest`
- [x] **2.4.2.7** Fix `SyncClient::push()` to send `data` as base64-encoded string (server expects JSON body)
- [x] **2.4.2.8** Fix `SyncClient::pull()` to deserialize `[{ sequence, data, timestamp }]`
- [x] **2.4.2.9** Add `SyncClient::status()` — `GET /api/sync/status` → server account info

---

## 2.4.3 BackupServer Storage Model

> **Crate:** `crates/core/src/storage/mod.rs`

- [x] **2.4.3.1** Add `BackupServerRecord` struct:
  ```rust
  pub struct BackupServerRecord {
      pub url: String,
      pub label: String,
      pub enabled: bool,
      pub last_sequence: u64,
      pub token: Option<String>,
      pub token_expires_at: Option<String>,
      pub last_synced_at: Option<String>,
  }
  ```
- [x] **2.4.3.2** Add `get_backup_servers() -> Result<Vec<BackupServerRecord>>`
- [x] **2.4.3.3** Add `upsert_backup_server(record: &BackupServerRecord) -> Result<()>` — keyed by URL
- [x] **2.4.3.4** Add `remove_backup_server(url: &str) -> Result<()>`

---

## 2.4.4 Backup Settings UI

> **Crate:** `crates/core/src/ui/settings.rs` — `BackupSettings` component
>
> See plan item **2.7.9.5** in phase-2-plan.md.

- [x] **2.4.4.1** `BackupSettings` — load server list from storage on mount
- [x] **2.4.4.2** Server list row: URL label, enabled toggle, status chip, last synced, actions
- [x] **2.4.4.3** Status chips: `Connected ✓` (green), `Auth Required` (yellow), `Unreachable` (red), `Syncing…` (blue)
- [x] **2.4.4.4** Per-server actions: Sync Now button, Re-authenticate button, Remove button
- [x] **2.4.4.5** Add server form (inline expand): URL input, label input, passphrase input → triggers auth flow
- [x] **2.4.4.6** Add server auth flow: requests PoW challenge, solves it (in `spawn()`), submits auth, shows token status
- [x] **2.4.4.7** i18n strings for all backup UI labels
- [x] **2.4.4.8** `Sync Now` triggers `SyncClient::push()` with current encrypted settings blob

---

## 2.4.5 Identity Settings UI

> **Crate:** `crates/core/src/ui/settings.rs` — `IdentitySettings` component
>
> See plan item **2.7.9.6** in phase-2-plan.md.

- [x] **2.4.5.1** Load identity from storage on mount (account_id from `AppSettings`)
- [x] **2.4.5.2** Display account ID (hex public key) in a monospace code block with copy button
- [x] **2.4.5.3** "Export Recovery Phrase" button — loads identity from storage, generates mnemonic, displays in modal
- [x] **2.4.5.4** Modal shows 24-word phrase in grid layout, copy-all button, close button
- [x] **2.4.5.5** i18n strings for identity section

---

## 2.4.6 Setup Wizard Key Generation

> **Crate:** `crates/core/src/ui/setup_wizard.rs`
>
> See plan items **2.7.1.2 — 2.7.1.5** in phase-2-plan.md.

- [x] **2.4.6.1** Wizard step 2: Key Generation — `Identity::generate()`, store private key bytes
- [x] **2.4.6.2** Display public key (account ID) to user as their Poly ID
- [x] **2.4.6.3** Wizard step 3: Recovery Phrase Display — show 24-word BIP39 mnemonic
- [x] **2.4.6.4** Copy and export-to-file buttons for recovery phrase (export via `rfd` file dialog)
- [x] **2.4.6.5** Store identity (private key bytes + account_id) in SurrealKV via storage module

---

## 2.4.7 End-to-End Protocol Test

> **Location:** `servers/backup-server/tests/e2e_protocol_test.rs`
>
> Integration test that starts the backup server in-process and exercises the full client flow.

- [x] **2.4.7.1** Start backup server with test config (low PoW difficulty=4, known passphrase)
- [x] **2.4.7.2** Generate test identity (Ed25519 keypair)
- [x] **2.4.7.3** Full auth flow: challenge → PoW solve → authenticate → receive token
- [x] **2.4.7.4** Push encrypted settings blob
- [x] **2.4.7.5** Pull back the blob, verify sequence numbers
- [x] **2.4.7.6** Decrypt blob, verify round-trip matches original data
- [x] **2.4.7.7** Test token expiry re-auth path (simulate expired token → 401 → re-auth)
- [x] **2.4.7.8** Test admin API: stats, accounts list, token revocation

---

## Phase 2.4 Completion Criteria

- [x] `cargo test -p poly-core` — crypto tests pass with real ChaCha20-Poly1305
- [x] `cargo test -p poly-backup-server` — E2E protocol test passes (10/10)
- [x] `cargo cranky --workspace` — zero lint warnings
- [x] Backup settings UI renders with server list, add form, status chips
- [x] Identity settings UI shows public key and mnemonic export modal
- [x] Setup wizard generates and displays real keypair + recovery phrase

---

## Session Log

### 2026-03-01 — Session 2: SurrealDB datetime Fix + E2E Tests Passing

**Root Cause Fixed:**
- SurrealDB 3.0.x with `kv-surrealkv` cannot deserialize `TYPE datetime` fields to `serde_json::Value`.
  Error: "Expected any, got datetime". Applies to ALL datetime fields in `SELECT *` results.

**Fix Applied:**
- Changed all `TYPE datetime` schema fields to `TYPE string` in `db.rs`
- Removed all `DEFAULT time::now()` clauses from schema
- Updated all CREATE/UPDATE SurrealQL queries in `auth/mod.rs` and `sync/mod.rs` to bind
  `Utc::now().to_rfc3339()` as `$now` instead of using `time::now()` in SurrealQL
- Changed all `SELECT *` queries to explicit column lists to avoid unexpected datetime fields
- Removed `AND expires_at > time::now()` from WHERE clauses (can't compare string to datetime);
  do expiry checking in Rust using `chrono::DateTime::parse_from_rfc3339()`
- Updated `web/mod.rs` admin API token listing query to use `$now` string comparison

**E2E Test Fixes:**
- Fixed `PushRequest` struct field name: `data` → `encrypted_blob` (mismatched server schema)
- Fixed `BlobEntry` struct: `data` → `encrypted_blob`, `u64` → `i64` for sequence
- Fixed `prev_seq` type: `0u64` → `0i64` in monotonic sequence test
- Removed trailing double-semicolons

**Result:** All 10 E2E tests pass. `cargo cranky --workspace` clean.

**DevTools Screenshots taken:**
- Backup-Server settings page (empty state, add form)
- Identität settings page (identity key + export recovery phrase)

**Implemented:**
- Created this plan file
- Real ChaCha20-Poly1305 encryption in `crates/core/src/crypto/mod.rs`
- Aligned sync client protocol with backup server in `crates/core/src/sync/mod.rs`
- Added `BackupServerRecord` storage model
- Implemented full `BackupSettings` UI component with auth flow
- Implemented `IdentitySettings` UI with mnemonic export modal
- Extended setup wizard with key generation and mnemonic steps
- Created E2E protocol test in `servers/backup-server/tests/`

---

## Session Log — 2026-03-01 (Session 2)

### Fixes Applied

**1. `indexing_slicing` lint violations in E2E test file (DECISION: no `#[allow]` even in tests)**
- `servers/backup-server/tests/e2e_protocol_test.rs`: replaced all `vec[n]` and `json["key"]`
  with `.get(n).context("msg")?` and `.get("key").and_then(Value::as_str)` variants
- All 10 E2E tests continue to pass

**2. Broken dark theme — CSS asset path mismatch (DECISION D14)**
- Root cause: `asset!("assets/tailwind.css")` in `crates/core/src/ui/mod.rs` generates URL
  `dioxus://…/crates/poly-core/assets/tailwind.css` (using **package** name `poly-core`)
  but the directory on disk is `crates/core/` → 404, 0 CSS rules loaded, app appeared white
- Fix: `ln -sf /home/laragana/workspcacemsg/crates/core /home/laragana/workspcacemsg/crates/poly-core`
- Symlink committed to git; CSS now loads 105 rules, dark theme fully applied

**3. WASM build errors in `crates/core/src/storage/web.rs`**
- `unsafe impl Send for StorageInner {}` and `unsafe impl Sync for StorageInner {}` denied by
  `unsafe_code` lint. `StorageInner` is a unit struct — Rust auto-provides `Send + Sync`.
  Removed both `unsafe impl` blocks.

**4. Broken delimiter structure in `crates/core/src/ui/settings.rs`**
- Multiple `{` / `}` / `(` mismatches corrupted the Sync-Now error branch, Remove button handler,
  and Add-server `Ok` branch. All fixed by restructuring the `if`/`else` chains and removing
  stray match arms (`Err(e) => ...` without a `match` head).
- Reauth token save refactored from `drop(srv)` workaround to `.map()` to avoid borrow issue.

### Test Results

- `cargo cranky --workspace` → CLEAN ✅
- `cargo build -p poly-web --target wasm32-unknown-unknown` → `Finished` ✅
- E2E: 10/10 pass ✅
- Crypto unit tests: 6/6 pass (roundtrip, nonce uniqueness, tamper detection, identity, mnemonic, key derivation) ✅
- Sync unit tests: 5/5 pass ✅
- Desktop app: dark theme confirmed with screenshots ✅
- Web app: dark theme confirmed, setup wizard fully functional ✅
