# Memory: Stoat transport slice completed

*Stored: 2026-03-16T20:54:03.492541441+00:00*

---

Completed the first interruptible Stoat implementation slice on 2026-03-16.

What was implemented:
- `clients/stoat/src/config.rs`
  - `StoatConfig` with normalized base URL handling
  - derived Bonfire websocket URL
  - derived route-safe `instance_id`
  - `StoatAuthInput` extracting only `Token` and `EmailPassword` credentials from `poly-client::AuthCredentials`
- `clients/stoat/src/http.rs`
  - isolated reqwest transport wrapper
  - session token state
  - request builder + authenticated request builder using Stoat's `x-session-token` header
- `clients/stoat/src/lib.rs`
  - `StoatClient` now owns isolated transport config instead of being an empty shell
  - added helpers for base URL / websocket URL / instance ID / request building / loading a persisted token
- `clients/stoat/src/guest.rs`
  - fixed WASM export wiring for current `wit-bindgen` syntax
  - added required `plugin_metadata::Guest` stub implementation
- `clients/stoat/src/wit_bindings.rs`
  - re-exported plugin metadata guest/types needed by the guest module

Important lessons discovered:
- Because `wit_bindgen::generate!` lives in `src/wit_bindings.rs`, the correct export syntax is:
  `export!(StoatPlugin with_types_in crate::wit_bindings)`
- The `messenger-plugin` world requires implementing `plugin_metadata::Guest` even for stub plugins.
- The generated export stubs require `#![allow(unsafe_code)]` at the guest module level; the attribute on the macro invocation itself is ignored.

Validation completed successfully:
- `cargo test -p poly-stoat`
- `cargo check -p poly-stoat`
- `cargo component build -p poly-stoat --target wasm32-wasip2`
- `cargo cranky -p poly-stoat --all-targets -- -D warnings`
- `cargo fmt --all`
- `cargo check --workspace`
- `cargo cranky --workspace --all-targets -- -D warnings`

Docs updated:
- `docs/phase-3-plan.md` → checked off 3.1.2.1
- `clients/stoat/agents.md`
- `clients/stoat/README.md`

Recommended next small step:
- implement 3.1.2.2 authentication plumbing only:
  - add typed Stoat login request/response models
  - POST `/auth/session/login`
  - store session token on success
  - fetch current user profile (likely `/users/@me` or equivalent) to build `poly_client::Session`
Do NOT jump to channels/messages yet; keep the next slice strictly auth-focused.
