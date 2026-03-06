# poly-teams

Microsoft Teams client for **Poly** (PolyGlot Messenger).

## Purpose

Implements the `ClientBackend` trait for Microsoft Teams using the Microsoft Graph REST API.

## WASM Plugin Support (2026-03-06)

Builds as **both** native and WASM Component Model plugin:

```sh
# Native (workspace default):
cargo build -p poly-teams

# WASM plugin:
cargo component build -p poly-teams --target wasm32-wasip2
# Output: target/wasm32-wasip1/debug/poly_teams.wasm (4.3MB debug)
```

Feature-gated (`native` feature default). Currently a **stub** — WIT guest implementation in `src/guest.rs` returns errors for all operations. Full implementation coming in Phase 3.4.

## Features

- OAuth2 authentication (Device Code Flow + PKCE browser flow)
- Teams displayed as servers with channels
- 1:1 chats as DMs
- Group chats as multi-user groups (displayed under DMs with Teams icon)
- Send, receive, edit, delete messages with reactions
- User presence and status
- Contact/people discovery

## Implementation

Built on the Microsoft Graph API (`graph.microsoft.com/v1.0/`). References the `ttyms` crate for auth flow and API patterns.

Ships with a default Azure AD client ID for out-of-the-box usage.

## Testing

**10 E2E tests** verify stub behavior through the WASM plugin host:

```sh
cargo test -p poly-plugin-loader-tests --features test-teams --test client_e2e -- --nocapture
```

## License

MIT / Apache-2.0
