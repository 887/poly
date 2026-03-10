# Memory: Current MCP build-output gap

*Stored: 2026-03-10T13:52:00.320149881+00:00*

---

Verified current state across the three devtools MCP backends:

- `mcp/devtools-protocol` has no shared structured build-diagnostics API/tool; only lifecycle + extension hooks.
- `desktop-devtools-mcp` launches `dx serve --hotpatch` with `stdout(Stdio::null())` and only inherits stderr, so MCP users cannot query the last Dioxus build output.
- `web-devtools-mcp` launches `dx serve` with `stdout(Stdio::null())` and `stderr(Stdio::inherit())`; `force_rebuild` also discards both stdout/stderr.
- `electron-devtools-mcp` runs `dx build --platform web` with `stdout(Stdio::null())` and `stderr(Stdio::inherit())`; npm/electron steps also discard or only inherit logs.
- Existing `get_generation` counters are useful, but when generation/build behavior is confusing there is no MCP/CLI tool to retrieve the actual last build status/output/failure reason.

Implementation direction: add a shared build diagnostics type + default backend methods in `poly-devtools-protocol`, then implement `get_last_build_status` / `get_last_build_log` extension or standard tools across desktop/web/electron backed by captured command output and richer status tracking.
