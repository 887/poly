# poly-server-client — Agent Instructions

> **Read the root `agents.md` first.**
> **Last updated:** 2026-03-03 (after splitting poly_server out of poly-client)

---

## Purpose

This crate implements the complete client for talking to a running
poly-server instance.  It is consumed by `poly-core` via the shared
`poly-client` trait but is otherwise self-contained.  Everything that talks
directly to the server's HTTP or WebSocket APIs lives here.

The public API consists of three crates:

- `PolyServerHttpClient` — typed REST client (reqwest-based)
- `PolyServerWsClient` — WebSocket client with broadcast publisher
- `PolyServerBackend` — `ClientBackend` adapter used by the UI

In addition there are wire-format models, error types, and a handful of
utility helpers.

Because this crate is native-only it **must not** be enabled for WASM
builds; all feature gating and workspace dependencies already reflect that.

## Development notes

- **Tests:** The integration suite (`tests/integration.rs`) boots an in-process
  `poly-server` with a temporary SurrealKV database. Run as
  `cargo test -p poly-server-client -- --test-threads=1` (tests are
  currently serialized to avoid SurrealKV collisions).

- **Linting:** This crate has its own `cranky.toml` with the workspace lints.
  Run `cargo cranky -p poly-server-client` frequently; the workspace CI will
  treat any clippy warning as an error.

- **Hot-reload:** Not required—HTTP/WS client code rarely changes—but any
  structural change to shared models should be verified via `dx serve` in
  the desktop-devtools project since poly-core imports this crate.

- **Dependency updates:** Because this crate is upstream of several
  backends (poly-core, poly-client tests, demo data), keep its dependencies
  on the latest versions.  Check `last-crate-update-date` regularly.

## File structure

```
clients/server-client/
├── Cargo.toml           # crate manifest
├── agents.md            # this file
├── README.md            # overview of the client
├── src/
│   ├── lib.rs           # re-exports and visibility
│   ├── backend.rs       # PolyServerBackend implementation
│   ├── http.rs          # HTTP client definitions
│   ├── ws.rs            # WebSocket client
│   ├── models.rs        # serde wire-format types
│   ├── error.rs         # error type and Result alias
│   └── tests/           # integration test file(s)
└── tests/
    └── integration.rs   # end-to-end scenarios against a live server
```

## Building & testing

1. Ensure workspace is up-to-date:
   ```sh
   cargo update
   cargo cranky --workspace
   cargo check --workspace
   cargo check -p poly-web --target wasm32-unknown-unknown
   ```

2. Run this crate's tests:
   ```sh
   cargo test -p poly-server-client -- --test-threads=1
   ```

3. After making API changes also re-run the `poly-client` and `poly-core`
   integration suites to confirm downstream compatibility.

4. Format with:
   ```sh
   cargo fmt --all
   ```

## Crate-specific lints

Inherited from workspace but worth repeating here:

- `warnings = true` (treat all compiler warnings as errors)
- `unsafe_code = true` (no unsafe allowed)
- Deny `clippy::unwrap_used`, `expect_used`, `panic`, `indexing_slicing`,
  `print_stdout`, `print_stderr`.

Integration tests may use `allow(clippy::unwrap_used)` where appropriate.

## Notes

- The client is intentionally minimal; it mirrors the server API rather than
  inventing a separate abstraction layer.  Any new server endpoints should
  have corresponding methods/structs added here before the UI can consume
  them.

- Keep wire models (`models.rs`) in sync with server schema.  When the
  server changes, update here and add a migration in `tests/integration.rs`
  that exercises the new shape.
