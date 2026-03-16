# Memory: Stoat auth slice completed with E2E and spec

*Stored: 2026-03-16T21:21:06.341224040+00:00*

---

Completed the second Stoat implementation slice on 2026-03-16.

Implemented:
- `clients/stoat/src/api.rs` with typed Stoat models for:
  - root config (`GET /`)
  - login request/response (`POST /auth/session/login`)
  - user/profile + presence mapping (`GET /users/@me`)
- Native auth flow in `src/http.rs` + `src/lib.rs`:
  - email/password login
  - token restore using `X-Session-Token`
  - logout via `POST /auth/session/logout`
  - fetch-self -> `poly_client::Session` mapping
- Presence mapping rule: Stoat `Focus` and `Busy` map to `PresenceStatus::DoNotDisturb`
- Added `StoatClient::fetch_server_config()` for `GET /`

Spec/documentation added:
- `clients/stoat/SPEC.md` now documents:
  - auth/session contract
  - full Discord-like Stoat feature matrix gleaned from the OpenAPI + JS reference client
  - current/future E2E coverage matrix
  - prioritized implementation order
- Updated:
  - `docs/phase-3-plan.md`
  - `clients/stoat/agents.md`
  - `clients/stoat/README.md`

E2E / integration coverage added:
- `clients/stoat/tests/integration.rs`
  - root config round trip
  - email/password login success
  - token resume success
  - MFA branch error
  - disabled-account error
  - logout clears session
- `crates/plugin-host-tests/tests/client_e2e/stoat.rs`
  - added explicit stub check for `EmailPassword` auth returning error

Additional workspace fix discovered while validating E2E:
- `clients/demo/src/guest.rs` had the old broken wit-bindgen export invocation; fixed it to `export!(DemoPlugin with_types_in crate::wit_bindings)` with guest-level `#![allow(unsafe_code)]` so the plugin-host client_e2e suite can run again.

Validation completed successfully:
- `cargo test -p poly-stoat`
- `cargo check -p poly-stoat`
- `cargo cranky -p poly-stoat --all-targets -- -D warnings`
- `cargo component build -p poly-stoat --target wasm32-wasip2`
- `cargo component build -p poly-demo --target wasm32-wasip2`
- `cargo test -p poly-plugin-loader-tests --features test-stoat --test client_e2e`
- `cargo fmt --all`
- `cargo check --workspace`
- `cargo cranky --workspace --all-targets -- -D warnings`

Recommended next small step:
- implement server + channel retrieval only:
  - discover Stoat endpoint(s) for current account server list
  - implement `get_server()` / `get_channels(server_id)` / `get_channel()`
  - add mock-backed integration tests for server/channel mapping
  - leave messages and websocket events for the following slice
