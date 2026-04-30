# Scenario: client-version-override-discord

End-to-end test for the client-version override feature on the Discord backend.

## What this tests

1. The Settings UI (`client-settings-section`) renders the Discord backend card.
2. Toggling the version-override input, filling it with `e2e-test/9.9.9`, and saving
   persists the override via the MCP layer (`client_settings_get_version` returns the
   override value).
3. A subsequent HTTP request to the test-discord mock server carries the overridden
   `User-Agent: e2e-test/9.9.9` header, proving end-to-end propagation.
4. Clearing the override reverts to `source: "default"`.

## How to run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario client-version-override-discord
```

The harness automatically boots:
- All 8 mock backends (`poly-test-runner`, ports 9100-9107)
- `poly-chat-mcp` HTTP server (port 3010)
- `poly-web` via `dx serve --fullstack` (port 3000)

After the stack is healthy the scenario runs, then tears everything down.

## Architecture

This scenario does NOT use `claude -p` or any personas. The test flow is:

1. **Pre-state**: `poly-cli call client_settings_get_version --backend_id=discord` asserts
   no override is active.
2. **UI**: Playwright opens port 3000, navigates to Settings, expands the Discord card,
   toggles the override input on, fills `e2e-test/9.9.9`, and saves.
3. **MCP poll**: `poly-cli` polls `client_settings_get_version` until the override appears
   (cap 30s).
4. **Wire ping**: `curl` fires a raw HTTP request at the test-discord mock server (port
   9102). The mock server's header-inspect ring buffer records every inbound request.
5. **Wire assert**: `GET http://localhost:9102/test/inspect/last-headers` returns the
   ring buffer; the script asserts the most-recent entry's `user-agent` header contains
   `e2e-test/9.9.9`.
6. **Cleanup**: Playwright clicks "clear override"; `poly-cli` confirms `source: "default"`.
   A second curl+inspect asserts the default UA is back.

## Files

| File | Purpose |
|------|---------|
| `scenario.sh` | Bash driver sourced by `persona-multi-agent.sh`; orchestrates Playwright + poly-cli + curl |
| `spec.ts` | Playwright spec — Page Object Model for the client-settings section |
| `personas.jsonl` | Empty (no personas needed) |

## Mock vs real-claude

Not applicable — this scenario makes no `claude -p` calls. The `--mode` flag is ignored.
