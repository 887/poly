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

## 2. Clippy (poly-core)

```bash
cargo clippy -p poly-core -- -D warnings 2>&1
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
cargo test -p poly-core -p poly-demo -p poly-stoat -p poly-plugin-host 2>&1
cargo test -p poly-plugin-loader-tests --tests 2>&1
```

Expected: all tests pass. Report any failures with test name + stderr.

---

## 5. Playwright — UI changes only

> Skip this section if no `.rs`, `.css`, or `.html` files changed.
> Requires `dx serve --platform web` already running on port 3000.

```bash
cd /home/laragana/workspcacemsg && npx playwright test --project=chromium 2>&1
```

Expected: all tests pass. Screenshot artifacts saved to `test-results/` on failure.

---

## Reporting

After running all applicable steps, respond with a table:

| Step | Result | Notes |
|------|--------|-------|
| 1. cargo check | PASS/FAIL | ... |
| 2. clippy | PASS/FAIL | ... |
| 3. WASM build | PASS/FAIL | ... |
| 4. unit tests | PASS/FAIL | N tests passed |
| 5. playwright | PASS/SKIP/FAIL | ... |
