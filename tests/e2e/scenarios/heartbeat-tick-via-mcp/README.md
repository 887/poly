# Scenario: heartbeat-tick-via-mcp

Sets a persona to `proactivity=drafts-only`, configures a heartbeat schedule via
`meta_persona_set_heartbeat` (minimum 60s per schema), and invokes the persona via
`meta_persona_invoke` (non-dry-run) to exercise the draft-creation path. Asserts
audit rows are queryable via `meta_persona_recent_actions`.

**Heartbeat trigger surface:** `meta_persona_set_heartbeat` (wires the persona into
`HeartbeatRegistry`). The minimum interval is 60 seconds per schema validation in
`tools.rs`.

**Mock mode:** Validates the setup surface and invoke path. Real heartbeat firing
(producing `heartbeat_run` + `draft_create` audit rows) is validated in real-claude
mode (nightly) where a 65s wait is acceptable within the 15-minute CI budget.

**Regression this catches:** If `meta_persona_set_heartbeat` stops persisting the
interval (e.g. a schema migration drops the column), or if `proactivity` changes its
meaning, this scenario catches it. The draft-create audit row assertion (real-claude
mode) catches breaks in the heartbeat→draft path specifically.
