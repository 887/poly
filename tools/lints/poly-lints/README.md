# poly-lints

HIR-precise [dylint] lints for the two WASM-hang classes that the regex
scripts in `tools/scripts/forbid-*.sh` currently gate. Phase 5 Track B
of:

- `docs/plans/plan-batched-signal.md` (hang class #1)
- `docs/plans/plan-use-spawn-once.md` (hang class #3)

## Lints shipped

| Name | Level | What it catches |
|------|-------|-----------------|
| `poly::raw_signal_write` | `deny` | `sig.write()` on `dioxus_signals::Signal<T>` / `ReadOnlySignal<T>`. |
| `poly::use_effect_spawn_cycle` | `deny` | `use_effect(\|\| { … spawn(async move { … sig.batch/write/set/pending_update(…) }) })`. |

Both lints resolve the target type / callee by **canonical DefPath**
(e.g. `dioxus_signals::Signal::write`) rather than by identifier string,
so renaming a local binding or re-exporting through `dioxus::prelude`
does not defeat them.

## Workspace status — EXCLUDED

This crate is NOT a member of the root workspace. It is listed under
`[workspace.exclude]` in the root `Cargo.toml`. Reason: dylint depends
on rustc internal crates (`rustc_hir`, `rustc_lint`, `rustc_middle`,
…) via `extern crate rustc_*`, which requires a nightly toolchain with
the `rustc-dev` + `llvm-tools-preview` components. Bringing the whole
workspace onto nightly would break the `rust-version = "1.85"` stable
pin.

The crate has its own `rust-toolchain.toml` pinning `nightly`, scoped
to this directory. The workspace itself still builds on stable.

## Running locally

```sh
# One-time install (requires nightly + rustc-dev in rustup):
cargo install cargo-dylint dylint-link
rustup component add --toolchain nightly rustc-dev llvm-tools-preview rust-src

# Build the lint library:
cd tools/lints/poly-lints
cargo build --release

# Run against the whole workspace:
cd ../../..
DYLINT_LIBRARY_PATH="$(pwd)/tools/lints/poly-lints/target/release" \
  cargo dylint --all -- --workspace

# Or just the UI crate where the hang classes live:
DYLINT_LIBRARY_PATH="$(pwd)/tools/lints/poly-lints/target/release" \
  cargo dylint --all -- -p poly-core
```

`dylint.toml` at the workspace root already points at this crate so
`cargo dylint --all` picks it up without extra args (provided
`DYLINT_LIBRARY_PATH` or a `--path` flag resolves it).

## CI integration

`.github/workflows/lint-test.yml` runs `cargo dylint --all` as a
**non-required** job (allows failure). The regex scripts
(`tools/scripts/forbid-*.sh`) remain the required gate for Phase 5
Track A until this crate is proven stable on CI's pinned nightly.

Timeline to flip dylint to required:

1. Two consecutive green dylint runs on `main`.
2. Confirmation that the dylint pass fires on the same sites the regex
   scripts fire on (run `cargo dylint --all -- -p poly-core` after
   reverting a known-bad migration).
3. Remove `continue-on-error: true` from the dylint CI step.

## Allowlist convention

Silence a site with an `#[allow]` attribute on the enclosing function
or `impl`:

```rust
#[allow(poly::raw_signal_write)] // reason: bootstrap path, not a hot signal
fn boot(sig: Signal<Foo>) {
    sig.write(); // lint silenced
}
```

The `// reason: …` comment is a convention, not enforced by the lint
itself. The regex allowlists
(`tools/scripts/signal-write-allowlist.txt`,
`tools/scripts/use-effect-spawn-cycle-allowlist.txt`) remain the
authoritative record of exempted sites with their rationale — this
lint is an AST-level second layer, not a replacement.

## Known divergences from the regex scripts

These are the heuristic gaps between the two layers. Neither is a bug
in either layer — just noting where the HIR version is stricter /
laxer than the regex version.

| Case | Regex script | dylint crate |
|------|-------------|------------|
| `rwlock.write().await` on `tokio::sync::RwLock` | Skipped via `.write().await` token filter. | Skipped because receiver DefPath is `tokio::sync::RwLock`, not `dioxus_signals::Signal`. |
| `.write()` on a chained continuation line (`foo\n    .write()`) | Resolved via previous-line carrier heuristic. Can misidentify the receiver. | Resolved via HIR — always correct. |
| `.write(buf)` on `std::io::Write` | Excluded by `.write()` arity match (zero args required). | Excluded because receiver is not a `Signal` DefPath AND the lint additionally requires zero args. |
| `use_effect(move \|\| { helper(sig); })` where `helper` internally spawns + writes `sig` | Flagged if `helper` is inlined in the same file and the regex sees `spawn(async move { … .batch(` inside it; false-negative otherwise. | False-negative — the HIR visitor does not follow function calls into other bodies. (Matches regex blind spot; both layers need refinement if it becomes a real issue.) |
| A `use_effect` whose spawn writes a signal the effect **does NOT read** | Flagged by both (same over-approximation). | Flagged — HIR version currently also does not correlate read/write signal identity across the outer/inner scopes; that's future work. |
| Lint on `const` / generated-macro expansions | Regex operates on pre-expansion text. | HIR operates on post-expansion code; macro-generated `.write()` calls WILL be flagged. Use `#[allow]` on the macro call site if this is a problem. |

## Gnarliest HIR-resolution problem

Resolving `Signal::write` by DefPath required comparing against the
canonical crate + module path (`dioxus_signals::Signal`) rather than
the surface-level `use dioxus::prelude::Signal` that most callers
write. `rustc`'s `def_path_str` **does** return the canonical form,
but we also have to peel auto-deref coercions on the receiver — a
`(&sig).write()` call has an adjusted type of `&Signal<T>`, not
`Signal<T>`, so `expr_ty_adjusted(receiver).peel_refs()` is required
before the ADT match. Missing `.peel_refs()` caused the initial
version of the lint to silently miss every `&sig.write()` site.

## Scope — deliberately narrow

This crate holds **only** the two lints above. Do not add general-purpose
or style lints here. The regex scripts are the Phase 5 Track A gate; this
crate is the optional Track B precision upgrade. Adding unrelated lints
expands the nightly-toolchain / CI-flakiness surface area unnecessarily.

[dylint]: https://github.com/trailofbits/dylint
