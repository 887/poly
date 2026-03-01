# poly-devtools-protocol

Shared protocol crate for the **Poly** devtools toolchain. Defines the `DevtoolsBackend` trait and the MCP stdio loop used by both `poly-desktop-devtools-mcp` and `poly-web-devtools-mcp`.

## Purpose

Without this crate, the two MCP servers would duplicate the MCP wire protocol and tool dispatch logic. This crate provides the common backbone so each MCP server only has to implement one trait.

## What It Provides

### `DevtoolsBackend` trait (`backend` module)

```rust
#[async_trait]
pub trait DevtoolsBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String>;
    async fn kill_app(&self) -> anyhow::Result<String>;
    async fn connect(&self) -> anyhow::Result<String>;
    async fn screenshot(&self) -> anyhow::Result<ScreenshotResult>;
    async fn js_eval(&self, expression: &str) -> anyhow::Result<String>;
    async fn get_dom(&self) -> anyhow::Result<String>;
    async fn get_console(&self) -> anyhow::Result<String>;
    async fn click(&self, x: i64, y: i64) -> anyhow::Result<String>;
    async fn type_text(&self, text: &str) -> anyhow::Result<String>;
    async fn reset_app(&self) -> anyhow::Result<String>;
}
```

### `run_mcp_loop` (`mcp` module)

Reads JSON-RPC 2.0 MCP messages from stdin, dispatches them to a `DevtoolsBackend` implementation, and writes responses to stdout. Handles tool listing, PNG screenshot encoding (base64), and error formatting.

## Users

| Crate | Backend impl |
|---|---|
| `poly-desktop-devtools-mcp` | `DesktopHttpBackend` — talks to `apps/desktop-devtools` over HTTP |
| `poly-web-devtools-mcp` | Chrome CDP over WebSocket |

## License

MIT / Apache-2.0
