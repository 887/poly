# Memory: Architecture findings and plan

*Stored: 2026-03-10T01:15:29.951590836+00:00*

---

# Architecture Findings for Poly Server E2E

## Current State
- `poly-server-client` works natively (17 integration tests pass)
- Web app (`apps/web/`) uses Dioxus `web` feature = pure client-side WASM
- `add_poly_server()` in client_manager is behind `#[cfg(not(target_arch = "wasm32"))]` 
- Settings → Accounts has a stub "Add Account" button (not wired)
- No account wizard UI exists yet
- `poly-server-client` has `native` feature gating tokio-tungstenite, reqwest, ed25519-dalek, hex
- For WASM: reqwest supports wasm32-unknown-unknown via web-sys fetch
- For WASM: ed25519-dalek is pure Rust, works in WASM
- Blocker: tokio-tungstenite pulls native-tls → can't WASM. Must gate behind native feature.
- WS module is already gated behind `#[cfg(feature = "native")]` in lib.rs

## Plan
1. Create a `wasm` feature in poly-server-client that enables reqwest + ed25519 for WASM
2. Configure reqwest to compile for wasm32 (default-features=false, +json)
3. Make backend.rs work on WASM (skip WS, HTTP-only)
4. Add poly-server-client dep to poly-core with proper feature gating
5. Update client_manager for WASM support
6. Build the Add Account wizard UI
7. Visual E2E test via poly-web MCP

## Key Files
- `clients/server-client/Cargo.toml` - needs wasm feature
- `clients/server-client/src/lib.rs` - module gating
- `clients/server-client/src/backend.rs` - needs WASM path
- `crates/core/Cargo.toml` - add server-client dep
- `crates/core/src/client_manager.rs` - remove wasm32 gate
- `crates/core/src/ui/settings/accounts.rs` - build wizard UI
- `apps/web/Cargo.toml` - may need feature updates
