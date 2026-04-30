# signup-link-forgejo

E2E scenario: asserts the Forgejo "Register" link on `/signup/forgejo`.

## What is asserted

- Default instance: `href` starts with `https://codeberg.org/user/sign_up`.
- Custom instance (`https://git.mycorp.internal`): `href` matches `{instance}/user/sign_up`.
- In real-network mode: clicking opens a reachable page.

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-forgejo
```

## Backend URL source

`clients/forgejo/src/signup.rs` — default `codeberg.org`; parameterised as
`{server_url}/user/sign_up`. Verified 2026-04-30.
