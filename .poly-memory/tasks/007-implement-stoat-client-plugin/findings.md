# Research Findings — Task: Implement Stoat client plugin

*Auto-updated by poly-memory-mcp. Add findings via CLI or MCP tool.*

---


## Finding 2026-03-16T21:39:35Z

Verified next Stoat slice inputs on 2026-03-16:

1. **Real WIT/shared-type drift exists right now**
   - `poly_client::Session` includes `backend_url: Option<String>` but `wit/messenger-plugin.wit` `record session` does not.
   - `poly_client::Server` includes `banner_url: Option<String>` but `wit/messenger-plugin.wit` `record server` does not.
   - `crates/plugin-host/src/bridge.rs` is therefore dropping both (`backend_url: None`, `banner_url: None`), and `clients/demo/src/guest.rs` also omits them in guest-side conversion.

2. **Stoat message retrieval contract is ready for implementation**
   - Endpoint: `GET /channels/{target}/messages`
   - Query params: `limit`, `before`, `after`, `sort`, `nearby`, `include_users`
   - `MessageQuery::around` should map to Stoat `nearby`
   - `BulkMessageResponse` is polymorphic:
     - either a plain `array<Message>`
     - or an object `{ messages, users, members? }`

3. **Useful Stoat message payload details**
   - `Message` carries `_id`, `channel`, `author`, optional embedded `user`, optional `member`, `content`, `attachments`, `edited`, `replies`, `reactions`, etc.
   - `File` carries `_id`, `tag`, `filename`, `content_type`, `size`, and root config `features.autumn.url` gives the file-service base URL.

Design consequence: the next patch should update WIT + bridge + guest conversions first, then implement native `get_messages()` with support for both `BulkMessageResponse` shapes and attachment URLs derived from the Autumn base service URL.

---


## Finding 2026-03-16T21:53:48Z

Follow-up validation finding on 2026-03-16 after extending WIT `session.backend-url` and `server.banner-url`:

- The WIT/bridge/demo side is now synchronized and passes:
  - `cargo test -p poly-stoat`
  - `cargo component build -p poly-stoat --target wasm32-wasip2`
  - `cargo component build -p poly-demo --target wasm32-wasip2`
  - `cargo test -p poly-plugin-loader-tests --features test-stoat --test client_e2e`
  - `cargo check --workspace`
  - `cargo cranky --workspace --all-targets -- -D warnings`

- While attempting broader all-features validation, four other stub client guests (`matrix`, `discord`, `teams`, `server-client`) were still using the outdated plain `export!(...)` pattern and lacked required plugin-metadata guest impls. Those stubs were updated to:
  - import `export`
  - use `export!(... with_types_in crate::wit_bindings)`
  - provide minimal `PluginMetadataGuest` impls

- Remaining cross-plugin blocker discovered:
  - `cargo component build -p poly-server-client --target wasm32-wasip2` still fails for a **separate pre-existing reason**: its dependency graph pulls in Tokio/OpenSSL/native networking that do not currently compile for the WASI component target (`tokio` unsupported features on wasm + `openssl-sys` cross-target failure).
  - This prevents a clean `cargo test -p poly-plugin-loader-tests --all-features` run in this environment, but it is not caused by the Stoat changes.

This should be treated as a separate WASM-boundary cleanup task for `poly-server-client`, not a regression in the Stoat message/WIT slice.

---
