# poly-chat-mcp fuzz targets

`cargo fuzz` targets for the persona subsystem.  Currently covers the
deny-wins source-resolution algorithm in `persona::context::is_chat_included`.

> **Why a separate crate?**  `cargo fuzz` requires a nightly toolchain with
> `llvm-tools-preview`.  The workspace builds on stable (`rust-version = "1.85"`
> in `Cargo.toml`).  This crate is excluded from the workspace (see root
> `Cargo.toml → [workspace.exclude]`) and has its own `rust-toolchain.toml`
> pinned to nightly — the same pattern as `tools/lints/poly-lints/`.

---

## Prerequisites

```bash
# Install cargo-fuzz once (needs nightly).
cargo install cargo-fuzz

# Ensure nightly is installed (rust-toolchain.toml in this directory
# auto-selects it, but you need it available):
rustup toolchain install nightly --component llvm-tools-preview rust-src
```

---

## One-command local invocation

```bash
# From the repo root:
cd mcp/chat-mcp/fuzz
cargo +nightly fuzz run source_resolve
```

This runs indefinitely.  For a timed run (e.g. 5 minutes):

```bash
cargo +nightly fuzz run source_resolve -- -max_total_time=300
```

With the seed corpus (recommended for first runs):

```bash
cargo +nightly fuzz run source_resolve corpus/source_resolve
```

The fuzzer prints a summary line every 10 seconds.  A healthy run looks like:

```
#1048576 pulse  cov: 42 ft: 87 corp: 12/256b lim: 4096 exec/s: 10485 rss: 64Mb
```

Zero panics and zero assertion failures (`SUMMARY: libFuzzer: no errors`) is
the acceptance bar.

---

## What this tests

`source_resolve` fuzzes `poly_chat_mcp::persona::is_chat_included`, which
implements the deny-wins rule for persona source resolution:

- If ANY matching `PersonaSourceRow` has `include=false`, the candidate
  `(account_id, chat_id)` is **excluded** from the persona bundle —
  regardless of how many allow rows also match.
- Only when no deny matches AND at least one allow matches is the candidate
  **included**.
- Default with no matching rules: **denied**.

The fuzz target asserts that the fast path (`is_chat_included`) agrees with
an independent slow reference implementation on every input.  Any panic in
either path or any divergence between the two is a finding.

---

## Adding a new seed

Seed corpus files live in `corpus/source_resolve/`.  They are raw bytes that
`Arbitrary::arbitrary` deserialises into the `FuzzInput` struct (see
`fuzz_targets/source_resolve.rs`).

The easiest way to add a seed is to let the fuzzer generate one automatically:
any interesting input the fuzzer finds gets saved to `corpus/source_resolve/`
automatically.  You can copy that file to the repo.

For hand-crafted seeds, run the seed-generation test:

```bash
cd mcp/chat-mcp/fuzz
cargo +nightly test gen_seeds
```

This regenerates all files in `corpus/source_resolve/` from known logical
scenarios.  After adding a new scenario to `seeds::gen_seeds` in `src/lib.rs`,
re-run this test and commit the new `.bin` file.

---

## Reproducing a CI crash from an artefact bundle

1. Download the `fuzz-crash-source_resolve-<run-id>` artefact from the
   failing GitHub Actions run.
2. Extract the bundle — it contains the crash input file and `fuzz-output.txt`.
3. Run the fuzzer with the specific crash input:

```bash
cd mcp/chat-mcp/fuzz
cargo +nightly fuzz run source_resolve \
    <path-to-extracted>/artifacts/source_resolve/crash-<hash>
```

The fuzzer will re-run just that one input and print the full stack trace
(it is compiled with debug symbols even in `--release` mode).

To minimise the crash input (find the smallest reproducer):

```bash
cargo +nightly fuzz tmin source_resolve \
    <path-to-extracted>/artifacts/source_resolve/crash-<hash>
```

---

## 5-minute zero-finding acceptance bar

The CI nightly job runs for exactly 5 minutes (`-max_total_time=300`).
A run is considered **passing** if it completes with no crashes and no
reference-impl divergence (`SUMMARY: libFuzzer: no errors`).

Local acceptance check (mirrors CI):

```bash
cd mcp/chat-mcp/fuzz
cargo +nightly fuzz run source_resolve corpus/source_resolve \
    -- -max_total_time=300 -print_final_stats=1
echo "Exit code: $?"  # 0 = no findings
```

---

## Related files

| File | Purpose |
|---|---|
| `fuzz_targets/source_resolve.rs` | Fuzz target entry point |
| `src/lib.rs` | `FuzzSourceRow` (Arbitrary mirror), reference impl, seed gen |
| `corpus/source_resolve/` | Hand-crafted seed corpus (6 files) |
| `rust-toolchain.toml` | Pins nightly for this directory |
| `.github/workflows/fuzz-personas.yml` | Nightly CI run (06:00 UTC) |
| `mcp/chat-mcp/src/persona/context.rs` | `is_chat_included` + `PersonaSourceRow` |
| `docs/plans/plan-persona-quality-gates.md` | Phase R — design rationale |
