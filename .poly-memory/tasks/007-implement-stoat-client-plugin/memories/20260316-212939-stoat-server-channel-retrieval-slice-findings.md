# Memory: Stoat server-channel retrieval slice findings

*Stored: 2026-03-16T21:29:39.055902958+00:00*

---

Implemented the next interruptible Stoat native slice on 2026-03-16:

## Key protocol finding
The published Stoat REST schema in `clients/stoat/api-1.json` exposes:
- `GET /servers/{id}`
- `GET /channels/{id}`
- `GET /sync/unreads`

but does **not** expose an obvious joined-server collection endpoint like `GET /servers` for the current authenticated account.

Design consequence for Poly's current slice:
- `get_servers()` remains `ClientError::NotSupported(...)` for now and should eventually be backed by websocket ready-state / sync cache or a newly discovered collection endpoint.
- `get_server(id)` now uses `GET /servers/{id}`.
- `get_channels(server_id)` now fetches the server first for its channel IDs, then fetches each channel with `GET /channels/{id}`.
- `get_channel(id)` now fetches a single channel directly.
- `/sync/unreads` now enriches server/channel mention counts and provides a conservative unread estimate (`mentions.len()` or at least `1` if unread state exists).

## Code added
- `clients/stoat/src/api.rs`
  - `StoatServer`, `StoatCategory`, `StoatChannel`, `StoatChannelUnread`
  - Poly mapping helpers for server/channel/category/unread shapes
- `clients/stoat/src/http.rs`
  - `fetch_server`
  - `fetch_channel`
  - `fetch_unreads`
  - session state now also stores authenticated account display name
- `clients/stoat/src/lib.rs`
  - implemented native `get_server`
  - implemented native `get_channels`
  - implemented native `get_channel`
  - `get_servers` now explicitly reports why it is not yet supported
- `clients/stoat/tests/integration.rs`
  - added mock HTTP coverage for server detail, channel list/detail, unread mapping, and DM-channel rejection

## Validation completed
- `cargo test -p poly-stoat`
- `cargo check -p poly-stoat`
- `cargo cranky -p poly-stoat --all-targets -- -D warnings`
- `cargo component build -p poly-stoat --target wasm32-wasip2`
- `cargo test -p poly-plugin-loader-tests --features test-stoat --test client_e2e`
- `cargo fmt --all`
- `cargo check --workspace`
- `cargo cranky --workspace --all-targets -- -D warnings`

## Recommended next small step
Keep the next Stoat slice narrowly focused on **joined-server discovery + message retrieval**:
1. confirm whether Bonfire ready-state / websocket initial sync provides the current server list
2. if yes, add a small cache-backed `get_servers()` path
3. then implement `get_messages(channel_id, query)` using the now-working channel lookup foundation

