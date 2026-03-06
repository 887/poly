# poly-discord

Discord client for **Poly** (PolyGlot Messenger).

## Purpose

Implements the `ClientBackend` trait for Discord. Allows viewing and interacting with Discord servers, channels, DMs, and group chats from within Poly.

## WASM Plugin Support (2026-03-06)

Builds as **both** native and WASM Component Model plugin:

```sh
# Native (workspace default):
cargo build -p poly-discord

# WASM plugin:
cargo component build -p poly-discord --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_discord.wasm (4.3MB debug)
```

Feature-gated (`native` feature default). Currently a **stub** — WIT guest implementation in `src/guest.rs` returns errors for all operations. Full implementation coming in Phase 3.3.

## ⚠️ Important Notice

Discord's Terms of Service prohibit unofficial clients and self-botting. Using this module may put your Discord account at risk. Users are clearly warned before adding a Discord account.

## Features (planned)

- View Discord servers (guilds) with categories and channels
- Send and receive text messages
- DMs and group DMs (up to ~10 users)
- Friend list and friend requests
- Voice and video channels (stretch goal)
- Self-hosted Discord-compatible API support (custom base URL)

## Implementation

Approach to be determined in Phase 3.3. See the agents.md for research notes on possible approaches (direct API, webview bridge, Matrix bridge, etc.).

## Testing

**10 E2E tests** verify stub behavior through the WASM plugin host:

```sh
cargo test -p poly-plugin-loader-tests --features test-discord --test client_e2e -- --nocapture
```

## License

MIT / Apache-2.0
