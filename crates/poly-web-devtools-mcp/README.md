# poly-web-devtools-mcp

MCP server that exposes the **Poly** web app to GitHub Copilot as a set of devtools tools — screenshot, click, JS eval, DOM inspection, console access, and browser lifecycle management.

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
Poly web app  (dx serve, localhost:8080)
```

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

## Usage

```bash
# Visible Chromium (default — useful for watching the agent work)
cargo run --bin poly-web-devtools-mcp

# Headless mode (CI / automated testing)
cargo run --bin poly-web-devtools-mcp -- --headless
```

## VS Code Integration

Configured in `.vscode/mcp.json` so Copilot loads it automatically when the workspace is open.

## Key Implementation Notes

- Uses `tokio-tungstenite` for the CDP WebSocket connection
- Chrome crashes / exits are detected and automatically restart + reconnect
- Both `dx serve` (port 8080) and Chromium (CDP port 9222) must be running for tools to work

## License

MIT / Apache-2.0
