# Memory: Stoat auth reference findings

*Stored: 2026-03-16T21:08:35.085706548+00:00*

---

Reference research for the next Stoat slice (2026-03-16):

Verified from `clients/stoat/api-1.json` and the `stoatchat/javascript-client-api` reference repo:
- Login endpoint: `POST /auth/session/login`
- Login request body schema: `DataLogin`
  - primary branch: `{ email, password, friendly_name? }`
  - MFA branch exists but can be deferred for the first auth slice
- Login success response (`ResponseLogin`) is a tagged union:
  - `result: "Success"` with `_id`, `user_id`, `token`, `name`, `last_seen`, optional `origin`, optional `subscription`
  - there are also MFA / Disabled branches that should map to typed auth failures for now
- Session auth header in the JS reference client is `X-Session-Token`
- Current user profile endpoint: `GET /users/@me`
- Root config endpoint: `GET /` returns `RevoltConfig`, including a `ws` field; this is useful for self-hosted instance websocket discovery and should eventually replace naive `/ws` derivation when available.

Design implication for Poly:
- The next small implementation slice should cover only email/password login + stored token resume + fetch-self -> `poly_client::Session` mapping.
- MFA and onboarding should be documented in the Stoat spec, but can remain explicit auth-failure/not-supported branches in this slice.
- E2E should split into: plugin-level contract tests for current stub behavior, and native transport tests for login/session parsing + request/header behavior using local mock HTTP responses.
