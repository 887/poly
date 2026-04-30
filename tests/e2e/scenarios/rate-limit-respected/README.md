# Scenario: rate-limit-respected

Sets `rate_limit_per_hour=2` on a persona, triggers the heartbeat 5 times
back-to-back, and asserts exactly 2 `draft_create` audit rows + 3 `rate_limited`
audit rows in `persona_audit`. This validates that the rate-limit enforcement in
`heartbeat.rs::run_heartbeat_task` correctly counts recent actions via
`count_persona_audit_since` and produces `rate_limited` audit rows when the limit
is exceeded.

**Mock mode:** Validates the `rate_limit_per_hour` setup surface
(`meta_persona_update` → `meta_persona_get` confirms the value). The 5× heartbeat
back-to-back audit row assertion runs in real-claude mode (nightly) only.

**Real-claude mode:** Triggers 5 rapid invocations; expects `heartbeat_run` + 2
`draft_create` + 3 `rate_limited` rows in `meta_persona_recent_actions`.

**Regression this catches:** If `count_persona_audit_since` returns wrong counts
(e.g. timezone bug, wrong `WHERE` clause), or if the `rate_limited` audit write is
skipped, the first 5 heartbeats all create drafts instead of 2, silently spamming
users with unsolicited messages.
