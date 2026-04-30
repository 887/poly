# signup-link-hackernews

E2E scenario: asserts the Hacker News "Register" link on `/signup/hackernews`.

## What is asserted

- `[data-testid="register-link-hackernews"]` is present and visible.
- `href` starts with `https://news.ycombinator.com/login`.
  HN's login page doubles as the create-account page (no separate /signup URL).
- In real-network mode: clicking opens a reachable page.

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-hackernews
```

## Backend URL source

`clients/hackernews/src/lib.rs` — `news.ycombinator.com`. HN uses /login for
both login and account creation. Verified 2026-04-30.
