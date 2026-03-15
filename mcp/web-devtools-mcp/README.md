# poly-web-devtools-mcp

MCP server that exposes the **Poly** web app to GitHub Copilot as a set of devtools tools — screenshot, click, JS eval, DOM inspection, console access, and browser lifecycle management.

> **Prefer CLI over MCP where possible.**  
> The CLI mode is faster, directly testable in a terminal, and doesn't require the full JSON-RPC handshake.  
> Use MCP (via `.vscode/mcp.json`) only when writing agent prompts that need to orchestrate multi-step sequences through Copilot.

## Why It Exists

Copilot agents need a way to "see" and interact with the running Poly web UI without a human in the loop. Unlike the desktop target (WebKit2GTK), the web app runs in Chromium, which supports the **Chrome DevTools Protocol (CDP)** natively. This MCP server launches Chromium, connects to CDP over a WebSocket, and exposes a consistent set of tools that mirror `poly-desktop-devtools-mcp`.

## Architecture

```
GitHub Copilot (MCP client)
        │  JSON-RPC 2.0 over stdio
        ▼
poly-web-devtools-mcp  ← this crate
        │  CDP over WebSocket  localhost:9222
        ▼
Chromium (--remote-debugging-port=9222)
        │  loads
        ▼
Poly web app  (dx serve, localhost:3000)
```

> **Important:** the web dev server must run on **port 3000** with
> `dx serve --platform web --port 3000`.
> Do **not** use `--hotpatch` for web/WASM on Dioxus 0.7.3 — it can leave the
> browser stuck showing the rebuild toast.

## CLI Access (PREFERRED)

All MCP tools are also available as direct CLI subcommands:

```bash
# Check CDP connection
cargo run --bin poly-web-devtools-mcp -- status

# Start dx serve + Chromium
cargo run --bin poly-web-devtools-mcp -- launch

# Stop processes
cargo run --bin poly-web-devtools-mcp -- kill

# Take a screenshot
cargo run --bin poly-web-devtools-mcp -- screenshot
cargo run --bin poly-web-devtools-mcp -- screenshot --save devtools-screenshots/snap.png

# DOM snapshot
cargo run --bin poly-web-devtools-mcp -- snapshot
cargo run --bin poly-web-devtools-mcp -- snapshot --verbose

# Evaluate JavaScript
cargo run --bin poly-web-devtools-mcp -- eval "document.title"

# Click a CSS selector
cargo run --bin poly-web-devtools-mcp -- click "#my-button"

# Fill an input
cargo run --bin poly-web-devtools-mcp -- fill "#username" "alice"

# Navigate to a route
cargo run --bin poly-web-devtools-mcp -- navigate "http://localhost:3000/settings"

# Get rebuild generation counters
cargo run --bin poly-web-devtools-mcp -- generation

# Show help
cargo run --bin poly-web-devtools-mcp -- help
```

Add `--headless` before the subcommand to run Chromium headlessly:

```bash
cargo run --bin poly-web-devtools-mcp -- --headless screenshot --save snap.png
```

VS Code tasks for the most common CLI commands are defined in `.vscode/tasks.json` under "CLI: web — *".

## MCP Tools Exposed

| Tool | Description |
|---|---|
| `launch_app` | Start Chromium navigated to the Poly web server |
| `kill_app` | Stop Chromium |
| `connect_cdp` | Connect (or reconnect) to the CDP WebSocket |
| `screenshot` | Capture the viewport as a PNG via CDP `Page.captureScreenshot` |
| `js_eval` | Evaluate JavaScript via CDP `Runtime.evaluate` |
| `get_dom` | Return `outerHTML` of the document |
| `get_console` | Return buffered `console.*` output |
| `click` | Dispatch a click at (x, y) via CDP `Input.dispatchMouseEvent` |
| `type_text` | Type text via CDP `Input.dispatchKeyEvent` |
| `reset_app` | Navigate to `/` to reset app state |

### Mobile Emulation Workflow

For Poly's mobile UI testing flow:

1. open `http://127.0.0.1:3000/?mobile=1`
2. call `set_viewport` with a phone-sized viewport
3. optionally enable Chromium mobile metrics / touch emulation through the same tool

Example:

```json
{
        "width": 393,
        "height": 852,
        "mobile": true,
        "deviceScaleFactor": 3,
        "touch": true
}
```

## VS Code Integration

- **MCP (Copilot agent):** configured in `.vscode/mcp.json` as `poly-web`
- **CLI (terminal):** tasks in `.vscode/tasks.json` under "CLI: web — *"

## Key Implementation Notes

- Uses `tokio-tungstenite` for the CDP WebSocket connection
- Chrome crashes / exits are detected and automatically restart + reconnect
- Both `dx serve` (port 3000) and Chromium (CDP port 9222) must be running for tools to work

## License

MIT / Apache-2.0

