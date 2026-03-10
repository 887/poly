# Memory: Devtools MCP last-build diagnostics added

*Stored: 2026-03-10T14:06:37.560482132+00:00*

---

Added shared cross-backend Dioxus build diagnostics across `poly-devtools-protocol`, `poly-desktop-devtools-mcp`, `poly-web-devtools-mcp`, and `poly-electron-devtools-mcp`.

### New shared tools
- `get_last_build_status` — structured JSON summary of the last Dioxus build/hotpatch attempt
- `get_last_build_log` — raw captured stdout/stderr for the last attempt

### Shared protocol changes
- `mcp/devtools-protocol/src/backend.rs` now defines `BuildLifecycleState`, `BuildDiagnostics`, and `RollingBuildLog`.
- `DevtoolsBackend` now has default methods for `get_last_build_status()` and `get_last_build_log()`.
- `mcp/devtools-protocol/src/mcp.rs` exposes the two tools as standard MCP tools and updates `rebuild_app` guidance to tell agents to inspect them when generation is ambiguous.

### Backend behavior
- **Desktop MCP** now captures long-lived `dx serve --hotpatch` stdout/stderr via piped log readers and slices the rolling log per launch/rebuild/force-rebuild attempt.
- **Web MCP** now captures long-lived `dx serve` stdout/stderr, classifies watcher rebuild output heuristically (success/failure/unknown), and records structured status for launch/rebuild/force-rebuild.
- **Electron MCP** now captures exact `dx build --platform web` output for launch/rebuild and stores structured diagnostics for those one-shot builds.
- All three backends now expose matching CLI commands: `build-status` and `build-log`.
- All three `get_generation` tool descriptions were updated to explicitly tell agents to inspect the new build-diagnostics tools whenever counters do not change as expected.

### Documentation updated
- Root `agents.md`
- `mcp/devtools-protocol/agents.md`
- `mcp/desktop-devtools-mcp/agents.md`
- `mcp/web-devtools-mcp/agents.md`
- `mcp/electron-devtools-mcp/agents.md`

### Validation
- `cargo fmt --all` ✅
- `cargo check --workspace` ✅
- `cargo cranky --workspace` ✅
