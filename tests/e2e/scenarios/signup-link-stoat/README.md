# signup-link-stoat

E2E scenario: asserts the Stoat "Register" link on `/signup/stoat`.

## What is asserted

- Default (official instance): `href` starts with `https://app.stoat.chat`.
- Self-hosted instance (`https://stoat.mycorp.internal`): `href` starts with the instance root.
- In real-network mode: clicking opens a reachable page.

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-stoat
```

## Backend URL source

`clients/stoat/src/config.rs` — `OFFICIAL_STOAT_BASE_URL = "https://api.stoat.chat"`;
user-facing app at `https://app.stoat.chat`. Stoat is the Revolt rebrand.
Verified 2026-04-30.
