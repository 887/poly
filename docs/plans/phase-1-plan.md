# Phase 1 Plan — Planning & Research

> **Status:** ✅ Complete  
> **Started:** 2026-02-28  
> **Completed:** 2026-02-28  
> **Parent:** [Overall Plan](overall-plan.md)

---

## 1.1 Project Documentation Setup

- [x] **1.1.1** Create `docs/overall-plan.md` — comprehensive reorganized plan
- [x] **1.1.2** Create `docs/phase-1-plan.md` — this document
- [x] **1.1.3** Create `docs/phase-2-plan.md` — structure + UI + backup infra checklist
- [x] **1.1.4** Create `docs/phase-3-plan.md` — client implementation checklist
- [x] **1.1.5** Create root `agents.md` — global project rules and memory
- [x] **1.1.6** Create `last-crate-update-date` file

## 1.2 Technology Research & Documentation

- [x] **1.2.1** Research Dioxus 0.7.3 subsecond hot-reload for library crates
  - Confirmed: `subsecond::call()` pattern, `dx serve --hotpatch`
  - Works with workspace member crates when properly configured
  - Documented in `crates/core/agents.md`
- [x] **1.2.2** Research Dioxus 0.7.3 multi-platform Dioxus.toml configuration
  - Each app target gets its own `Dioxus.toml`
  - Platform-specific settings: AndroidManifest.xml, Info.plist customization
- [x] **1.2.3** Research SurrealDB 3.0 SurrealKV embedded mode
  - `surrealdb = "3.0.1"` with `kv-surrealkv` feature
  - Pure Rust, should compile for all targets
  - iOS/Android compilation needs early validation in Phase 2
- [x] **1.2.4** Research `fluent-bundle` for custom i18n wrapper
  - `fluent-bundle` crate provides core Fluent runtime
  - `.ftl` file format, message references, parameterized messages
  - Will build thin `t!()` macro wrapper with locale switching
- [x] **1.2.5** Research TailwindCSS integration with Dioxus monorepo
  - Dioxus auto-detects `tailwind.css` at workspace root
  - Zero-config, supports Tailwind v3 and v4
- [x] **1.2.6** Research Electron + WASM integration pattern
  - Build Dioxus web target (WASM), serve from Electron main process
  - Electron `main.js` loads `index.html` pointing to WASM bundle
  - Requires Node.js + npm toolchain for Electron build
- [x] **1.2.7** Research Dioxus 0.7.3 mobile targets
  - Android: `dx serve --platform android`, customizable AndroidManifest.xml
  - iOS: `dx serve --platform ios`, customizable Info.plist
  - ADB reverse proxy for real-device hot-reload
- [x] **1.2.8** Research Ed25519 + X25519 + BIP39 for Session-style identity
  - `ed25519-dalek` for Ed25519 key generation
  - `x25519-dalek` for key exchange derivation
  - `bip39` crate for mnemonic phrase encoding
  - X25519 derived from Ed25519 for encryption operations
- [x] **1.2.9** Research Anubis PoW challenge pattern for backup server
  - Client receives challenge with difficulty target
  - Client computes SHA-256 hash with nonce until hash meets difficulty
  - Server verifies solution in O(1), client proves work in O(2^difficulty)
  - Prevents brute-force passphrase attempts

## 1.3 Messenger Backend Research

- [x] **1.3.1** Stoat (Revolt) API deep-dive
  - REST API + WebSocket for real-time events
  - Developer docs at `developers.stoat.chat`
  - Auth: email/password or token-based
  - No mature Rust SDK — must build `poly-stoat` from scratch
  - Supports self-hosted instances (different base URL)
  - Voice/video: WebRTC-based
  - Documented in `clients/stoat/agents.md`
- [x] **1.3.2** Matrix SDK study
  - `matrix-sdk = "0.16.0"` — production-grade, powers Element X
  - Spaces = server-like groupings (categories of rooms)
  - Rooms = channels, DMs = 2-person rooms
  - SSO login flow supported
  - E2EE via Olm/Megolm (matrix-sdk-crypto)
  - VoIP for voice/video (WebRTC-based)
  - Federation: any homeserver, matrix.org default
  - Documented in `clients/matrix/agents.md`
- [x] **1.3.3** Microsoft Teams / Graph API study
  - Reference: `ttyms` crate — terminal Teams client using Microsoft Graph API
  - OAuth2 Device Code Flow or PKCE browser flow
  - Default Azure AD client ID available
  - Teams-Teams → Poly servers, channels → channels
  - Group chats → DMs with multi-user
  - Documented in `clients/teams/agents.md`
- [x] **1.3.4** Discord client landscape survey
  - `discord_client_gateway` / `discord_client_rest` — pre-alpha Rust crates
  - TOS explicitly prohibits unofficial clients / self-botting
  - Approach decision deferred to Phase 3.3
  - Options: direct API, bridge, hidden webview, Matrix bridge
  - Documented in `clients/discord/agents.md`

## 1.4 Agent & Memory File Creation

- [x] **1.4.1** Root `agents.md` — global project rules
- [x] **1.4.2** `crates/core/agents.md` + `README.md`
- [x] **1.4.3** `clients/client/agents.md` + `README.md`
- [x] **1.4.4** `clients/demo/agents.md` + `README.md`
- [x] **1.4.5** `clients/stoat/agents.md` + `README.md`
- [x] **1.4.6** `clients/matrix/agents.md` + `README.md`
- [x] **1.4.7** `clients/discord/agents.md` + `README.md`
- [x] **1.4.8** `clients/teams/agents.md` + `README.md`
- [x] **1.4.9** `servers/backup-server/agents.md` + `README.md`
- [x] **1.4.10** App entry point agents: `apps/desktop/agents.md`, `apps/desktop-blitz/agents.md`, `apps/desktop-electron/agents.md`, `apps/android/agents.md`, `apps/ios/agents.md`, `apps/web/agents.md`

## 1.5 Research Documentation

- [x] **1.5.1** Create `docs/research/technology-stack.md` — consolidated research findings
- [x] **1.5.2** Create `docs/research/client-backends.md` — per-backend research notes

---

## Phase 1 Completion Criteria

- [x] All plan documents exist with numbered items and checkboxes
- [x] All agent.md files written for every crate and app
- [x] All README.md files written for every crate
- [x] Research documented in `docs/research/`
- [x] `last-crate-update-date` file created
- [x] Root `agents.md` with all global rules
- [x] Directory structure in place for all crates/apps
