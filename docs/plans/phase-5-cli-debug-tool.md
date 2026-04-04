# Phase 5: poly-cli -- Backend Debugging & Automation Tool

> Created: 2026-04-04
> Status: Plan

---

## Problem

Debugging backend integrations (Stoat, Matrix, Discord, Teams, Poly Server)
currently requires running the full Dioxus UI and interacting via screenshots
through the MCP devtools servers. This creates two pain points:

1. **AI agents** must take screenshots and parse DOM snapshots just to verify
   that a backend returned the right data -- slow, brittle, expensive.
2. **Developers** cannot quickly test a backend client against a test server
   (or real server) without launching the full app.

The `docker` CLI analogy: `docker ps`, `docker stop`, `docker logs` talk to
the Docker daemon over a socket. `poly-cli` should talk to running test
servers (direct HTTP) and to MCP servers (JSON-RPC over stdio/socket) to
inspect and control the full Poly stack without touching the UI.

---

## Existing Architecture (what we have today)

### Client Backend Trait (`clients/client/src/lib.rs`)

All backends implement `ClientBackend`:

```
authenticate, logout, is_authenticated
get_servers, get_server, get_channels, get_channel
send_message, get_messages, search_messages
get_user, get_friends, get_channel_members
get_groups, get_dm_channels
get_notifications, respond_to_friend_request
get_voice_participants, get_presence, set_presence
create_server, create_channel
event_stream
```

Six backend types: `Stoat`, `Matrix`, `Discord`, `Teams`, `Demo`, `Poly`.

### Test Servers (`servers/test-*/`)

Mock HTTP servers implementing each backend's API subset:

| Server | Default Port | Routes |
|--------|-------------|--------|
| `poly-test-matrix` | 9100 | Matrix Client-Server API subset |
| `poly-test-stoat` | 9101 | Revolt REST API subset |
| `poly-test-discord` | 9102 | Discord REST API subset |
| `poly-test-teams` | 9103 | Teams Graph API subset |
| `poly-test-poly` | 9104 | Poly Server REST + WS |

All share lifecycle endpoints from `poly-test-common`:
- `POST /seed` -- populate demo data (idempotent)
- `POST /reset` -- wipe all data
- `POST /reseed` -- reset + seed in one call
- `GET /health` -- readiness probe

Spawned individually or via `poly-test-runner` (ports 9100-9104).

### MCP Devtools Servers (`mcp/*/`)

Three MCP servers manage the app lifecycle over JSON-RPC stdio:
- `poly-web-devtools-mcp` (Chrome CDP, port 3000)
- `poly-desktop-devtools-mcp` (Wry HTTP eval, port 3002)
- `poly-electron-devtools-mcp` (Electron CDP, port 3001)

Tools: `launch_app`, `kill_app`, `rebuild_app`, `connect_cdp`,
`get_last_build_status`, `take_screenshot`, `take_snapshot`,
`evaluate_script`, `click`, `fill`, `navigate_page`, etc.

The web MCP already has an embryonic CLI mode (`--cli <cmd>`) that
dispatches to `dispatch_web_cli()` -- supporting `status`, `launch`,
`kill`, `rebuild`, `snapshot`, `eval`, `click`, `fill`, `navigate`.

### Memory MCP (`mcp/memory-mcp/`)

Dual-mode (MCP + CLI) server for task/knowledge persistence. Good
architectural precedent: same binary, `--cli` flag switches from
JSON-RPC stdin loop to direct command dispatch.

---

## Architecture: poly-cli

### Design Principles

1. **Thin client, fat backends.** poly-cli is a lightweight dispatcher.
   It creates `ClientBackend` instances (native feature) and calls their
   methods directly. No new server process needed for backend operations.

2. **Two communication modes:**
   - **Direct mode:** poly-cli instantiates a `ClientBackend` (e.g.
     `StoatClient`) and calls methods against a test server or real
     server URL. No MCP server needed.
   - **MCP mode:** poly-cli sends JSON-RPC tool calls to a running MCP
     server (via stdio pipe or future TCP/Unix socket transport) for app
     lifecycle operations (launch, kill, screenshot, snapshot).

3. **Structured output.** JSON by default (for AI agents), with `--pretty`
   and `--table` flags for humans. Errors go to stderr.

4. **Composable.** Each command does one thing. AI agents chain commands.
   Exit code 0 = success, non-zero = failure.

### Crate Structure

```
tools/
  poly-cli/
    Cargo.toml          # binary crate
    src/
      main.rs           # arg parsing, dispatch
      backends.rs       # ClientBackend instantiation per backend type
      commands/
        mod.rs
        stoat.rs        # stoat subcommands
        matrix.rs       # matrix subcommands
        discord.rs      # discord subcommands
        teams.rs        # teams subcommands
        poly.rs         # poly-server subcommands
        test.rs         # test server lifecycle (seed/reset/reseed/health)
        mcp.rs          # MCP server interaction (launch/kill/status/eval)
      output.rs         # JSON/table/pretty formatting
```

### Dependencies

```toml
[dependencies]
poly-client       = { path = "../../clients/client" }
poly-stoat        = { path = "../../clients/stoat", features = ["native"] }
poly-matrix       = { path = "../../clients/matrix", features = ["native"] }
poly-discord      = { path = "../../clients/discord", features = ["native"] }
poly-teams        = { path = "../../clients/teams", features = ["native"] }
poly-server-client = { path = "../../clients/server-client", features = ["native"] }
clap              = { version = "4", features = ["derive"] }
tokio             = { version = "1", features = ["full"] }
serde_json        = "1"
reqwest           = { version = "0.12", features = ["json"] }
anyhow            = "1"
tracing           = "0.1"
tracing-subscriber = "0.3"
```

---

## Command Reference

### Global Flags

```
poly-cli [--url <BASE_URL>] [--format json|table|pretty] [--verbose] <COMMAND>
```

- `--url` overrides the backend server URL (default: test server ports)
- `--format` controls output (default: `json` when stdout is not a TTY,
  `pretty` when it is)
- `--verbose` enables debug tracing to stderr

### Backend Commands (Direct Mode)

These instantiate a `ClientBackend` and call its methods against the
target server.

#### Authentication

```bash
# Stoat: email/password login against a Stoat/Revolt server
poly-cli stoat login --url http://localhost:9101 --email stoat@example.com --password secret
# Stores session token in ~/.poly-cli/sessions.json (or --token flag for one-shot)

# Matrix: username/password
poly-cli matrix login --url http://localhost:9100 --user @owl:localhost --password hoot

# Poly Server: Ed25519 challenge-response
poly-cli poly login --url http://localhost:9104 --key ~/.poly/keys/ed25519.key

# Logout
poly-cli stoat logout
```

Session tokens are cached per (backend, url) pair in
`~/.poly-cli/sessions.json`. Commands that require auth read from this
cache or accept `--token <TOKEN>` directly.

#### Servers & Channels

```bash
# List servers the authenticated user belongs to
poly-cli stoat servers
poly-cli matrix servers

# List channels in a server
poly-cli stoat channels --server <SERVER_ID>
poly-cli stoat channels --server "Woodland Café"  # name lookup

# Get channel details
poly-cli stoat channel <CHANNEL_ID>
```

#### Messages

```bash
# Get recent messages (default: 50)
poly-cli stoat messages <CHANNEL_ID> [--limit 25] [--before <MSG_ID>]

# Shorthand with server + channel name
poly-cli stoat messages --server "Woodland Café" --channel general

# Send a message
poly-cli stoat send <CHANNEL_ID> "Hello from CLI"
poly-cli stoat send --server "Woodland Café" --channel general "Hello from CLI"

# Search messages
poly-cli stoat search --query "meeting notes" --server <SERVER_ID>
```

#### Users & Presence

```bash
# Get current user info
poly-cli stoat me

# Get user by ID
poly-cli stoat user <USER_ID>

# List friends
poly-cli stoat friends

# Get/set presence
poly-cli stoat presence
poly-cli stoat presence --set online
```

#### DMs & Groups

```bash
# List DM channels
poly-cli stoat dms

# List group chats
poly-cli stoat groups

# Send DM
poly-cli stoat dm <USER_ID> "Hey, quick question"
```

### Test Server Commands

These talk directly to test server HTTP endpoints (no ClientBackend needed).

```bash
# Health check all test servers
poly-cli test health
# Output: { "matrix": "ok", "stoat": "ok", "discord": "ok", "teams": "ok", "poly": "ok" }

# Health check one
poly-cli test health stoat

# Seed/reset/reseed
poly-cli test seed stoat              # POST http://localhost:9101/seed
poly-cli test reset stoat             # POST http://localhost:9101/reset
poly-cli test reseed stoat            # POST http://localhost:9101/reseed
poly-cli test reseed --all            # reseed all 5 test servers

# Start test runner (wraps poly-test-runner)
poly-cli test start [--seed]          # spawns all test servers
poly-cli test stop                    # kills test runner + children
```

### MCP Commands

These send JSON-RPC tool calls to MCP servers for app lifecycle control.

```bash
# Connect to a running MCP server via stdio pipe
poly-cli mcp web launch               # equivalent to MCP launch_app tool
poly-cli mcp web status               # get_last_build_status
poly-cli mcp web kill                  # kill_app
poly-cli mcp web rebuild              # rebuild_app
poly-cli mcp web snapshot             # take_snapshot (returns text DOM tree)
poly-cli mcp web eval "document.title" # evaluate_script

# Desktop/Electron variants
poly-cli mcp desktop launch
poly-cli mcp electron status
```

Implementation: these exec the MCP binary with `--cli <args>`, inheriting
the pattern already established in `poly-web-devtools-mcp`. No separate
socket server needed in phase 1.

### Cross-Backend Commands

```bash
# List all active sessions across backends
poly-cli sessions

# Unified message stream (for AI agent monitoring)
poly-cli watch --backends stoat,matrix
# Streams events as newline-delimited JSON to stdout:
# {"type":"MessageReceived","backend":"stoat","channel_id":"...","message":{...}}
# {"type":"TypingStarted","backend":"matrix","channel_id":"...","user_id":"..."}
```

---

## AI Agent Integration

### Why This Matters

Currently, an AI agent debugging a Stoat integration must:
1. Call `launch_app` via MCP
2. Poll `get_last_build_status` until build succeeds
3. Call `connect_cdp`
4. Call `take_snapshot` to read the DOM
5. Parse the text tree to find data
6. Call `evaluate_script` to read WASM state

With poly-cli, the same agent can:
1. `poly-cli test reseed stoat`
2. `poly-cli stoat login --url http://localhost:9101 --email stoat@test --password test`
3. `poly-cli stoat servers` -- direct JSON, no parsing
4. `poly-cli stoat messages --server "Woodland Café" --channel general`
5. `poly-cli stoat send --server "Woodland Café" --channel general "test msg"`
6. `poly-cli stoat messages --server "Woodland Café" --channel general --limit 1`

All JSON output, no screenshots, no DOM parsing, no build step.

### MCP Tool Wrapping

poly-cli can itself be exposed as an MCP tool server, so AI agents using
Claude Code / Copilot can call backend commands as MCP tools:

```json
{
  "name": "poly_cli",
  "description": "Execute a poly-cli command",
  "inputSchema": {
    "type": "object",
    "properties": {
      "command": { "type": "string", "description": "Full poly-cli command" }
    },
    "required": ["command"]
  }
}
```

This is a future enhancement (Phase 5.3) -- initially, agents use `poly-cli`
via shell execution.

---

## Connection to Real Backends

poly-cli is not limited to test servers. The `--url` flag points to any
compatible server:

```bash
# Debug against real Stoat instance
poly-cli stoat login --url https://stoat.chat --email user@example.com --password secret
poly-cli stoat servers --url https://stoat.chat

# Debug against real Revolt instance
poly-cli stoat login --url https://api.revolt.chat --email user@example.com --password secret

# Debug against self-hosted Poly Server
poly-cli poly login --url https://my-poly.example.com --key ~/.poly/keys/ed25519.key
poly-cli poly servers --url https://my-poly.example.com
```

This lets developers reproduce issues against real data without the UI.

---

## Implementation Plan

### Phase 5.1: Foundation (1-2 days)

**Goal:** Basic `poly-cli` binary with Stoat backend working against
test-stoat.

1. Create `tools/poly-cli/` crate with clap-based arg parsing
2. Implement `backends.rs` -- factory that creates a `Box<dyn ClientBackend>`
   given a `BackendType` + URL + credentials
3. Implement `commands/stoat.rs`:
   - `login`, `logout`, `me`
   - `servers`, `channels`, `messages`, `send`
4. Implement `commands/test.rs`:
   - `health`, `seed`, `reset`, `reseed` (direct HTTP POSTs)
5. Implement `output.rs` -- JSON formatter with `--pretty` and `--table`
6. Integration test: `test reseed stoat && stoat login && stoat servers`

**Key decision:** Session storage format. Use a simple JSON file at
`~/.poly-cli/sessions.json`:
```json
{
  "stoat:http://localhost:9101": {
    "token": "...",
    "user_id": "...",
    "expires": "2026-04-05T..."
  }
}
```

### Phase 5.2: All Backends + Test Runner (2-3 days)

**Goal:** Full backend coverage and test lifecycle management.

1. Implement `commands/matrix.rs`, `commands/discord.rs`,
   `commands/teams.rs`, `commands/poly.rs`
   - Each follows the same pattern: instantiate backend, call ClientBackend
     methods, format output
2. Implement `commands/test.rs` enhancements:
   - `test start` / `test stop` (spawn/kill poly-test-runner)
   - `test start stoat` (spawn individual test server)
3. Add `--all` flag for batch operations across backends
4. Add `sessions` command to list/manage cached sessions
5. Integration tests against all 5 test servers

### Phase 5.3: MCP Integration (1-2 days)

**Goal:** Control app lifecycle from CLI.

1. Implement `commands/mcp.rs`:
   - Exec MCP binary with `--cli` flag (reuse existing dispatch_web_cli pattern)
   - Commands: `launch`, `kill`, `rebuild`, `status`, `snapshot`, `eval`
2. Extend desktop and electron MCPs with `--cli` support (if not already present)
   using the same pattern as web MCP's `dispatch_web_cli`
3. Add `poly-cli mcp web`, `poly-cli mcp desktop`, `poly-cli mcp electron`
   subcommand routing

### Phase 5.4: Event Streaming (1-2 days)

**Goal:** Real-time event monitoring for AI agents.

1. Implement `watch` command that subscribes to backend event streams
2. Output newline-delimited JSON (NDJSON) to stdout
3. Support filtering: `--backends`, `--events`, `--channels`
4. Support WebSocket connections to test servers that expose them
   (Stoat Bonfire, Matrix /sync, Poly WS)

### Phase 5.5: MCP Tool Server Mode (1 day)

**Goal:** Expose poly-cli as an MCP server for AI agent consumption.

1. Add `poly-cli mcp-serve` mode that reads JSON-RPC from stdin
2. Map each CLI command to an MCP tool definition
3. Register in VS Code / Claude Code MCP config
4. AI agents can call `poly_stoat_servers`, `poly_stoat_messages`, etc.
   as native MCP tools -- no shell exec needed

---

## File Layout (Final)

```
tools/
  poly-cli/
    Cargo.toml
    src/
      main.rs             # Entry point, clap dispatch
      backends.rs         # BackendType -> Box<dyn ClientBackend> factory
      session.rs          # Token cache read/write
      output.rs           # JSON/table/pretty formatters
      commands/
        mod.rs            # Re-exports
        stoat.rs          # stoat login/servers/channels/messages/send/...
        matrix.rs         # matrix login/servers/channels/messages/...
        discord.rs        # discord login/servers/channels/messages/...
        teams.rs          # teams login/servers/channels/messages/...
        poly.rs           # poly login/servers/channels/messages/...
        test.rs           # test health/seed/reset/reseed/start/stop
        mcp.rs            # mcp web/desktop/electron launch/kill/status/...
        watch.rs          # Real-time event stream
        sessions.rs       # Session management
```

---

## Testing Strategy

### Unit Tests

- Command parsing (clap derive tests)
- Output formatting (JSON, table, pretty)
- Session cache serialization

### Integration Tests

Require test servers running (via `poly-test-runner --seed`):

```bash
# Full workflow test per backend
poly-cli test reseed --all
poly-cli stoat login --url http://localhost:9101 --email stoat@test --password test
poly-cli stoat servers | jq '.[0].name'
poly-cli stoat channels --server <id> | jq '.[0].name'
poly-cli stoat messages --server <id> --channel <id> --limit 5 | jq 'length'
poly-cli stoat send <channel_id> "integration test message"
poly-cli stoat logout
```

### CI

Add `poly-cli` to the workspace build matrix. Integration tests run in CI
after `poly-test-runner --seed` is healthy.

---

## Non-Goals (Explicitly Out of Scope)

- **GUI / TUI.** poly-cli is a headless command-line tool. No curses, no
  interactive prompts (except `--interactive` login if password omitted).
- **Replacing MCP servers.** poly-cli complements the MCP devtools servers,
  it does not replace them. App lifecycle (build, launch, screenshot) stays
  in the MCP servers.
- **Social Agent features.** The Phase 5 Social Agent (personality, memory,
  outreach) is a separate system. poly-cli may later be used to test it, but
  agent logic does not live here.
- **WASM build.** poly-cli is native-only. It uses `features = ["native"]`
  on all client crates.

---

## Open Questions

1. **Socket transport for MCP?** Currently MCP servers use stdio. For
   poly-cli to talk to an already-running MCP server (not exec a new one),
   we would need a socket transport (TCP or Unix). Alternatively, the
   `--cli` exec approach avoids this entirely at the cost of spawning a
   process per command. Phase 5.1-5.3 use exec; socket transport is a
   future optimization.

2. **Credential security.** Session tokens in `~/.poly-cli/sessions.json`
   are plaintext. For test servers this is fine. For real servers, consider
   OS keychain integration (macOS Keychain, libsecret on Linux) as a
   follow-up.

3. **Which backends are ready?** The Stoat client (`clients/stoat/`) has
   the most complete native implementation. Matrix, Discord, Teams clients
   may have stub implementations. Phase 5.1 focuses on Stoat; other
   backends are added as their client crates mature.
