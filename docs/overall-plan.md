# Poly — PolyGlot Messenger: Overall Plan

> **Last Updated:** 2026-02-28  
> **Status:** Phase 1 — Planning & Research  
> **License:** MIT / Apache-2.0 dual license  
> **Repository:** `poly`

---

## 1. Vision & Goals

**Poly** is a cross-platform, multi-backend messenger client built with **Rust + Dioxus 0.7.3 + SurrealDB 3.0**. It aggregates multiple messaging platforms into a single, unified, Discord-like UI.

### 1.1 Core Value Proposition
- **One client, many networks**: Chat on Stoat (Revolt), Matrix, Discord, and Microsoft Teams simultaneously without switching apps
- **Multi-account**: Multiple accounts per backend (e.g., 3 Discord accounts, 2 Matrix accounts)
- **Favorites-driven UX**: User curates their own server sidebar from across all backends — not every joined server, just the ones they care about
- **Cross-platform**: Desktop (Windows/Linux/Mac), mobile (Android/iOS), web — single codebase
- **Encrypted cloud backup**: Settings synced to user-controlled backup servers with end-to-end encryption
- **Privacy-first identity**: Session Messenger-style Ed25519 keypair identity — no email, no phone number

### 1.2 Design Philosophy
- Look and feel like Discord/Stoat — familiar enough to pull Discord users into a multi-protocol world
- Act like Thunderbird — aggregating backends the way Thunderbird aggregates email providers
- Be a polyglot client, not a feature-complete replacement for any single platform's official client
- Feature-flag each backend so users can build with only the platforms they need

---

## 2. Technology Stack

### 2.1 Core Technologies

| Technology | Version | Purpose |
|---|---|---|
| Rust | Latest stable (rustup) | Language |
| Dioxus | 0.7.3 | Cross-platform UI framework |
| SurrealDB | 3.0.1 | Embedded database (SurrealKV backend everywhere) |
| Tokio | Multi-threaded runtime | Async runtime (implied by Dioxus) |
| TailwindCSS | v4 (auto-detected by Dioxus) | Styling |
| Axum | 0.8 (via Dioxus fullstack) | Web server / backup server |

### 2.2 Messenger Backend Libraries

| Backend | Approach | Crate/Library |
|---|---|---|
| Matrix | Mature Rust SDK | `matrix-sdk = "0.16.0"` (powers Element X) |
| Stoat (Revolt) | Custom REST/WS client | Build from `developers.stoat.chat` API docs |
| Discord | TBD in Phase 3.3 | Options: direct API, bridge, webview — decision deferred |
| Teams | Microsoft Graph API | Reference: `ttyms` crate for auth/message patterns |

### 2.3 Supporting Crates

| Crate | Version | Purpose |
|---|---|---|
| `ed25519-dalek` | latest | Identity key generation (Session-style) |
| `x25519-dalek` | latest | Key exchange, encryption derivation |
| `bip39` | latest | Mnemonic seed phrase for key recovery |
| `webrtc` | 0.17.1 | Voice/video calling (pure Rust WebRTC) |
| `fluent-bundle` | latest | i18n (Project Fluent .ftl files) |

### 2.4 Desktop Renderers (3 builds per OS)

1. **Wry (webview)** — Primary, stable, uses system webview. Default desktop build.
2. **Blitz (WGPU native)** — Experimental Dioxus native renderer. GPU-accelerated, no webview dependency.
3. **Electron** — WASM web build wrapped in Electron shell. For users who prefer Electron ecosystem.

---

## 3. Monorepo Crate Architecture

```
poly/
├── Cargo.toml                      # Workspace root — shared dependency versions
├── agents.md                       # Root agent rules (ALWAYS READ FIRST)
├── last-crate-update-date          # Track when crates were last updated
├── .github/workflows/              # CI/CD GitHub Actions
├── .vscode/                        # Launch profiles, tasks, settings
│
├── crates/
│   ├── poly-core/                  # ★ SHARED LIBRARY — main development target
│   │   ├── src/
│   │   │   ├── lib.rs              # Library entry point
│   │   │   ├── ui/                 # All Dioxus UI components
│   │   │   ├── state/              # App state management (Dioxus Stores)
│   │   │   ├── db/                 # SurrealDB abstraction layer
│   │   │   ├── i18n/               # Custom i18n wrapper over fluent-bundle
│   │   │   ├── theme/              # Theme engine — presets + custom CSS
│   │   │   ├── crypto/             # Key generation, encrypt/decrypt
│   │   │   └── sync/               # Backup server sync client
│   │   ├── Cargo.toml
│   │   ├── agents.md
│   │   └── README.md
│   │
│   ├── poly-client/                # Shared messenger client trait/protocol
│   │   ├── src/lib.rs              # ClientBackend trait + shared types
│   │   ├── Cargo.toml
│   │   ├── agents.md
│   │   └── README.md
│   │
│   ├── poly-demo/                  # Demo/mock client for UI testing
│   ├── poly-stoat/                 # Stoat (Revolt) client implementation
│   ├── poly-matrix/                # Matrix client (wraps matrix-sdk)
│   ├── poly-discord/               # Discord client
│   ├── poly-teams/                 # Microsoft Teams client (Graph API)
│   │
│   └── poly-backup-server/         # Encrypted backup sync server
│       ├── src/
│       │   ├── main.rs             # Axum + Dioxus fullstack entry
│       │   ├── auth/               # PoW challenge + passphrase + tokens
│       │   ├── sync/               # Encrypted blob storage/retrieval
│       │   └── web/                # Admin web UI
│       ├── Cargo.toml
│       ├── agents.md
│       └── README.md
│
├── apps/                           # Platform entry points (thin wrappers)
│   ├── desktop/                    # Wry (webview) desktop
│   ├── desktop-blitz/              # Blitz (WGPU native) desktop
│   ├── desktop-electron/           # Electron wrapper
│   ├── android/                    # Android
│   ├── ios/                        # iOS
│   └── web/                        # Dioxus fullstack web (Axum)
│
├── locales/                        # Fluent .ftl translation files
│   ├── en/                         # English (default)
│   ├── de/                         # German
│   ├── fr/                         # French
│   └── es/                         # Spanish
│
├── assets/                         # Shared static assets
│   ├── tailwind.css                # Tailwind entry (auto-detected by Dioxus)
│   ├── styling/themes/             # Theme CSS presets
│   │   ├── neutral-dark.css        # Default dark theme
│   │   ├── purple.css              # Discord-inspired purple theme
│   │   └── red.css                 # Stoat-inspired red/coral theme
│   └── icons/                      # App icons, backend logos
│
└── docs/                           # Project documentation
    ├── overall-plan.md             # This file
    ├── phase-1-plan.md             # Planning & Research
    ├── phase-2-plan.md             # Structure + UI + Backup Infra
    ├── phase-3-plan.md             # Client Implementations
    └── research/                   # Technology research notes
```

### 3.1 Key Architecture Decisions

- **`poly-core` is THE library crate**. All shared UI, state, DB, crypto, i18n, theme logic lives here. This crate MUST support Dioxus subsecond hot-reload. All apps import it.
- **`poly-client` defines the protocol**. The `ClientBackend` trait abstracts all messenger operations. Each backend crate (poly-stoat, poly-matrix, etc.) implements this trait.
- **Feature flags** control which backends are compiled: `stoat`, `matrix`, `discord`, `teams`, `demo`.
- **SurrealKV everywhere**. No RocksDB/SQLite divergence between platforms.
- **Apps are thin wrappers**. Each app in `apps/` is just a `main.rs` that initializes the platform-specific Dioxus renderer and pulls in `poly-core`.

---

## 4. UI Architecture

### 4.1 Layout (Desktop)

```
┌──────────────────────────────────────────────────────────────────┐
│ ┌───┐ ┌────────────┐ ┌──────────────────────────────┐ ┌───────┐│
│ │   │ │            │ │ #channel-name  [⚙] [🔍]      │ │ Users ││
│ │ F │ │ Categories │ │                               │ │       ││
│ │ a │ │ ├─#general │ │  Messages...                  │ │ @user1││
│ │ v │ │ ├─#random  │ │  [Avatar] Username  12:34 PM  │ │ @user2││
│ │   │ │ ├─🔊voice  │ │   Message content with       │ │ @user3││
│ │ S │ │            │ │   images, reactions...        │ │  ...  ││
│ │ e │ │ Source:    │ │                               │ │       ││
│ │ r │ │ 🟣Discord  │ │                               │ │       ││
│ │ v │ │ @account1  │ │                               │ │       ││
│ │ e │ │            │ │ ┌───────────────────────────┐ │ │       ││
│ │ r │ │            │ │ │ Type a message... [Send]  │ │ │       ││
│ │ s │ │            │ │ └───────────────────────────┘ │ │       ││
│ └───┘ └────────────┘ └──────────────────────────────┘ └───────┘│
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ [Avatar] Username#1234  ⚙ Settings                        │ │
│ └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

### 4.2 Server Sidebar Details
- **Top**: DMs/Friends icon (aggregated across all accounts)
- **Below**: Notifications icon (aggregated feed)
- **Below**: Favorited server icons, each showing:
  - Large: Server icon
  - Small overlay (top-left): Source network badge (Stoat/Matrix/Discord/Teams icon)
  - Small overlay (bottom-right): Account avatar for the account it's from
  - Notification badge (unread count)
- Source network + account shown in channel list banner when server is selected

### 4.3 Layout (Mobile — 3 swipeable panels)
- **Left swipe**: Server list + Channel list for current server
- **Center**: Chat view (messages + input)
- **Right swipe**: User list for current channel/call

### 4.4 Settings Architecture
Located in top-right bar area of chat view:
- **Accounts**: Per-backend account list → add/remove, view server browser, favorite servers, friend list (with icons, searchable)
- **Backup servers**: Add/remove/configure multiple backup sync servers
- **Identity**: View/export mnemonic recovery phrase, public key (user ID)
- **Theme**: Preset selector (Neutral Dark / Purple / Red), per-color customizer, custom CSS editor with full import/export
- **Language**: Locale selector
- **Appearance**: Dark/Light mode toggle (follows device by default, overridable)

### 4.5 First-Launch Setup Wizard
Session Messenger-style:
1. Generate Ed25519 keypair on-device
2. Derive X25519 key from Ed25519
3. Display public key as Account ID / username
4. Display mnemonic recovery phrase (BIP39)
5. Prompt user to save/export recovery phrase
6. Initialize local SurrealKV database
7. Redirect to main app (empty — add accounts through settings)

---

## 5. Backup Server Architecture

### 5.1 Purpose
A minimal encrypted settings synchronization server. Knows NOTHING about user data — stores only encrypted blobs identified by Ed25519 public key. The server cannot read any stored content.

### 5.2 Auth Flow
```
Client                                    Server
  │                                          │
  │─── 1. POST /api/challenge ─────────────►│
  │    { public_key: "<hex ed25519 pubkey>" }│
  │                                          │ Generate random nonce + target prefix
  │                                          │ Store pending challenge (TTL: 60s)
  │◄── 2. { nonce, difficulty, expires_at } ─│
  │                                          │
  │    (Client mines: SHA-256(nonce+counter) │
  │     must start with N zero bits)         │
  │                                          │
  │─── 3. POST /api/auth ──────────────────►│
  │    { public_key, nonce, counter,         │
  │      passphrase }                        │
  │    (passphrase = server-wide secret      │
  │     shared out-of-band by server admin)  │
  │                                          │ Verify PoW solution
  │                                          │ Verify passphrase (constant-time compare)
  │                                          │ Check: max_accounts not exceeded
  │                                          │ Upsert account record for public_key
  │                                          │ Generate token: 128-char random Base62
  │                                          │ Store token with device metadata
  │◄── 4. { token, expires_at } ───────────│
  │                                          │
  │    Client stores token in SurrealKV:     │
  │    key = "backup_token:{server_url}"     │
  │    value = { token, expires_at,          │
  │              server_url, acquired_at }   │
  │                                          │
  │─── 5. Sync operations ─────────────────►│
  │    Authorization: Bearer <token>         │
  │    Push/pull encrypted settings blobs    │
  └──────────────────────────────────────────┘
```

### 5.3 Passphrase Authentication
- The server is configured with a **server-wide passphrase** (`POLY_PASSPHRASE` env var)
- This passphrase is shared out-of-band by the server administrator (e.g., published on a private page, shared via Signal, etc.)
- The passphrase is sent alongside the PoW solution — both must be correct in a single request
- The passphrase is compared using constant-time equality to prevent timing attacks
- Failed attempts (wrong passphrase OR invalid PoW) increment a per-IP rate-limit counter
- After N failures: exponential backoff enforced (429 Too Many Requests + `Retry-After` header)
- The passphrase is **never stored** beyond comparison — no hash, no log
- Goal: prevent open registration while keeping the protocol simple (no invite codes, no email)

### 5.4 Token System

**Token format**: 128-character random Base62 string (`[a-zA-Z0-9]`)
- Generated server-side with `rand::thread_rng().sample_iter(Alphanumeric)`
- 128 chars × log₂(62) ≈ 760 bits of entropy — brute force is computationally infeasible

**Server-side token record** (stored in SurrealDB):
```
{
  token_hash: SHA-256(token),   // Never store raw token
  public_key: "<hex>",          // Which account this belongs to
  device_name: "<user-agent>",  // Client-provided label (e.g. "Linux Desktop")
  created_at: timestamp,
  last_seen_at: timestamp,       // Updated on every successful API call
  expires_at: timestamp          // = created_at + POLY_TOKEN_EXPIRY_DAYS
}
```

**Client-side token storage** (in SurrealKV, `poly-core/src/sync/`):
```
{
  server_url: "https://backup.example.com",
  token: "<raw 128-char token>",
  expires_at: timestamp,
  acquired_at: timestamp
}
```
Key: `backup_token:{server_url}` — one token per configured backup server.

**Token lifecycle**:
1. **Acquisition**: After successful PoW + passphrase auth — token stored locally
2. **Use**: Sent as `Authorization: Bearer <token>` on every sync request
3. **Refresh**: Tokens are long-lived (default 1 year inactivity window). No periodic refresh needed. Re-auth only happens when:
   - Server returns `401 Unauthorized` (token expired or revoked)
   - `expires_at` is within 30 days (proactive re-auth)
   - User manually triggers re-auth from backup server settings
4. **Revocation**: Server admin can revoke tokens via admin UI. Client detects 401, prompts re-auth.
5. **Expiry**: Tokens expire after `POLY_TOKEN_EXPIRY_DAYS` of inactivity (rolling — reset on each use). A token used today expires N days from today, not from creation.

**Token validation on server** (every API call):
1. Hash incoming token with SHA-256
2. Look up hash in DB — 404-equivalent if not found
3. Check `expires_at` — 401 if expired
4. Update `last_seen_at` + roll `expires_at` forward
5. Proceed with request

- Tokens can be revoked (remote logout from admin UI)
- Rate limiting + exponential backoff on failed passphrase/PoW attempts

### 5.5 Storage Model
- **Per-user record**: identified by Ed25519 public key
- **Encrypted blob storage**: app settings encrypted client-side, stored as opaque blobs
- **Sync protocol**: each setting change gets a monotonic sequence number; client pulls changes since last-seen sequence
- **Multi-server**: App supports adding multiple backup servers for redundancy. Each server is independently enabled/disabled.

### 5.6 Per-Server Status (Client UI)
The backup servers settings page shows, per configured server:
- **URL** + admin-provided label
- **Enabled/disabled toggle** (on/off slider) — disabled servers are skipped during sync
- **Status indicator**: Connected ✓ / Auth required / Unreachable / Syncing…
- **Last synced** timestamp
- **Sequence number** (last pulled seq)
- **Token info**: acquired date, days until expiry
- **Actions**: Sync now, Re-authenticate, Remove server

### 5.7 Server Configuration
- `POLY_PASSPHRASE` — server-wide access passphrase
- `POLY_MAX_ACCOUNTS` — maximum user accounts (0 = unlimited)
- `POLY_TOKEN_EXPIRY_DAYS` — days of inactivity before token expires (default: 365)
- `POLY_POW_DIFFICULTY` — proof-of-work difficulty in leading zero bits (default: 20)

### 5.4 Storage Model
- **Per-user record**: identified by Ed25519 public key
- **Encrypted blob storage**: app settings encrypted client-side, stored as opaque blobs
- **Sync protocol**: each setting change gets a monotonic sequence number; client pulls changes since last-seen sequence
- **Multi-server**: App supports adding multiple backup servers for redundancy

### 5.5 Server Configuration
- `POLY_PASSPHRASE` — server-wide access passphrase
- `POLY_MAX_ACCOUNTS` — maximum user accounts (0 = unlimited)
- `POLY_TOKEN_EXPIRY_DAYS` — days of inactivity before token expires
- `POLY_POW_DIFFICULTY` — proof-of-work challenge difficulty

---

## 6. Encryption & Identity

### 6.1 Key Generation (Session Messenger Model)
1. Generate Ed25519 signing keypair (private + public)
2. Derive X25519 Diffie-Hellman keypair from Ed25519 keys
3. Public key = User ID / "username" (hex-encoded, like Session's Account ID)
4. Private key → BIP39 mnemonic phrase (Recovery Password)

### 6.2 What Gets Encrypted
- **Local SurrealKV**: Account tokens stored UNENCRYPTED (acceptable — local device)
- **Backup server**: ALL data encrypted with user's key BEFORE leaving the device
  - Account tokens, OAuth cookies, server favorites, friend lists, theme settings
  - Never send plaintext to the backup server
- **Encryption algorithm**: XSalsa20-Poly1305 (same as Session) or AES-256-GCM

### 6.3 Sync Encryption Flow
```
Local Settings → Serialize → Encrypt with derived symmetric key → Upload encrypted blob
                                                                         │
Download encrypted blob → Decrypt with derived symmetric key → Deserialize → Merge
```

---

## 7. Internationalization (i18n)

### 7.1 Approach
Custom thin wrapper over `fluent-bundle` (Project Fluent):
- `.ftl` files in `locales/{lang}/` directory
- Runtime locale switching
- `t!("key", arg: "value")` macro for translations
- Fallback chain: user locale → English
- All UI strings MUST go through i18n from day one

### 7.2 Initial Languages
- English (en) — default
- German (de)
- French (fr)
- Spanish (es)

### 7.3 File Structure
```
locales/
├── en/
│   ├── main.ftl          # General UI strings
│   ├── settings.ftl      # Settings page
│   ├── chat.ftl          # Chat-related strings
│   └── setup.ftl         # First-launch wizard
├── de/
│   ├── main.ftl
│   └── ...
└── ...
```

---

## 8. Theme System

### 8.1 Built-in Presets
- **Neutral Dark** (default): Dark slate/charcoal, modern neutral tones
- **Purple** (Discord-inspired): Blurple accents (#5865F2), dark background
- **Red** (Stoat-inspired): Red/coral accents, dark background

### 8.2 Customization Levels
1. **Preset selector**: Quick switch between built-in themes
2. **Per-color editor**: Modify individual CSS variables (background, text, accent, etc.)
3. **Custom CSS editor**: Full CSS editing with live preview
4. **Theme import/export**: Share complete themes as CSS files with friends
5. **Device-follows mode**: Dark/light follows OS preference (overridable)

### 8.3 Implementation
- CSS custom properties (variables) for all themeable colors
- Theme stored in SurrealKV as user preference
- Custom CSS applied as `<style>` override
- Theme settings page with color pickers + CSS editor

---

## 9. Phase Overview

| Phase | Description | Key Deliverables |
|---|---|---|
| **Phase 1** | Planning & Research | All plan docs, agent.md files, technology research, architecture decisions |
| **Phase 2** | Project Structure + UI | Working monorepo, 90% UI, backup server, demo client, i18n, themes, CI/CD |
| **Phase 3.1** | Stoat Client + Voice/Video | Chat, voice, video with Stoat servers, WebRTC infrastructure |
| **Phase 3.2** | Matrix Client | matrix-sdk integration, Spaces-as-servers, E2EE, federation |
| **Phase 3.3** | Discord Client | TBD approach, server/channel/DM support |
| **Phase 3.4** | Teams Client | Microsoft Graph API, Teams-as-servers, group chats |

See individual phase plan documents for detailed checklists:
- [Phase 1 Plan](phase-1-plan.md)
- [Phase 2 Plan](phase-2-plan.md)
- [Phase 3 Plan](phase-3-plan.md)

---

## 10. Key Decisions Registry

| # | Decision | Chosen Option | Rationale | Date |
|---|---|---|---|---|
| D1 | Desktop renderers | Wry + Blitz + Electron (3 builds) | User wants all three options per OS | 2026-02-28 |
| D2 | Database engine | SurrealKV everywhere | No platform divergence, simpler codebase | 2026-02-28 |
| D3 | Discord approach | Deferred to Phase 3.3 | TOS risk, landscape may change | 2026-02-28 |
| D4 | Voice/Video timing | Phase 3.1 with Stoat | Build WebRTC infra with first real client | 2026-02-28 |
| D5 | i18n approach | Custom wrapper over fluent-bundle | dioxus-i18n outdated for 0.7, minimal wrapper is cleaner | 2026-02-28 |
| D6 | Stoat naming | "Stoat" + "(formerly Revolt)" note | Revolt rebranded to Stoat | 2026-02-28 |
| D7 | License | MIT / Apache-2.0 dual | Permissive, standard Rust ecosystem choice | 2026-02-28 |
| D8 | Repo name | `poly` | Short, matches app name | 2026-02-28 |
| D9 | Theme system | Neutral default + presets + full CSS customization | Every color configurable, import/export themes | 2026-02-28 |
| D10 | Backup auth | PoW challenge + server passphrase + long tokens + device tracking | Anti-brute-force, session management | 2026-02-28 |
| D11 | Initial languages | EN + DE + FR + ES | English default, German + 2 more for baseline | 2026-02-28 |
| D12 | Demo client | Phase 2 alongside UI | Enables full UI testing without real backends | 2026-02-28 |

---

## 11. Risk Register

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| R1 | Discord TOS bans unofficial clients | High — could lose user accounts | Defer approach decision; consider bridge/webview; warn users | 
| R2 | Blitz renderer too immature | Medium — experimental features may break | Wry is primary; Blitz is optional extra |
| R3 | dioxus-i18n not ready for 0.7 | Low — we're building our own wrapper | Using fluent-bundle directly |
| R4 | SurrealKV doesn't compile on iOS/Android | High — breaks mobile | Test early in Phase 2; fallback to kv-mem or remote |
| R5 | WebRTC mobile needs native bridges | Medium — camera/mic platform-specific | Research native bindings, platform-specific code in Phase 3.1 |
| R6 | Subsecond hot-reload fails on library crate | **Critical** — stated failure condition | Test immediately in Phase 2 setup; adjust crate boundaries if needed |
| R7 | Electron wrapper adds significant complexity | Low-Medium — extra build target | Can deprioritize if becomes too costly |

---

## 12. Glossary

| Term | Definition |
|---|---|
| **Poly** | This app — PolyGlot Messenger |
| **Backend** | A messaging platform (Stoat, Matrix, Discord, Teams) |
| **Client** | Our implementation that speaks a backend's protocol |
| **Server** (UI) | A community/workspace in the favorites sidebar (e.g., a Discord guild, Stoat server, Matrix Space) |
| **Backup Server** | Our encrypted settings sync server (poly-backup-server) |
| **Recovery Phrase** | BIP39 mnemonic encoding of the user's Ed25519 private key |
| **Account ID** | User's Ed25519 public key, used as identifier |
| **Stoat** | Revolt messenger's new name (formerly Revolt) |
| **SurrealKV** | SurrealDB's embedded key-value storage engine |
| **Wry** | System webview wrapper used by Dioxus desktop |
| **Blitz** | Dioxus's experimental WGPU native HTML/CSS renderer |
