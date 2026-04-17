# TEST_HARNESS — Poly Project

Run this file as a subagent task. Execute every step in order and report
pass/fail for each. Stop on first hard failure (compilation error, panic).
Warnings are acceptable unless they are new compared to the previous run.

---

## 1. Rust type-check (all crates)

```bash
cargo check --workspace 2>&1
```

Expected: `Finished` with zero errors.

---

## 2. Clippy (poly-core + all native plugins + chat-mcp)

```bash
cargo clippy \
  -p poly-core \
  -p poly-client \
  -p poly-demo \
  -p poly-stoat \
  -p poly-matrix \
  -p poly-discord \
  -p poly-teams \
  -p poly-lemmy \
  -p poly-hackernews \
  -p poly-github \
  -p poly-server-client \
  -p poly-chat-mcp \
  -- -D warnings 2>&1
```

Expected: zero errors. Pre-existing warnings in unrelated crates are ignored.

---

## 3. WASM build check

```bash
cd apps/web && dx build --platform web 2>&1 | tail -5
```

Expected: `Client build completed successfully!`

---

## 4. Unit tests

`cargo test --workspace` does not work because the repo mixes native and WASM targets
(dependency conflicts). Run each testable crate individually instead:

```bash
cargo test \
  -p poly-core \
  -p poly-client \
  -p poly-demo \
  -p poly-stoat \
  -p poly-matrix \
  -p poly-discord \
  -p poly-teams \
  -p poly-lemmy \
  -p poly-hackernews \
  -p poly-github \
  -p poly-server-client \
  -p poly-plugin-host 2>&1
cargo test -p poly-plugin-loader-tests --tests 2>&1
cargo test -p poly-chat-mcp --test mcp_integration 2>&1
```

Expected: all tests pass. Report any failures with test name + stderr.

---

## 5. poly-web MCP smoke-test — UI changes only

> Skip this section if no `.rs`, `.css`, or `.html` files changed.
> This step uses the **poly-web** MCP server (custom Rust binary at
> `mcp/web-devtools-mcp/`), NOT Playwright or `chrome-devtools-mcp`.
> If the poly-web MCP is not loaded in the current session, report
> SKIP — do not substitute any other browser MCP.

Workflow:

1. `mcp__poly-web__launch_app` — starts `dx serve --platform web` + Chromium.
   Non-blocking; returns immediately.
2. Poll `mcp__poly-web__get_last_build_status` every 5–10s until
   `state != "Running"`. Report FAIL if `state == "Failed"` and include
   the tail of `mcp__poly-web__get_last_build_log`.
3. `mcp__poly-web__connect_cdp` — attach to the running Chromium.
4. `mcp__poly-web__take_screenshot` of the root route and each
   top-level UI surface affected by the change.
5. `mcp__poly-web__list_console_messages` — fail on any `error`-level
   console messages that are new compared to a clean baseline.
6. For each modified component / route, exercise the golden path
   (click, type, navigate) and re-screenshot.
7. `mcp__poly-web__kill_app` when done.

Pass criteria: build succeeds, no error-level console messages, all
screenshots render (no blank / 0x0 / crash overlays), every exercised
interaction responds as expected.

---

## Reporting

After running all applicable steps, respond with a table:

| Step | Result | Notes |
|------|--------|-------|
| 1. cargo check | PASS/FAIL | ... |
| 2. clippy | PASS/FAIL | ... |
| 3. WASM build | PASS/FAIL | ... |
| 4. unit tests | PASS/FAIL | N tests passed |
| 5. poly-web MCP | PASS/SKIP/FAIL | ... |
