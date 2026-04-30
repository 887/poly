# Scenario: two-personas-shared-channel

Two personas — broker-bob (finance) and greens-greg (environment) — both bind to
the same test-discord channel `ch-shared`. Broker-bob's agent invokes its persona
and posts a market update; greens-greg's agent reads the same channel via its own
persona invocation and asserts the shared-channel context is visible.

Playwright asserts both persona rows appear in `PersonaListPanel` within 5s of
the agents completing, and that no full page reload occurred during the scenario.

**Regression this catches:** If the `PersonaListPanel` reactive subscription to
`meta_persona_list` breaks — either the signal subscription is dropped or the
persona row rendering stops — the `data-testid="persona-row-*"` locators will
not appear and Playwright fails immediately. This is the simplest two-agent
shared-channel interaction and serves as a baseline for E.3 (the headline test).
