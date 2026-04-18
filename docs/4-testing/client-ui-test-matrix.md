# Client UI Surface — Testing Matrix

> Last updated: 2026-04-18  
> Source: [`docs/plans/plan-client-ui-surface.md`](../plans/plan-client-ui-surface.md) §6A  
> How to run each layer: [`docs/plugin-testing.md`](../plugin-testing.md)

| Layer | Location | Run command | Current state | Extension for client UI surface |
|---|---|---|---|---|
| Unit tests | Inline `#[cfg(test)] mod tests` in each source file | `cargo test -p <crate> --lib` | Present across `crates/core`, `clients/*`, `clients/client` | Add modules for action-id parser, FTL key lookup, SVG sanitizer, build-route validator, cursor round-trip |
| Per-backend capability tests | `clients/<name>/tests/capabilities.rs` | `cargo test -p poly-<backend> --test capabilities` | Present for demo, discord, github, hackernews, lemmy, matrix, stoat, teams | Each backend asserts declared menu items, settings sections, sidebar layout, view descriptors, and composer buttons match its `BackendCapabilities` |
| Per-backend integration tests | `clients/<name>/tests/integration*.rs` | `cargo test -p poly-<backend> --test integration_test` | Present for all backends | Add round-trip assertions: declared action-id → `invoke_context_action` returns expected `ActionOutcome`; unknown id → `ClientError::NotFound` |
| Cross-backend parity | `clients/client/tests/client_ui_surface_parity.rs` | `cargo test -p poly-client --test client_ui_surface_parity` | Present (added in WP 1) | Every backend implements all 5 surfaces (D9 enforces at compile time); test pins runtime shape — e.g. every backend claiming `dms: true` returns explicit list for `Dm` target |
| E2E via WASM host | `crates/plugin-host-tests/tests/client_e2e/` | `cargo test -p poly-plugin-loader-tests --features test-<backend>` | Present; gated by `test-demo`, `test-discord`, `test-matrix`, `test-teams`, `test-server`, `test-stoat` | Extend `harness_*.rs` modules; every backend driver calls all applicable helpers; empty-list backends still assert explicit empty |
| Lint-gate scanner tests | `crates/lint-gate/src/lib.rs` `#[cfg(test)] mod tests` | `cargo test -p poly-lint-gate --lib` | Present (`context_menu_coverage`, `ui_action_coverage`, `action_enum_coverage`) | Add `ftl_label_key_coverage_*`, `action_id_naming_*`, `custom_block_usage_*`, `forbid_backend_slug_match_in_ui` tests |
| Trybuild compile-fail fixtures | `crates/ui-macros/tests/compile-fail-client-ui/*.rs` + `*.stderr` | `cargo test -p poly-ui-macros --test compile_fail_client_ui` | Present (added in WP 1) | Fixtures: missing FTL key for declared label-key → build error; non-kebab-case action id → build error; plugin missing required export → compile error |
| UI snapshots (Playwright) | `tests/snapshots/<backend>/<surface>.html` | poly-web MCP → `take_snapshot`; CI diffs vs golden | Golden files created in WP 0; updated per surface WP | Snapshot right-click menu (server/channel/user/message/dm), settings panel, sidebar, and forum/feed/issue view for every backend |
| MCP contract tests | `mcp/chat-mcp/tests/mcp_integration.rs` | `cargo test -p poly-chat-mcp --test mcp_integration` | Present | Add tests for `context_menu_<target>`, `invoke_context_action`, `plugin_settings_*`, `sidebar_*` MCP tools; tool-list filtered by backend capabilities |
