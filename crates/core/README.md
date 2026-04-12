# poly-core

The shared library crate for **Poly** (PolyGlot Messenger). Contains all UI components, state management, database abstraction, i18n, theming, crypto, and backup sync logic.

## Purpose

Every Poly app entry point (`apps/desktop`, `apps/web`, `apps/android`, etc.) depends on this crate. It provides:

- **UI**: All Dioxus components (server sidebar, channel list, chat view, settings, etc.)
- **State**: Global app state using Dioxus Stores
- **Database**: SQLite storage via host-bridge (`/host/kv/*`) for local settings and account storage
- **i18n**: Internationalization wrapper over Project Fluent
- **Themes**: CSS variable-based theme engine with presets and custom CSS editor
- **Crypto**: Ed25519/X25519 key generation, BIP39 mnemonics, encryption helpers
- **Sync**: Backup server client for encrypted settings synchronization

## Feature Flags

| Feature | Description |
|---|---|
| `demo` (default) | Demo/mock client for testing |
| `stoat` | Stoat (Revolt) messenger backend |
| `matrix` | Matrix messenger backend |
| `discord` | Discord messenger backend (dev-only) |
| `teams` | Microsoft Teams messenger backend (dev-only) |
| `server` | Poly server protocol client |
| `hackernews` | Hacker News forum backend |
| `lemmy` | Lemmy federated forum backend |
| `github` | GitHub Issues/PRs/notifications |
| `storage-host-bridge` | SQLite storage via `/host/*` routes |
| `native-nav` | Native navigation (desktop) |
| `all-backends` | Enable all backends |

## License

MIT / Apache-2.0
