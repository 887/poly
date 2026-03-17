# poly-electron-devtools-mcp

MCP server that exposes the **Poly** Desktop Electron app to GitHub Copilot as a set of devtools tools — screenshot, click, JS eval, DOM inspection, console access, and Electron lifecycle management.

> **Prefer CLI over MCP where possible.**  
> The CLI mode is faster, directly testable in a terminal, and doesn't require the full JSON-RPC handshake.  
> Use MCP (via `.vscode/mcp.json`) only when writing agent prompts that need to orchestrate multi-step sequences through Copilot.

## Why It Exists

The Poly Electron build (`apps/desktop-electron`) packages the Dioxus WASM web app inside an Electron shell. Like the web MCP, this server connects to Electron via the **Chrome DevTools Protocol (CDP)** — but on a separate port (9224) so it can coexist with `poly-web-devtools-mcp`.

## Architecture

```
GitHub Copilot (MCP client)
        │  JSON-RPC 2.0 over stdio
        ▼
poly-electron-devtools-mcp  ← this crate
        │  CDP over WebSocket  localhost:9224
        ▼
Electron (--remote-debugging-port=9224)
        │  loads (via localhost HTTP server)
        ▼
Poly Electron app  (apps/desktop-electron WASM bundle)
```

## CLI Access (PREFERRED)

All MCP tools are also available as direct CLI subcommands:

```bash
# Check CDP connection / get status
cargo run --bin poly-electron-devtools-mcp -- status

# Build WASM bundle + launch Electron
cargo run --bin poly-electron-devtools-mcp -- launch
cargo run --bin poly-electron-devtools-mcp -- launch /path/to/workspace

# Stop Electron
cargo run --bin poly-electron-devtools-mcp -- kill

# Take a screenshot (inline by default; save only if you need a file)
cargo run --bin poly-electron-devtools-mcp -- screenshot
cargo run --bin poly-electron-devtools-mcp -- screenshot --save devtools-screenshots/snap.png

# DOM snapshot
cargo run --bin poly-electron-devtools-mcp -- snapshot
cargo run --bin poly-electron-devtools-mcp -- snapshot --verbose

# Evaluate JavaScript
cargo run --bin poly-electron-devtools-mcp -- eval "document.title"

# Click a CSS selector
cargo run --bin poly-electron-devtools-mcp -- click "#my-button"

# Fill an input
cargo run --bin poly-electron-devtools-mcp -- fill "#username" "alice"

# Navigate to a URL
cargo run --bin poly-electron-devtools-mcp -- navigate "http://localhost:8765"

# Get rebuild generation counters
cargo run --bin poly-electron-devtools-mcp -- generation

# Show help
cargo run --bin poly-electron-devtools-mcp -- help
```

The workspace root is auto-detected from the `POLY_WORKSPACE` env var or the current working directory. VS Code tasks for the most common CLI commands are defined in `.vscode/tasks.json` under "CLI: electron — *".

**Screenshot policy:** prefer screenshot commands **without** `--save` so the image is returned inline. Use `--save` only when you explicitly want a saved artifact.

## MCP Tools Exposed

| Tool | Description |
|---|---|
| `launch_app` | Build WASM (`dx build --platform web`) + launch Electron |
| `kill_app` | Stop Electron |
| `connect_cdp` | Connect (or reconnect) to CDP on port 9224 |
| `screenshot` | Capture the viewport as PNG via CDP `Page.captureScreenshot` |
| `js_eval` | Evaluate JavaScript via CDP `Runtime.evaluate` |
| `get_dom` | Return `outerHTML` of the document |
| `get_console` | Return buffered `console.*` output |
| `click` | Dispatch a click via CDP |
| `type_text` | Type text via CDP `Input.dispatchKeyEvent` |
| `reset_app` | Navigate back to the app root URL |
| `rebuild_app` | Rebuild the WASM bundle + reload (`Page.reload`) |
| `force_rebuild` | Same as `rebuild_app` — always a full `dx build` |
| `get_generation` | Return `{ generation, build_id, electron_pid }` counters |

## VS Code Integration

- **MCP (Copilot agent):** configured in `.vscode/mcp.json` as `poly-electron`
- **CLI (terminal):** tasks in `.vscode/tasks.json` under "CLI: electron — *"

## Key Implementation Notes

- **CDP port:** 9224 (distinct from web 9222 and desktop HTTP 9223)
- **Build command:** `dx build --platform web` (one-shot, not `dx serve`)
- **Rebuild counter:** `/tmp/poly-devtools-electron-rebuild-counter`
- Electron is launched with `disable-dev-shm-usage` + `no-zygote` flags on Linux for stable CDP
- `launch_app` kills any stale Electron devtools processes before building so you always get a clean start
- The WASM bundle is served over loopback HTTP (not `file://`) so absolute `/wasm/…` and `/assets/…` paths resolve correctly

## License

MIT / Apache-2.0
