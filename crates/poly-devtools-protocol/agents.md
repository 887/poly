# poly-devtools-protocol — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

Shared crate providing the `DevtoolsBackend` trait and MCP JSON-RPC protocol
handling used by both the desktop and web devtools MCP servers.

**You should never need to run this crate directly.** It is a library consumed by:
- `poly-desktop-devtools-mcp` (desktop HTTP backend)
- `poly-web-devtools-mcp` (web Chrome CDP backend)

---

## Module Map

| Module | Contents |
|---|---|
| `backend` | `DevtoolsBackend` trait, `ScreenshotResult`, `ConsoleEntry` |
| `mcp` | `run_mcp_loop()`, `standard_tool_list()`, `dispatch_tool()`, JSON-RPC helpers |

## DevtoolsBackend Trait

Standard methods every backend must implement:

| Method | Purpose |
|---|---|
| `launch_app(workspace)` | Build and launch the app under test |
| `kill_app()` | Kill the running app |
| `connect()` | Verify connectivity |
| `screenshot()` | Capture PNG screenshot |
| `js_eval(expr)` | Evaluate JavaScript, return result |
| `get_dom()` | Return full document HTML |
| `get_console()` | Return buffered console messages |
| `click(x, y)` | Simulate mouse click |
| `type_text(text)` | Type text into focused element |
| `reset_app()` | Reset to first-launch state |
| `navigate(route)` | Navigate to a route |

Extension point: backends can add custom tools via `extension_tools()` and
`handle_extension_tool()`.

## Standard MCP Tools (12)

`launch_app`, `kill_app`, `connect_cdp`, `cdp_status`, `screenshot`,
`js_eval`, `get_dom`, `get_console`, `click`, `type_text`, `reset_app`,
`navigate`

## Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | Crate entry point |
| `src/backend.rs` | `DevtoolsBackend` trait + types |
| `src/mcp.rs` | MCP main loop + JSON-RPC helpers |
