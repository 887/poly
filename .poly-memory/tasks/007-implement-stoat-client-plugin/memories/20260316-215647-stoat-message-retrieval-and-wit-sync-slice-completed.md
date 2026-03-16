# Memory: Stoat message retrieval and WIT sync slice completed

*Stored: 2026-03-16T21:56:47.369891886+00:00*

---

Completed the next Stoat implementation slice on 2026-03-16.

Implemented:
- `clients/stoat/src/api.rs`
  - added typed Stoat root feature models (`features.autumn.url`)
  - added `BulkMessageResponse` support for both array and expanded envelope shapes
  - added typed message/member/webhook/file models
  - added message → `poly_client::Message` mapping with:
    - bundled user/member display-name resolution
    - reaction mapping with `me` detection
    - reply preview hydration when the referenced message is in the batch
    - best-effort Autumn attachment URLs (`autumn.url + /{tag}/{id}`)
    - ULID timestamp decoding for chronological sorting
- `clients/stoat/src/http.rs`
  - added `fetch_messages(channel_id, &MessageQuery)`
  - mapped Poly `before` / `after` / `around` / `limit` into Stoat `before` / `after` / `nearby` / `limit`
- `clients/stoat/src/lib.rs`
  - implemented native `get_messages(channel_id, query)`
  - now sorts returned Stoat windows chronologically after fetch
  - `get_server(id)` now also uses root config for icon/banner URL derivation

WIT / plugin-boundary sync completed in the same slice:
- `wit/messenger-plugin.wit`
  - `record session` now includes `backend-url`
  - `record server` now includes `banner-url`
- `crates/plugin-host/src/bridge.rs`
  - preserves both fields when converting WIT → `poly-client`
- `clients/demo/src/guest.rs`
  - now exports both fields back through the guest boundary

Additional validation cleanup discovered and fixed:
- `clients/matrix`, `clients/discord`, `clients/teams`, and `clients/server-client` stub guests still used the outdated plain `export!(...)` pattern.
- Updated all four to:
  - import `export`
  - use `export!(... with_types_in crate::wit_bindings)`
  - implement minimal `PluginMetadataGuest`
- `clients/server-client` WASM build must still use the crate-specific documented command:
  `cargo component build -p poly-server-client --target wasm32-wasip2 --no-default-features`

Validation completed successfully:
- `cargo fmt --all`
- `cargo test -p poly-stoat`
- `cargo component build -p poly-stoat --target wasm32-wasip2`
- `cargo component build -p poly-demo --target wasm32-wasip2`
- `cargo component build -p poly-matrix --target wasm32-wasip2`
- `cargo component build -p poly-discord --target wasm32-wasip2`
- `cargo component build -p poly-teams --target wasm32-wasip2`
- `cargo component build -p poly-server-client --target wasm32-wasip2 --no-default-features`
- `cargo test -p poly-plugin-loader-tests --features test-stoat --test client_e2e`
- `cargo test -p poly-plugin-loader-tests --all-features`
- `cargo check --workspace`
- `cargo cranky --workspace --all-targets -- -D warnings`

Recommended next small step:
- implement `send_message()` / `send_reply_message()` for Stoat using `POST /channels/{target}/messages`
- keep the next slice limited to text + reply send first
- leave edit/delete/pin/search for the slice after that
