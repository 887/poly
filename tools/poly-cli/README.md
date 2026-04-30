# poly-cli

Dynamic MCP-to-CLI translator for Poly chat backends.

Connects to a running `poly-chat-mcp` server, discovers available tools, and
exposes them as CLI subcommands via `poly-cli call <tool> --key value …`.  No
per-tool configuration is required — the tool list is discovered at runtime.

## Installation

```bash
cargo install --path tools/poly-cli
```

Or build and run directly:

```bash
cargo run -p poly-cli -- call meta_persona_list
```

## Quick start

```bash
# Check connectivity
poly-cli health

# List all available MCP tools
poly-cli tools

# Call any tool by name
poly-cli call meta_persona_list

# Show schema for a specific tool
poly-cli call meta_persona_invoke --help
```

## Usage

```
poly-cli [--url <URL>] [--format json|pretty] <subcommand>

Subcommands:
  health          Check MCP server reachability
  tools           List available tools and short descriptions
  call <tool>     Call a tool with --key value arguments
```

### `call` argument syntax

Arguments are passed as `--key value` pairs.  Values are automatically coerced:

- `true` / `false` → JSON boolean
- Integer strings → JSON number
- Strings starting with `{` or `[` → parsed as JSON
- Everything else → JSON string

```bash
poly-cli call meta_persona_create \
  --slug broker-bob \
  --name "Broker Bob" \
  --enabled true

poly-cli call meta_persona_set_sources \
  --slug broker-bob \
  --sources '[{"account_id":"discord:123","selector_kind":"server","selector_value":"guild-id","include":true}]'
```

## Persona recipes

See `docs/personas-cli.md` for the canonical recipe book covering all persona
lifecycle operations:

- Create, list, get, update, delete
- Set sources and tool whitelist
- Invoke (normal and dry-run)
- Pin facts, pause/resume, audit log

## Design note — no typed subcommands

`poly-cli` intentionally has no typed `persona create`, `persona list`, etc.
subcommands.  The dynamic `call <tool>` bridge auto-derives every MCP tool the
server advertises; typed subcommands would duplicate this mapping and drift
behind the MCP schema.  See `docs/personas-cli.md` for the full rationale.
