# poly-core — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-core` is **THE** shared library crate. It contains ALL shared UI components, state management, database logic, i18n, theming, crypto, and backup sync. Every app entry point (`apps/*`) depends on this crate.

**This is where you spend 90%+ of your development time.**

---

## CRITICAL: Hot Reload

This crate **MUST** support Dioxus subsecond hot-reload:
- Test with: `dx serve --hotpatch` from any app entry point
- If hot-reload breaks, STOP all other work and fix it
- Use `subsecond::call()` for function-level hot-patching where needed
- All Dioxus components in `src/ui/` are automatically hot-patched
- Ensure the crate is a proper `lib` crate (not bin) — hot-reload only works on library code

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
