# signup-link-teams

E2E scenario: asserts the Microsoft Teams "Register" link on `/signup/teams`.

## What is asserted

- `[data-testid="register-link-teams"]` is present and visible.
- `href` starts with `https://signup.live.com/signup` (MSA signup URL).
- In real-network mode: clicking opens a reachable page.

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-teams
```

## Backend URL source

`clients/teams/src/signup.rs` — `https://signup.live.com/signup?lic=1`.
Verified 2026-04-30. If the test fails with a 404, re-verify the current
Microsoft MSA signup URL.
