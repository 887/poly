# signup-link-discord

E2E scenario: asserts the Discord "Register" link on `/signup/discord`.

## What is asserted

- `[data-testid="register-link-discord"]` is present and visible.
- `href` matches `https://discord.com/register` (exact prefix).
- In real-network mode (`POLY_SIGNUP_E2E_REAL=1`): clicking opens a reachable page.

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-discord
```

## Backend URL source

`clients/discord/src/signup.rs` — hardcoded `https://discord.com/register`.
Verified 2026-04-30.
