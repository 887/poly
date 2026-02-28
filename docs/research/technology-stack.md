# Technology Stack Research

> **Compiled:** 2026-02-28  
> **Phase:** 1 (Planning & Research)

---

## 1. Dioxus 0.7.3

**Crate:** `dioxus = "0.7.3"`  
**Docs:** https://dioxuslabs.com/learn/0.7/  
**GitHub:** 34.9k stars  
**Release:** v0.7.0 on Oct 31, 2025

### Key Features for Poly

| Feature | Status | Notes |
|---|---|---|
| Subsecond hot-patching | ✅ Stable | `dx serve --hotpatch`, works on Web/Desktop/Mobile |
| TailwindCSS auto-detect | ✅ Stable | Detects `tailwind.css` at root, zero-config, v3 + v4 |
| Stores (reactive state) | ✅ Stable | `derive(Store)` on structs, reactive collections |
| Dioxus Primitives | ✅ Stable | 28 unstyled accessible components (Radix-UI equivalent) |
| WASM code splitting | ✅ Stable | Lazy loading for web target |
| Fullstack (Axum 0.8) | ✅ Stable | Server functions, WebSocket, SSE, streaming, forms |
| Desktop (Wry 52) | ✅ Stable | System webview, all 3 desktop OS |
| Blitz (WGPU native) | ⚠️ Experimental | Not all CSS supported, see blitz.is/status/css |
| Mobile (iOS/Android) | ⚠️ Less tested | Supported, iPad support added, ADB reverse proxy |
| Multi-package serve | ✅ Stable | `dx serve @client --package x @server --package y` |
| Integrated debugger | ✅ Stable | Press `d` during `dx serve` for CodeLLDB |
| Android manifest | ✅ Stable | Customizable via Dioxus.toml |
| iOS Info.plist | ✅ Stable | Customizable via Dioxus.toml |

### Hot-Reload for Library Crates

- `dx serve --hotpatch` patches function bodies at runtime
- Works on library crates in the workspace — the library's component functions are hot-patched
- `subsecond::call(fn)` for explicit hot-patch sites
- State preserved across patches (no restart)
- **MUST test this works with poly-core as workspace member in Phase 2**

### Server Functions (Dioxus 0.7.3)

New Rocket-inspired syntax:
```rust
#[post("/api/endpoint")]
async fn my_server_fn(data: Json<MyData>) -> Result<Json<Response>> {
    // runs on server
}
```

WebSocket support:
```rust
let ws = use_websocket("ws://...");
```

### Electron Wrapper (Not Native)

Dioxus does NOT natively support Electron. Our approach:
1. Build Dioxus web target → WASM + HTML/JS/CSS bundle
2. Create Electron `main.js` that loads the bundle in a `BrowserWindow`
3. Package with `electron-builder`

This adds significant complexity but user requested it. Wry desktop is strictly lighter.

---

## 2. SurrealDB 3.0

**Crate:** `surrealdb = "3.0.1"`  
**Docs:** https://surrealdb.com/3.0  
**Released:** 2026-02-24  
**License:** BSL 1.1

### Key Features

| Feature | Relevance |
|---|---|
| Multi-model (doc/graph/relational/vector) | Settings storage flexibility |
| SurrealKV embedded backend | **Our choice** — pure Rust, cross-platform |
| Remote WebSocket mode | Backup server |
| Client-side transactions | Atomic settings updates |
| Refresh tokens | Session management |
| Change feeds | Sync protocol (detect changes) |
| DEFINE API | Custom REST endpoints (backup server) |

### SurrealKV Decision (D2)

**Using SurrealKV everywhere** (not RocksDB):
- Pure Rust implementation — should compile for all targets
- No C/C++ dependencies (unlike RocksDB)
- Feature flag: `kv-surrealkv`
- **RISK**: Not tested on iOS/Android/WASM — validate in Phase 2

### Embedded Usage Pattern

```rust
use surrealdb::Surreal;
use surrealdb::engine::local::SurrealKV;

let db = Surreal::new::<SurrealKV>("path/to/data").await?;
db.use_ns("poly").use_db("settings").await?;
```

### WASM Concerns

SurrealKV likely won't work in WASM (no filesystem). Options for web target:
1. `kv-mem` (in-memory, non-persistent) + sync to backup server
2. Remote WebSocket connection to a SurrealDB server
3. IndexedDB adapter (if available)

---

## 3. Cryptography

### Ed25519 + X25519 (Session Messenger Model)

**Identity generation:**
1. `ed25519-dalek` → generate Ed25519 signing keypair
2. Derive X25519 DH keypair from Ed25519 (using curve conversion)
3. Public key (hex-encoded) = user's Account ID
4. Private key → BIP39 mnemonic = Recovery Password

**Encryption for backup:**
- Derive symmetric key from X25519 private key (or HKDF from seed)
- Encrypt with XSalsa20-Poly1305 (Session's choice) or AES-256-GCM
- Pad to fixed-size blocks to prevent size analysis

**Crates:**
- `ed25519-dalek` — Ed25519 key generation and signing
- `x25519-dalek` — X25519 ECDH
- `bip39` — mnemonic phrase encoding
- `chacha20poly1305` or `aes-gcm` — symmetric encryption
- `hkdf`, `sha2` — key derivation

### Proof-of-Work (Anubis Style)

For backup server auth rate-limiting:
1. Server generates random `challenge` bytes + `difficulty` (number of leading zero bits)
2. Client finds `nonce` such that `SHA-256(challenge || nonce)` has `difficulty` leading zero bits
3. Server verifies in O(1) — just check the hash
4. Client work is O(2^difficulty) on average
5. Difficulty 20 ≈ ~1M hashes ≈ ~0.5-2 seconds on typical hardware

---

## 4. WebRTC

**Crate:** `webrtc = "0.17.1"`  
**Downloads:** 3.45M total  
**Stars:** 5k  
**License:** MIT/Apache-2.0

### Status

- v0.17.x is the **final Tokio-coupled release** — bug fixes only
- v0.20.0+ in development — sans-I/O, runtime-agnostic (not stable yet)
- We should use v0.17.1 since we're Tokio-based

### Feature Coverage

- ✅ WebRTC peer connection
- ✅ Media streams (audio/video)
- ✅ RTP/RTCP/SRTP
- ✅ Data channels (SCTP)
- ✅ ICE (connectivity)
- ✅ DTLS (encryption)
- ✅ STUN/TURN (NAT traversal)
- ✅ SDP (session description)

### Mobile Challenges

The `webrtc` crate is pure Rust — it handles the protocol but NOT hardware access:
- **Camera**: Needs platform-specific capture (V4L2 on Linux, AVFoundation on Mac/iOS, Camera2 on Android)
- **Microphone**: Needs platform-specific audio input (ALSA/PulseAudio/CoreAudio/AAudio)
- **Speakers**: Platform-specific audio output

For Phase 3.1, we'll need:
- `cpal` crate for cross-platform audio I/O (covers desktop)
- Native Android/iOS bridges for mobile camera/mic
- Research Flutter/native packages that expose audio/video capture for Rust FFI

---

## 5. Internationalization (i18n)

### Approach: Custom Wrapper over fluent-bundle

**Why not dioxus-i18n?**
- `dioxus-i18n = "0.5.1"` targets Dioxus 0.6 — not updated for 0.7
- Rather than fork/maintain someone else's crate, build a minimal wrapper

**fluent-bundle crate:**
- Core implementation of Project Fluent
- Loads `.ftl` files
- Message formatting with parameters
- Pluralization, gender, number formatting
- Stable, well-maintained

**Our wrapper will provide:**
1. `t!("key")` / `t!("key", arg: "value")` macro for easy translations
2. `use_i18n()` Dioxus hook for reactive locale switching
3. `.ftl` file loading (embedded at compile-time or runtime-loaded)
4. Fallback chain: user locale → English

### Fluent (.ftl) File Format

```ftl
# locales/en/main.ftl
app-name = Poly
welcome-message = Welcome to { app-name }!
server-count = { $count ->
    [one] {$count} server
   *[other] {$count} servers
}
```

---

## 6. TailwindCSS

### Integration with Dioxus 0.7.3

- Dioxus auto-detects `tailwind.css` at project/workspace root
- Zero configuration — just have the file present
- Supports both Tailwind v3 and v4
- Auto-starts the Tailwind watcher during `dx serve`

### Theme CSS Variables

We'll use CSS custom properties (`--var-name`) for theming:
```css
:root {
  --bg-primary: #1a1a2e;
  --bg-secondary: #16213e;
  --text-primary: #e0e0e0;
  --accent: #7c83db;
  /* ... many more */
}

/* Theme presets override these variables */
.theme-purple {
  --accent: #5865F2;  /* Discord blurple */
}
```

This allows:
- Theme switching by swapping a CSS class
- Per-color customization by overriding individual variables
- Custom CSS editor can inject arbitrary styles
- Import/export as complete CSS files
