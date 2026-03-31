# poly-devtools-protocol — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-03-10


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

## Standard MCP Tools

The shared tool surface now includes the cross-backend build diagnostics workflow:

- `launch_app`
- `kill_app`
- `connect_cdp` / `cdp_status`
- `rebuild_app`
- `get_last_build_status` ← structured JSON summary of the last Dioxus build/hotpatch attempt
- `get_last_build_log` ← raw captured Dioxus CLI / compiler output for the last attempt
- `reset_app`
- plus the existing screenshot / snapshot / script / console / navigation / input tools

## Build Diagnostics Protocol (NEW — 2026-03-10)

`poly-devtools-protocol` now defines the shared last-build diagnostics surface used by all three devtools MCP backends:

- `get_last_build_status` — returns structured JSON including trigger, mode, command line,
  working directory, lifecycle state, exit code, timing, verification notes, and a log excerpt
- `get_last_build_log` — returns the raw captured Dioxus stdout/stderr for the most recent build attempt

**Mandatory agent workflow:** if generation / rebuild counters do not move as expected,
inspect `get_last_build_status` and `get_last_build_log` before concluding the build is stuck or failed.

## Shared MCP Timeout Enforcement (NEW — 2026-03-10)

`dispatch_tool()` now wraps **every** standard and extension tool call in a timeout budget derived from
`DevtoolsBackend::tool_timeout_ms(name, args)`.

This means:

- MCP calls should fail with a timeout error instead of hanging forever
- timeouts are now part of the expected debugging workflow
- backend authors should override `tool_timeout_ms(...)` if a custom tool legitimately needs more time

When you see a timeout error, interpret it as a probable hung renderer / transport issue, not as missing output.

## Key Files

| File | Purpose |
|---|---|
| `src/lib.rs` | Crate entry point |
| `src/backend.rs` | `DevtoolsBackend` trait + types |
| `src/mcp.rs` | MCP main loop + JSON-RPC helpers |

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
