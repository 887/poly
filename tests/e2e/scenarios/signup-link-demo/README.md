# signup-link-demo

E2E scenario: asserts the demo backend renders NO "Register" link.

## What is asserted

- No `[data-testid="register-link-demo"]` element exists on `/signup/demo`.
- No `[data-testid^="register-link-"]` element appears inside
  `[data-testid="signup-form-container"]`.

The demo backend returns `SignupMethod::NotSupported`, so `RegisterLink`
renders nothing (`rsx! {}`).

## Run

```bash
bash tests/e2e/persona-multi-agent.sh --scenario signup-link-demo
```

## Backend source

`clients/demo/src/lib.rs` — no `signup.rs` file; default trait impl returns
`NotSupported`. Verified 2026-04-30.
