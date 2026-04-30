# Scenario: mcp-to-ui-live-update

**HEADLINE REGRESSION TEST.** Invokes `meta_persona_create` via `poly-cli` (same
surface as an MCP tool call from Claude). Playwright asserts the new persona row
(`data-testid="persona-row-live-probe-xyz"`) appears in `PersonaListPanel` within 5s
of the MCP call completing, with no full page reload.

**Reactive chain under test:**
```
SQLite INSERT → backend events → poll_events → app_state BatchedSignal
             → PersonaListPanel re-render → DOM update
```

**Regression this catches:** Any break in the reactive subscription from the SQLite
write to the WASM DOM. Historical causes include: dropped signal subscriptions, missing
`use_future` poll loops, `BatchedSignal` mis-migration leaving a stale `Signal::read()`,
or a server-side event not being emitted after a write. If E.3 fails, the reactive
chain is broken — do not merge.
