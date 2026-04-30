# signup-link-github

E2E scenario: asserts the GitHub "Register" link on `/signup/github`.

## What is asserted

- Public GitHub: `href` starts with `https://github.com/signup`.
- GitHub Enterprise (`https://github.mycorp.internal`): `href` is the instance root.
- In real-network mode: clicking opens a reachable page.

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-github
```

## Backend URL source

`clients/github/src/signup.rs` — `github.com` → `https://github.com/signup`;
Enterprise → instance root (SSO landing). Verified 2026-04-30.
