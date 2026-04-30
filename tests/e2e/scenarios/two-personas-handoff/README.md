# Scenario: two-personas-handoff

Regression test for message-based cross-persona communication via a shared channel.

Persona **alpha-sender** sends a message to `test-discord/ch-shared`. Persona
**beta-receiver** (bound to the same channel) is then invoked and must be able to
read that message from the shared channel. Both personas use the same
`POLY_DATA_DIR` SQLite, so alpha's write is immediately visible to beta.

**What this catches:** any regression in the source-binding → context-bundle pipeline
where a message sent to a channel by one persona-driven agent is not visible to a
second persona whose source bindings include that channel. The handoff path is
`send_message → poly-chat-mcp → SQLite → get_messages / meta_persona_invoke bundle`.

**Mock vs real-claude:**
- Mock mode (CI default): `mock-actions.jsonl` drives both agents via `poly-cli`
  directly. The assertion checks that `get_messages` returns without error.
- Real-claude mode (`--mode real-claude`): asserts the literal text "3pm" appears
  in `ch-shared` messages, proving alpha's message was actually stored.
