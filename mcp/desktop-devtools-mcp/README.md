# poly-desktop-devtools-mcp

MCP server that exposes the **Poly** desktop app to GitHub Copilot as a set of devtools tools — screenshot, click, JS eval, DOM inspection, console access, and app lifecycle management.

> **Prefer CLI over MCP where possible.**  
> The CLI mode is faster, directly testable in a terminal, and doesn't require the full JSON-RPC handshake.  
> Use MCP (via `.vscode/mcp.json`) only when writing agent prompts that need to orchestrate multi-step sequences through Copilot.

## Why It Exists

Copilot agents need a way to "see" and interact with the running Poly UI without a human in the loop. The Chrome DevTools Protocol (CDP) is unavailable on WebKit2GTK (which backs Dioxus's desktop target on Linux). This MCP server bridges that gap by talking to `apps/desktop-devtools`, which embeds a small Axum HTTP eval-bridge on port 9223.

## Architecture

```
GitHub Copilot (MCP client)
        │  JSON-RPC 2.0 over stdio
        ▼
poly-desktop-devtools-mcp  ← this crate
        │  HTTP  localhost:9223
        ▼
apps/desktop-devtools  (eval-bridge embedded in the Dioxus app)
        │  dioxus eval() / WebKit2GTK snapshot API
        ▼
WebKit webview running poly-core UI
```

## CLI Access (PREFERRED)

All MCP tools are also available as direct CLI subcommands — no JSON-RPC protocol overhead:

```bash
# Check if the devtools app is running
cargo run --bin poly-desktop-devtools-mcp -- status

# Start the devtools app
cargo run --bin poly-desktop-devtools-mcp -- launch

# Stop the devtools app
cargo run --bin poly-desktop-devtools-mcp -- kill

# Take a screenshot (inline by default; save only if you need a file)
cargo run --bin poly-desktop-devtools-mcp -- screenshot
cargo run --bin poly-desktop-devtools-mcp -- screenshot --save devtools-screenshots/snap.png

# DOM snapshot
cargo run --bin poly-desktop-devtools-mcp -- snapshot
cargo run --bin poly-desktop-devtools-mcp -- snapshot --verbose

# Evaluate JavaScript
cargo run --bin poly-desktop-devtools-mcp -- eval "document.title"

# Click a CSS selector
cargo run --bin poly-desktop-devtools-mcp -- click "#my-button"

# Fill an input
cargo run --bin poly-desktop-devtools-mcp -- fill "#username" "alice"

# Navigate to a route
cargo run --bin poly-desktop-devtools-mcp -- navigate "/settings"

# Get rebuild/hotpatch generation counters
cargo run --bin poly-desktop-devtools-mcp -- generation

# Show help
cargo run --bin poly-desktop-devtools-mcp -- help
```

VS Code tasks for the most common CLI commands are defined in `.vscode/tasks.json` under "CLI: desktop — *".

**Screenshot policy:** prefer screenshot commands **without** `--save` so the image is returned inline. Use `--save` only when you explicitly want an artifact on disk.

## MCP Tools Exposed

| Tool | Description |
|---|---|
| `launch_app` | Start the devtools app (health-checks first — no double-instances) |
| `kill_app` | Stop the devtools app |
| `connect_cdp` | Verify the eval-bridge is reachable |
| `screenshot` | Capture viewport as PNG (coordinates are 1:1 CSS pixels) |
| `js_eval` | Evaluate arbitrary JavaScript |
| `get_dom` | Return `outerHTML` of the document |
| `get_console` | Return buffered `console.*` output |
| `click` | Dispatch full pointer/mouse/click event sequence at (x, y) |
| `type_text` | Type text into the focused element |
| `reset_app` | Wipe local data and restart at the setup wizard |

## VS Code Integration

- **MCP (Copilot agent):** configured in `.vscode/mcp.json` as `poly-desktop`
- **CLI (terminal):** tasks in `.vscode/tasks.json` under "CLI: desktop — *"

## Key Implementation Notes

- `launch_app` pings `/status` first; reuses the running instance rather than respawning
- `click` uses `elementFromPoint` + 5-event sequence (pointerdown→mousedown→pointerup→mouseup→click) for WebKit2GTK synthetic event acceptance
- Screenshot coordinates are exact because `apps/desktop-devtools` uses `SnapshotRegion::Visible`

## License

MIT / Apache-2.0

