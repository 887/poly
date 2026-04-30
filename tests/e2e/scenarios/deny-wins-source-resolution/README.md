# Scenario: deny-wins-source-resolution

Persona A is bound to `(account=test-discord, kind=server, value=guild-A, include=true)`
AND `(account=test-discord, kind=channel, value=guild-A/ch-secret, include=false)`. A
message is sent to ch-secret. The persona's `meta_persona_invoke` bundle must NOT
include that message — the channel-level deny overrides the server-level allow.

**Regression this catches:** The deny-wins precedence rule in `persona/context.rs`
`resolve_sources()`. Unit tests cover this path inside the Rust binary; this e2e
scenario catches integration regressions where source resolution is skipped, the
wrong precedence is applied at the SQL layer, or a new MCP surface bypasses the
filtering logic. If this scenario fails, the persona could expose messages from
channels the user explicitly excluded, which is a privacy regression.
