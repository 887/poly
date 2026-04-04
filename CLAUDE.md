# CLAUDE.md — Poly Project Context

> Last updated: 2026-03-28

---

## !! MANDATORY — READ FIRST, ALWAYS !!

> Source: https://github.com/drona23/claude-token-efficient

- Think before acting. Read existing files before writing code.
- Be concise in output but thorough in reasoning.
- Prefer editing over rewriting whole files.
- Do not re-read files you have already read unless the file may have changed.
- No sycophantic openers or closing fluff.
- Keep solutions simple and direct. No over-engineering.
- If unsure: say so. Never guess or invent file paths.
- Read before writing. Understand the problem before coding.
- No redundant file reads. Read each file once.
- One focused coding pass. Avoid write-delete-rewrite cycles.
- Test once, fix if needed, verify once. No unnecessary iterations.
- Budget: 50 tool calls maximum. Work efficiently.

---

## Priority 2 — Use Jujutsu (jj) Instead of Git

- **Always use `jj` commands** for version control, never raw `git`
- `jj status`, `jj diff`, `jj log`, `jj show` for inspection
- `jj new`, `jj describe`, `jj commit` for creating changes
- `jj git push` to push to remote
- Only fall back to `git` if `jj` cannot accomplish the task

---

## Project Overview

**Poly** is an AI-powered social layer that unifies all your messaging platforms
(Discord, Matrix, Stoat, Teams, self-hosted) into one app — then adds an AI agent
that remembers your conversations, responds in your voice, manages your social
relationships, and acts as your external social memory.

Built with Rust, Dioxus 0.7.3, and WASM Component Model plugins. Two layers:

1. **Unified Chat UI** — 6 messenger backends via plugin architecture (demo, stoat,
   matrix, discord, teams, poly-server). One sidebar, one message view.
2. **Social Agent** (Phase 5) — MCP server exposing all chat backends to AI. Per-chat
   personality, conversation memory, typing simulation, outreach scheduling, digest
   briefings. Bring your own AI provider (Claude, GPT, Gemini, Ollama).

## Platform Targets

| App | Shell | Dev Server Port | Debug Port | MCP |
|-----|-------|----------------|------------|-----|
| `apps/web` | Chrome/Chromium | 3000 | 9222 (CDP) | `poly-web` |
| `apps/desktop` | `apps/desktop-web` (Wry) | 3002 | 9223 (HTTP eval) | `poly-desktop` |
| `apps/desktop-electron` | `apps/desktop-electron-web` (Electron) | 3001 | 9224 (CDP) | `poly-electron` |

## WASM Hot-Reload Architecture

All three platforms use the same pattern:
1. `dx serve --platform web --port <PORT>` compiles the app as WASM
2. A thin native shell (Chrome / Wry / Electron) loads from the dev server
3. On code changes, only the WASM reloads — the native window stays alive
4. The MCP reconnects via CDP or eval-bridge after each rebuild

### Key Files

| Shell | Source |
|-------|--------|
| Desktop Wry shell | `apps/desktop-web/src/main.rs` |
| Electron thin shell | `apps/desktop-electron-web/electron/main.js` |
| Desktop MCP | `mcp/desktop-devtools-mcp/src/main.rs` |
| Electron MCP | `mcp/electron-devtools-mcp/src/main.rs` |
| Web MCP | `mcp/web-devtools-mcp/src/main.rs` |
| Shared protocol | `mcp/devtools-protocol/src/` |

## Critical Implementation Notes

### ELECTRON_RUN_AS_NODE
VS Code and Claude Code terminals set `ELECTRON_RUN_AS_NODE=1`. This causes Electron
to run as plain Node.js where `require('electron')` fails. The MCPs strip this env var
when spawning Electron processes.

### Wry build_gtk
On Linux, `wry::WebViewBuilder::build_gtk()` must receive `window.default_vbox()`,
NOT `window.gtk_window()`. Using `gtk_window()` results in a 0x0 viewport.

### Electron Frameless Windows
Use `frame: false` only. Do NOT combine with `titleBarStyle: 'hidden'` or
`titleBarOverlay: false` — these conflict on Linux and cause pixel offsets.

### CSS Layout
`.main-layout` uses `height: 100%` (not `100vh`) so it respects the flex parent's
allocated size when the Electron custom titlebar (34px) is present.

### Screenshot Safety
All MCPs guard against 0x0 viewport screenshots in `devtools-protocol/src/mcp.rs`.
A 0x0 or sub-100-byte image returns a text error instead of sending a corrupt PNG
to the API.

### Orphan Process Cleanup
The Electron MCP kills stale processes by matching `poly-desktop-electron-web` in
the command line (catches main, GPU, network, renderer). The desktop MCP uses
`poly-desktop-web` pattern. Both also kill by dx serve port pattern.

### Desktop WASM Compatibility
`apps/desktop/Cargo.toml` uses cfg-gated dependencies:
- Native: `dioxus = ["desktop"]`, `tokio`, `tracing-subscriber`
- WASM: `dioxus = ["web"]`, `getrandom04-wasm`

## Build Commands

```bash
# Build all MCPs
cargo build -p poly-desktop-devtools-mcp -p poly-electron-devtools-mcp -p poly-web-devtools-mcp

# Build desktop Wry shell
cargo build -p poly-desktop-web

# Test desktop WASM compilation
cd apps/desktop && dx build --platform web
```

## MCP Workflow

```
launch_app → poll get_last_build_status → connect_cdp → take_screenshot / navigate
```

All `launch_app` and `rebuild_app` calls are **non-blocking** — poll `get_last_build_status`
every 5-10s until `state != "Running"`.

### MCP Identity — DO NOT CONFUSE

The **poly-electron**, **poly-web**, and **poly-desktop** MCP servers are custom Rust
binaries in this repo (`mcp/*/src/main.rs`). They are **NOT** `chrome-devtools-mcp`,
`chrome-devtools-headless`, or `firefox-devtools-mcp`. Never substitute a generic
browser MCP for a poly MCP — they have different tools (`launch_app`, `rebuild_app`,
`get_last_build_status`, `connect_cdp`, etc.) and manage the full app lifecycle.
If the poly MCPs are not loaded in the current session, say so — do not fall back
to chrome-devtools as a replacement.
