# Personas — CLI Recipe Book

> Canonical reference for interacting with Poly meta-personalities from the
> command line.  All examples use `poly-cli call <tool> --key value …` — the
> dynamic MCP-to-CLI bridge shipped in `tools/poly-cli/`.
>
> Source of truth for the "no typed subcommands" decision: see
> [No typed subcommands — and why](#no-typed-poly-cli-persona-verb-subcommands--and-why)
> at the bottom of this page.

## Prerequisites

```bash
# Start the poly-chat-mcp server (or rely on your app's fullstack server)
# then confirm the CLI can reach it:
poly-cli health
```

Default server URL is `http://localhost:3010/mcp`.  Override with `--url`:

```bash
poly-cli --url http://localhost:3000/mcp health
```

---

## 1. Create a persona

```bash
poly-cli call meta_persona_create \
  --slug broker-bob \
  --name "Broker Bob" \
  --system_prompt "You are my finance broker. Watch my finance Discord servers and the family-finance Matrix room. Surface deals, flag risks. Speak plainly, no MBA jargon." \
  --avatar_emoji "💹" \
  --enabled true
```

The slug is a URL-safe identifier (lowercase, hyphens only).  It cannot be
changed after creation.

---

## 2. List all personas

```bash
poly-cli call meta_persona_list
```

---

## 3. Get a single persona (full detail)

```bash
poly-cli call meta_persona_get --slug broker-bob
```

Returns the full persona record including sources, tool whitelist, and recent
audit rows.

---

## 4. Set sources

Sources bind a persona to specific accounts and chats.  The `sources` argument
is a JSON array; `include: true` allows a source, `include: false` denies it
(deny-wins — a denied row overrides any allow above it in scope).

```bash
poly-cli call meta_persona_set_sources \
  --slug broker-bob \
  --sources '[
    {"account_id":"discord:12345","selector_kind":"server","selector_value":"guild-id-here","include":true},
    {"account_id":"matrix:@me:example.com","selector_kind":"channel","selector_value":"!roomid:example.com","include":true},
    {"account_id":"discord:12345","selector_kind":"channel","selector_value":"channel-id-to-exclude","include":false}
  ]'
```

`selector_kind` values:
- `"all"` — every chat in the account
- `"server"` — all channels in a Discord guild / Matrix space
- `"channel"` — a single channel or room
- `"dm"` — a direct message thread

`meta_persona_set_sources` is a **full replace** — it atomically swaps the
entire source list.  Fetch the current list with `meta_persona_get`, modify it
locally, and re-submit.

---

## 5. Set tool whitelist

Controls which MCP tools the persona may call when invoked.

```bash
poly-cli call meta_persona_set_tool_whitelist \
  --slug broker-bob \
  --tools '["get_messages","list_servers","list_channels","meta_persona_invoke","memory_store","memory_recall"]'
```

An empty whitelist defaults to all read + memory + draft tools.  Explicitly
include `"send_message"` or `"send_typing"` to grant outbound access.

---

## 6. Invoke a persona

```bash
poly-cli call meta_persona_invoke \
  --slug broker-bob \
  --user_prompt "What deals should I be watching today?"
```

Returns a `bundle_v1` JSON object.  Pipe into `jq` for inspection:

```bash
poly-cli call meta_persona_invoke \
  --slug broker-bob \
  --user_prompt "What deals should I be watching today?" \
  | jq '.chats | length'
```

Optional tuning flags:

| Flag | Default | Meaning |
|---|---|---|
| `--max_messages_per_chat` | `30` | Max messages fetched per chat |
| `--max_chats` | `25` | Max chats included in bundle |
| `--include_summaries` | `true` | Prefer stored summaries over raw messages |

---

## 7. Dry-run invoke (bundle preview, no audit pollution)

When `dry_run=true` the bundle is built identically — same source resolution,
same message fetching — but the implicit `memory_read` audit rows are
suppressed.  The user-initiated `invoke` audit row still fires.

Use this to inspect the bundle shape without polluting audit history.  The
e2e harness (`plan-persona-e2e-multi-agent.md`) uses this flag for bundle
sanity-checks.

```bash
poly-cli call meta_persona_invoke \
  --slug broker-bob \
  --user_prompt "preview: what would you pull in right now?" \
  --dry_run true
```

The returned bundle includes `"dry_run": true` at the top level so consumers
can detect it.

---

## 8. Pin a fact (composed: get then set)

There is no standalone `pin_fact` tool.  Use two calls:

```bash
# Step 1 — find the fact_id
poly-cli call meta_persona_get_memory --slug broker-bob

# Step 2 — update the fact with pinned=true (fact_id from step 1)
poly-cli call meta_persona_set_memory \
  --slug broker-bob \
  --fact_text "Client prefers ETFs over individual stocks" \
  --pinned true \
  --category preference
```

To **replace** an existing fact, first forget it then re-add it pinned:

```bash
poly-cli call meta_persona_forget_memory --slug broker-bob --fact_id 42
poly-cli call meta_persona_set_memory \
  --slug broker-bob \
  --fact_text "Updated: client now prefers dividend ETFs" \
  --pinned true \
  --category preference
```

---

## 9. Pause a persona

```bash
poly-cli call meta_persona_update --slug broker-bob --enabled false
```

Resume:

```bash
poly-cli call meta_persona_update --slug broker-bob --enabled true
```

---

## 10. Delete a persona

```bash
poly-cli call meta_persona_delete --slug broker-bob
```

This is permanent — all associated sources, memory facts, and audit rows are
removed.  There is no soft-delete; confirm before running.

---

## 11. List recent actions (audit log)

```bash
poly-cli call meta_persona_recent_actions --slug broker-bob --limit 20
```

Returns audit rows newest-first.  `action` values: `invoke`, `memory_read`,
`source_update`, `tool_whitelist_update`, `create`, `update`, `delete`.

---

## 12. Set heartbeat (Phase F, when available)

Once Phase F ships, use:

```bash
# Set to every 4 hours
poly-cli call meta_persona_set_heartbeat --slug broker-bob --interval_secs 14400

# Disable heartbeat
poly-cli call meta_persona_set_heartbeat --slug broker-bob --interval_secs 0
```

---

## 13. Audit recipes

The `meta_persona_audit_query` tool provides server-side filtering over the
persona audit log.  All arguments are optional; combine them freely.

### What did broker-bob do today?

```bash
poly-cli call meta_persona_audit_query \
  --slug broker-bob \
  --since 2026-04-30T00:00:00Z
```

### All denied outbound attempts (any persona)

```bash
poly-cli call meta_persona_audit_query \
  --result denied \
  --action outbound_send
```

### Rate-limit-exceeded rows in the last hour

```bash
poly-cli call meta_persona_audit_query \
  --action rate_limited \
  --since $(date -u -d '1 hour ago' --iso-8601=seconds)
```

_(On macOS use `date -u -v-1H +%Y-%m-%dT%H:%M:%SZ` instead.)_

### Export full audit before deleting a persona

```bash
poly-cli call meta_persona_audit_export --slug broker-bob > audit.jsonl
poly-cli call meta_persona_delete --slug broker-bob
```

`meta_persona_audit_export` returns the complete history as JSONL (one JSON
object per line, oldest-first).  Redirect to a file before deleting — the
delete permanently removes all associated audit rows.

### Diff today's vs yesterday's action counts

```bash
TODAY=$(date -u +%Y-%m-%dT00:00:00Z)
YEST=$(date -u -d 'yesterday' +%Y-%m-%dT00:00:00Z 2>/dev/null \
  || date -u -v-1d +%Y-%m-%dT00:00:00Z)

echo "Today:"
poly-cli --format json call meta_persona_audit_query \
  --slug broker-bob --since "$TODAY" --limit 500 \
  | jq '[.[].action] | group_by(.) | map({action: .[0], count: length})'

echo "Yesterday:"
poly-cli --format json call meta_persona_audit_query \
  --slug broker-bob --since "$YEST" --until "$TODAY" --limit 500 \
  | jq '[.[].action] | group_by(.) | map({action: .[0], count: length})'
```

### Live-tail audit rows with `--watch`

The `--watch <N>` flag re-runs a `call` every N seconds, printing only new
rows (deduplicated by `id`).  Combine with `--since auto` to start from the
current moment and stream deltas:

```bash
poly-cli --watch 5 call meta_persona_audit_query \
  --slug broker-bob \
  --since auto \
  --limit 100
```

`--since auto` tells `poly-cli` to initialise `since` to the current UTC
timestamp and advance it to the latest row's `occurred_at` after each poll.
Exit cleanly with **Ctrl+C**.

---

## See also

- `docs/client-settings.md` — CLI recipe book for `client_settings_*` tools:
  version overrides, mechanism toggles, and the rollback story.
- `docs/signup-link-surface.md` — per-backend "Register here" affordance:
  `SignupMethod` enum, per-backend URL table, browser-opening behaviour across
  Web / Electron / Wry shells, and how to add signup support to a new backend.

---

## No typed `poly-cli persona <verb>` subcommands — and why

**Decision (codified 2026-04-30, Phase J rescope).**

The original Phase J planned to add typed `poly-cli persona create`,
`poly-cli persona list`, etc. subcommands.  This was dropped because:

1. **`tools/poly-cli` already auto-derives every tool.**  The dynamic
   `poly-cli call <tool> --key val …` bridge in
   `tools/poly-cli/src/main.rs` exposes every MCP tool the server advertises
   without any per-tool CLI code.  Adding a `persona` subcommand tree would
   require maintaining a second, static mapping of the same tools — more code
   for no new capability.

2. **The MCP tool surface is the authoritative API.**  Typed subcommands
   inevitably drift behind the MCP schema.  Consumers — the e2e harness,
   automation scripts, future UI — should target the MCP tool names directly
   so they can use `poly-cli call --help` to discover the current schema
   without consulting stale documentation.

3. **Shortcut wrappers add maintenance burden.**  `set_enabled`, `pin_fact`,
   and similar conveniences are one or two `poly-cli call` lines.  A separate
   Rust subcommand for each would need its own argument parsing, error
   handling, and documentation — all of which go stale.

If a future contributor wants to add typed subcommands: reopen the discussion
in the issue tracker and update `docs/plans/plan-meta-personalities.md` Phase J
scope-decision block.  Do not silently re-add them as an "ergonomic" addition —
the tradeoffs above apply to any new typed subcommand, not just persona ones.

**Reference:** `tools/poly-cli/src/main.rs` — `Command::Call` variant;
`docs/plans/plan-meta-personalities.md` — Phase J "Scope decision" block.
