# Plan: networked messenger plugins build for wasm32-wasip2

## Status: ✅ DONE — all 8 messenger plugins build for wasm32-wasip2 and load/unload (2026-06-03, change `ykxlqqmn`)

## Problem
Only `poly-demo` builds as a `wasm32-wasip2` component. The 7 networked
plugins (stoat, discord, matrix, teams, lemmy, forgejo, server-client) fail
with `socket2`/`openssl-sys` "doesn't support the compile target".

## Root cause
The WIT-import HTTP transport already exists in each guest
(`<crate>/src/guest.rs` calls `wit_bindings::poly::messenger::host_api::http_request`).
The failure is **dependency cfg-gating**: browser-only deps leak into the
wasip2 build because the code uses `cfg(target_arch = "wasm32")` as a proxy for
"browser", but `wasm32-wasip2` is *also* `target_arch = "wasm32"`.

Two leak shapes:
- **Group 1 (stoat, discord):** a non-optional
  `[target.'cfg(target_arch = "wasm32")'.dependencies]` block pulls
  `poly-host-bridge` (→ reqwest → openssl/socket2) + `gloo-net` + `web-sys`.
  Leaks even under `--no-default-features`.
- **Group 2 (matrix, teams, lemmy, forgejo, server-client):**
  `poly-host-bridge`/`reqwest`/`tokio` are `optional`, enabled only by the
  `native` feature (the default). Leaks only because the build uses default
  features. `--no-default-features` excludes them.

`poly-client` is clean (its reqwest/tokio are dev-only). No host-bridge change
is needed once guests stop pulling it on wasip2.

## Fix shape
- Group 1: narrow the browser dep block + browser `mod` gates from
  `cfg(target_arch = "wasm32")` → `cfg(all(target_arch = "wasm32", target_os = "unknown"))`.
  Browser (wasm32-unknown-unknown) behavior is identical; only wasip2 stops
  pulling the deps.
- Group 2: build wasip2 with `--no-default-features`.
- Harness: `crates/plugin-host-tests/src/lib.rs` `ensure_wasm_built` must pass
  `--no-default-features` for the networked crates so `cargo test` rebuilds
  match.

## THIRD LAYER discovered during Phase A (2026-06-03)
After fixing cfg-gating + nnnoiseless, stoat's wasip2 build gets PAST all
dependency errors and reveals the real reason the networked plugins broke:
**the guests' WIT export impls have drifted behind the evolved WIT contract.**
Because the wasip2 build has been broken for a long time, nobody caught the
guests falling out of sync. stoat alone is missing:
- `Guest::get_signup_method` (native: `is_backend.rs:650`)
- `Guest::get_account_overview_view` (native: `view_descriptor.rs:30`)
- the entire `client_config::Guest` interface (get/set client-version,
  get/set client-mechanisms)
Each of the 7 networked plugins needs its guest WIT exports resurrected to the
current contract, mirroring its native `is_backend`. This is per-plugin
implementation work (LSP/SOLID applies), not just config. Effort is ~7x stoat.

## Phases

### Phase A — Pilot: stoat
- [x] **A.1** Gate stoat `[target.'cfg(target_arch = "wasm32")'.dependencies]`
      → `cfg(all(target_arch = "wasm32", target_os = "unknown"))`. (change `ykxlqqmn`)
- [x] **A.2** Gate stoat browser `mod`s in lib.rs (13 attrs) →
      `all(target_arch="wasm32", target_os="unknown")`. (change `ykxlqqmn`)
- [x] **A.2b** nnnoiseless `default-features = false` (drops clap 3 → os_str_bytes
      nightly `wasip2` feature). (change `ykxlqqmn`)
- [x] **A.3** Resurrect stoat guest WIT exports (get_signup_method,
      get_account_overview_view, client_config::Guest) mirroring native. (change `ykxlqqmn`)
- [x] **A.4** `cargo component build -p poly-stoat --target wasm32-wasip2 --no-default-features` clean → poly_stoat.wasm (2.0M). (change `ykxlqqmn`)

### Phase B — discord (Group 1: Cargo dep block + lib.rs + WIT exports)
- [x] **B.1** Cargo + lib.rs gating + WIT exports (get_signup_method,
      get_account_overview_view, ClientConfigGuest). (subagent, integrated)
- [x] **B.2** wasip2 build clean → poly_discord.wasm (1.9M). Native unregressed.

### Phase C — Group 2: matrix, teams, lemmy, forgejo, server-client
- [x] **C.1** Each builds wasip2 with `--no-default-features` + WIT exports
      resurrected (matrix/teams/server-client: ClientConfigGuest + 2 methods;
      forgejo: full guest.rs+wit_bindings.rs created; lemmy: already correct).
      (subagents, integrated)
- [x] **C.2** All five wasip2 builds clean (matrix 1.9M, teams 2.0M, lemmy 1.9M,
      forgejo 1.8M, server-client 1.5M). All native unregressed.
- [x] **C.3** demo also fixed: its `MessagingBackend` impl was missing the
      `#[cfg(feature="native")]` gate (only built before because the harness
      left native on). Now builds wasip2 with `--no-default-features` (2.3M).

### Phase D — Harness + verify + cache
- [x] **D.1** Updated `plugin-host-tests` `ensure_wasm_built` to `--no-default-features`.
- [x] **D.2** `cargo test -p poly-plugin-host-tests --test integration` — all 8
      load/unload PASS (test extended from 6→8 plugins). 38 client_e2e tests also pass.
- [x] **D.3** `cargo cranky --workspace` four-guard unchanged: 541 lints all in
      `clients/discord/` (pre-existing deferred discord-voice surface), zero
      non-discord locations. No new lints from the wasm work.
- [x] **D.4** Populated wasm side of sccache (all 8 components built under sccache).
- [x] **D.5** Plan DONE.

## Verification
`cargo component build -p <crate> --target wasm32-wasip2 --no-default-features`
must finish with no socket2/openssl error, emitting the `.wasm` under
`target/wasm32-wasip2/debug/`.
