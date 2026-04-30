# signup-link-matrix

E2E scenario: asserts the Matrix "Register" link on `/signup/matrix`.

## What is asserted

- Default (no server URL): `href` starts with `https://app.element.io/#/register`.
- Custom server URL injected (`https://matrix.example.org`):
  `href` matches `https://matrix.example.org/_matrix/client/v3/register`.
- In real-network mode: clicking opens a reachable page.

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-matrix
```

## Backend URL source

`clients/matrix/src/signup.rs` — default `https://app.element.io/#/register`;
custom server parameterised as `{server_url}/_matrix/client/v3/register`.
Verified 2026-04-30.
