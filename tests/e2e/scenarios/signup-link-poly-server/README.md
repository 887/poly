# signup-link-poly-server

E2E scenario: asserts the poly-server "Register" link uses the in-app flow.

## What is asserted

- On `/signup` (picker page): `[data-testid="register-link-poly-server"]`
  is present. Clicking it navigates to `/signup/poly`.
- `[data-testid="signup-form-container"]` is visible on `/signup/poly`.
- On `/signup/poly` itself: the link is hidden (Phase D hides it when the
  user is already on the target route).

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-poly-server
```

## Backend routing source

`clients/server-client/src/signup.rs` — `SignupEntry { slug: "poly" }` mounts
the Ed25519 key-first flow at `/signup/poly`. Phase D wires `InApp("/signup/poly")`
as the signup method. Verified 2026-04-30.
