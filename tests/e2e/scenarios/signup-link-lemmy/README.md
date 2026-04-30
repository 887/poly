# signup-link-lemmy

E2E scenario: asserts the Lemmy "Register" link on `/signup/lemmy`.

## What is asserted

- Default instance: `href` starts with `https://lemmy.ml/signup`.
- Custom instance (`https://beehaw.org`): `href` starts with `https://beehaw.org/signup`.
- In real-network mode: clicking opens a reachable page.

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-lemmy
```

## Backend URL source

`clients/lemmy/src/signup.rs` — default `lemmy.ml`; parameterised as
`{server_url}/signup`. Verified 2026-04-30.
