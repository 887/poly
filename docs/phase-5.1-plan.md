# Phase 5.1 Plan — MCP Backend Server + Dynamic CLI + End-to-End Testing

> **Created:** 2026-04-04
> **Updated:** 2026-04-04
> **Status:** In Progress
> **Goal:** Build an MCP server embedded in the Poly app (and startable standalone), plus a thin CLI that dynamically discovers MCP tools and exposes them as subcommands. Test end-to-end that accounts connected via MCP appear in the Poly UI.
> **Depends on:** Phase 4 (test servers), Phase 3.x (client backends)

---

## Core Concept

Phase 5.1 is the **infrastructure layer** that makes all of Poly's chat backends programmable. Two deliverables:

1. **`poly-chat-mcp`** — An MCP server that exposes every `ClientBackend` method as a tool. Runs in two modes:
   - **Embedded in poly-web** (or poly-electron/poly-desktop): mounted as non-UI HTTP routes under the same server. Shares the UI's `ClientBackend` pool — when the MCP logs into a Matrix server, the account appears in the UI sidebar immediately.
   - **Standalone**: starts its own HTTP server on a configurable port, manages its own backend pool. Used for headless testing.

2. **`poly-cli`** — A thin MCP client (no backend crate dependencies). Connects to a running MCP server via HTTP, calls `tools/list` to discover available tools, and dynamically exposes them as CLI subcommands. No recompilation needed when MCP tools change.

### Architecture

```
┌──────────────────────────────────────────────────┐
│  Poly App Process (e.g. poly-web on :3000)       │
│                                                  │
│  ClientManager (Signal<ClientManager>)           │
│  ├── Matrix account (Owl) ──┐                    │
│  ├── Stoat account ─────────┤                    │
│  ├── Discord account ───────┼─► Shared Pool      │
│  ├── Teams account ─────────┤                    │
│  └── Poly account ──────────┘                    │
│           │                    │                  │
│           ▼                    ▼                  │
│     Poly UI (Dioxus)    poly-chat-mcp            │
│     - sidebar            - HTTP JSON-RPC routes  │
│     - chat view          - /mcp/tools/list       │
│     - settings           - /mcp/tools/call       │
│                                                  │
│  poly-cli (separate process)                     │
│  ├── connects to http://localhost:3000/mcp       │
│  ├── discovers tools dynamically                 │
│  └── translates MCP tools → CLI subcommands      │
│                                                  │
│  --url override: target electron (:3001),        │
│    desktop (:3002), or standalone MCP (:3010)    │
└──────────────────────────────────────────────────┘
```

### Why This Design

- **No mcp.json needed.** The MCP runs as HTTP routes inside the app. poly-cli talks to it via HTTP. Later, for Claude Code integration, we add it to mcp.json — but not now.
- **Dynamic tool discovery.** poly-cli never compiles against backend crates. It discovers tools at runtime from the MCP. When the MCP adds a new tool, poly-cli gets it automatically.
- **One MCP, multiple consumers.** The same MCP serves the CLI, future Claude Code integration, and eventually the social agent. All share the same backend pool.
- **URL-based targeting.** `--url http://localhost:3000` targets poly-web. `--url http://localhost:3001` targets poly-electron. `--url http://localhost:3010` targets standalone MCP.

---

## 5.1.0 Architecture Decisions

- [x] **5.1.0.1** `poly-chat-mcp` is an HTTP server exposing JSON-RPC at `/mcp` (or `/api/mcp`). Standalone mode listens on its own port (default 3010). Embedded mode adds routes to the existing app server.
- [x] **5.1.0.2** `poly-cli` is a thin HTTP client (`tools/poly-cli/`). Only depends on `reqwest`, `clap`, `serde_json`. No backend crate dependencies.
- [x] **5.1.0.3** Dynamic tool discovery: poly-cli calls `tools/list` on startup, builds subcommands from the MCP's tool schemas. Tool arguments come from the `inputSchema` property definitions.
- [x] **5.1.0.4** Default URL: `http://localhost:3010` (standalone MCP). Override with `--url`.
- [x] **5.1.0.5** Session state lives in the MCP server (in-memory), not in the CLI. The CLI is stateless.

---

## 5.1.1 poly-chat-mcp — HTTP MCP Server

### Modes

1. **Standalone** (`cargo run --bin poly-chat-mcp`): starts HTTP server on port 3010. Manages its own `ClientBackend` pool. Good for testing without the UI.
2. **Embedded** (future): poly-web mounts the MCP routes under `/mcp`. Shares the app's `ClientManager`. Accounts connected via MCP appear in the UI immediately.

### HTTP Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/mcp` | JSON-RPC 2.0 dispatch (initialize, tools/list, tools/call) |
| GET | `/health` | Server health check |

### MCP Tool Surface

#### Account Management
| Tool | Description |
|------|-------------|
| `login` | Authenticate: `{backend, url, username, password}` → session |
| `logout` | Disconnect: `{backend, account_id}` |
| `list_accounts` | All connected accounts |

#### Read Tools
| Tool | Description |
|------|-------------|
| `list_servers` | Servers/guilds/spaces for an account |
| `list_channels` | Channels in a server |
| `get_messages` | Messages from a channel (paginated) |
| `list_dms` | DM channels |
| `list_friends` | Friend list |
| `get_user` | User profile by ID |

#### Write Tools
| Tool | Description |
|------|-------------|
| `send_message` | Send message to a channel |

#### Test Server Tools
| Tool | Description |
|------|-------------|
| `test_health` | Check test server health |
| `test_reseed` | Reseed test server demo data |

### Checklist

- [x] **5.1.1.1** Create `mcp/chat-mcp/` crate with HTTP server (axum)
- [x] **5.1.1.2** Implement JSON-RPC dispatch: `initialize`, `tools/list`, `tools/call`
- [x] **5.1.1.3** Implement backend pool: `BackendPool` manages authenticated `ClientBackend` instances
- [x] **5.1.1.4** Implement all account/read/write/test tools
- [ ] **5.1.1.5** Add HTTP mode: listen on port (default 3010), serve POST `/mcp`
- [ ] **5.1.1.6** Keep stdio mode as fallback (for future mcp.json integration)
- [ ] **5.1.1.7** Integration test: HTTP request → login → list_servers → send_message

---

## 5.1.2 poly-cli — Dynamic MCP-to-CLI Translator

### How It Works

1. On startup, poly-cli sends `tools/list` to the MCP server
2. It builds a CLI subcommand for each tool, using the tool's `inputSchema` to define arguments
3. When the user runs `poly-cli <tool_name> --arg1 val1 --arg2 val2`, it:
   - Converts CLI args to a JSON object
   - Sends `tools/call` with `{name: tool_name, arguments: {...}}` to the MCP
   - Prints the result

### Usage

```bash
# Default: connects to http://localhost:3010
poly-cli tools                    # list available tools
poly-cli call login --backend matrix --url http://localhost:9100 --username owl --password testpass123
poly-cli call list_servers --backend matrix
poly-cli call send_message --backend matrix --channel_id '!general1:localhost' --text "Hello!"
poly-cli call list_accounts
poly-cli call test_reseed --backend all

# Target poly-web instead of standalone MCP
poly-cli --url http://localhost:3000/mcp call list_accounts

# Target electron
poly-cli --url http://localhost:3001/mcp call list_accounts
```

### Crate Structure

```
tools/
  poly-cli/
    Cargo.toml          # minimal: reqwest, clap, serde_json, anyhow, tokio
    src/
      main.rs           # arg parsing, dispatch
      mcp_client.rs     # HTTP JSON-RPC client
      dynamic.rs        # tool discovery + dynamic arg building
```

### Checklist

- [ ] **5.1.2.1** Rewrite `Cargo.toml` — remove all backend crate deps, keep only reqwest/clap/serde_json/tokio/anyhow
- [ ] **5.1.2.2** Implement `mcp_client.rs` — send JSON-RPC over HTTP to MCP server
- [ ] **5.1.2.3** Implement `dynamic.rs` — call `tools/list`, parse tool schemas, build arg descriptions
- [ ] **5.1.2.4** Implement `main.rs`:
  - `poly-cli tools` — list tools with descriptions
  - `poly-cli call <tool_name> [--key value ...]` — call any MCP tool
  - `--url` flag (default: `http://localhost:3010`)
  - `--format json|pretty` output control
- [ ] **5.1.2.5** Pretty-print tool results (strip MCP wrapper, show just the text content)

---

## 5.1.3 End-to-End Testing with CLI + MCP

### Flow

```bash
# 1. Start test servers
./target/debug/poly-test-matrix --port 9100 --seed &
./target/debug/poly-test-stoat --port 9101 --seed &

# 2. Start standalone MCP server
./target/debug/poly-chat-mcp --port 3010 &

# 3. Use CLI to test everything
poly-cli call test_health
poly-cli call test_reseed --backend all
poly-cli call login --backend matrix --url http://localhost:9100 --username owl --password testpass123
poly-cli call login --backend stoat --url http://localhost:9101 --username stoat --password testpass123
poly-cli call list_accounts
poly-cli call list_servers --backend matrix
poly-cli call list_channels --backend matrix --server_id '!space1:localhost'
poly-cli call send_message --backend matrix --channel_id '!general1:localhost' --text "Hello from CLI!"
poly-cli call list_dms --backend stoat
poly-cli call send_message --backend stoat --channel_id CHDM001 --text "Stoat checking in!"
```

### Per-Backend Test Matrix

| Backend | Port | Animal 1 | Animal 2 | Status |
|---------|------|----------|----------|--------|
| Matrix | 9100 | Owl | Axolotl | ✅ Fully implemented |
| Stoat | 9101 | Stoat | Raccoon | ✅ Fully implemented |
| Discord | 9102 | Koala | Kangaroo | ⬜ Stubbed (Phase 3.3) |
| Teams | 9103 | Sheep | Walrus | ⬜ Stubbed (Phase 3.4) |
| Poly | 9104 | Cockatoo | Parrot | ⬜ Seed not implemented |

### Checklist

- [ ] **5.1.3.1** Test Matrix: login → list_servers → list_channels → get_messages → send_message
- [ ] **5.1.3.2** Test Stoat: login → list_dms → send_message → get_messages
- [ ] **5.1.3.3** Test cross-backend: login both, list_accounts shows both
- [ ] **5.1.3.4** Test tool discovery: `poly-cli tools` shows all available tools

---

## 5.1.4 Visual UI Integration Testing

Once the MCP is embedded in poly-web (sharing the `ClientManager`):

1. Launch poly-web via `poly-web` MCP (`launch_app`)
2. Start test servers
3. Use poly-cli (targeting poly-web's URL) to login to backends
4. Take screenshot → verify accounts appear in sidebar
5. Send message via CLI → verify it appears in UI

This requires the MCP to be embedded in poly-web (Phase 5.1.1 embedded mode). Deferred until standalone mode is solid.

### Checklist

- [ ] **5.1.4.1** Embed MCP routes in poly-web's axum server
- [ ] **5.1.4.2** Login via CLI → verify account appears in UI sidebar
- [ ] **5.1.4.3** Send message via CLI → verify it appears in UI chat view
- [ ] **5.1.4.4** Screenshot verification via chrome-devtools MCP

---

## Completion Criteria

- [ ] `poly-chat-mcp` starts as standalone HTTP server on port 3010
- [ ] All MCP tools work: login, list_servers, get_messages, send_message, test_reseed, etc.
- [ ] `poly-cli` connects to MCP, discovers tools dynamically
- [ ] `poly-cli tools` lists all available tools
- [ ] `poly-cli call <tool> --args` calls any tool and prints result
- [ ] `--url` flag targets different app instances (web/electron/desktop/standalone)
- [ ] Full E2E test: start MCP → login to Matrix + Stoat → list/send/verify
