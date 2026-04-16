# Plan — Component-Size and Dead-Code Lints

> **Created:** 2026-04-16
> **Status:** 🔵 drafted
> **Scope:** workspace-wide tooling; no production code behavior change
> **Goal:** Give teeth to the two rules AI agents keep breaking — the 100-line
> `#[component]` cap and the ban on `#[allow(dead_code)]` / `#[allow(unused*)]` —
> by enforcing both natively under `cargo check` (proc-macro `compile_error!`
> for size; `build.rs` emitting `cargo::error` for the allow ban).

---

## 1. Current state (repo audit, 2026-04-16)

### 1.1 `#[allow(...)]` occurrences

Workspace-wide ripgrep over `*.rs`:

| Attribute | Total | Files |
|-----------|-------|-------|
| `#[allow(dead_code)]` | **45** | **12** |
| `#[allow(unused)]` | 0 | 0 |
| `#[allow(unused_imports)]` | 0 | 0 |
| `#[allow(unused_variables)]` | 2 | 2 |
| `#[cfg_attr(test, allow(...))]` | 0 | 0 |

Representative hits (full list produced by the audit script in section 6):

- `clients/hackernews/src/api.rs:66,186,203` — `#[allow(dead_code)]` on private helpers
- `clients/hackernews/src/cache.rs:110` — same
- `clients/hackernews/src/types.rs:122` — on a struct field
- `clients/forgejo/src/mapping.rs:52`, `clients/github/src/mapping.rs:55` — parallel shape
- `clients/forgejo/src/lib.rs:88`, `clients/teams/src/auth.rs:121` — inside `#[cfg(test)]` tests but **not** wrapped with `cfg_attr(test, …)`
- `crates/core/src/ui/mod.rs:1018,1034` — gated with the comment `// Not all feature combinations consume this helper yet.` — the legitimate "feature skew" case
- `crates/core/src/ui/account/common/chat_view.rs:1876` — on a helper kept for near-future use
- `servers/test-github/src/routes.rs`, `servers/test-forgejo/src/routes.rs`,
  `servers/test-lemmy/src/routes.rs` — 33 hits on test-server request/response structs
  (Serde stubs that exist for future write endpoints)

`#[allow(clippy::…)]` attrs also appear (≈15 hits; `unwrap_used`, `expect_used`,
`panic`, `indexing_slicing`, `too_many_arguments`, `non_snake_case`,
`needless_pass_by_value`). The `agents.md` "ABSOLUTE PROHIBITION" rule already
bans those outside `#[cfg(test)]`, but nothing enforces it — every single
non-test hit below is a real violation:

- `clients/forgejo/src/lib.rs:414` — `#[allow(clippy::indexing_slicing)]` on prod code
- `clients/github/src/lib.rs:468` — same
- `crates/core/src/ui/favorites_sidebar.rs:53` — `#[allow(clippy::needless_pass_by_value)]`
- `crates/core/src/ui/mod.rs:1033` — `#[allow(clippy::too_many_arguments)]`
- `crates/core/src/ui/settings/ai.rs:45` — `#[allow(clippy::indexing_slicing)]`

### 1.2 Oversize `#[component]` functions

From `docs/4-ui/ui-component-150-line-refactor-checklist.md` (full audit complete
2026-03-07, scope `crates/core/src/ui/**/*.rs` only):

- **130 components measured**
- **29 components over the 100-line cap** (22% failure rate)
- **101 components compliant**

Five worst offenders (from the audit log):

| Component | File | Lines |
|-----------|------|-------|
| `FavoriteServerIcon` | `crates/core/src/ui/favorites_sidebar.rs` | **684** |
| `ServerContextMenu` | `crates/core/src/ui/account/server/context_menu.rs` | **286** |
| `DMFriendsView` | `crates/core/src/ui/account/common/channel_list.rs` | **203** |
| `AccountServerIcon` | `crates/core/src/ui/account/common/account_server_bar.rs` | **198** |
| `ServerBanner` | `crates/core/src/ui/account/common/channel_list.rs` | **187** |

The audit covered `crates/core` only. Running the proposed lint on the full
workspace will almost certainly find more — there are ~420 `#[component]` hits
workspace-wide (client backends, `apps/*`, mock servers).

`ChatView` (1129 lines) has since been partially refactored but remains over
100.

---

## 2. Declared limits (cite, do not invent)

- **Per-function / per-component body line cap:** `too-many-lines-threshold = 100`
  in `/home/laragana/workspcacemsg/clippy.toml` (the single source of truth).
- Pairs with `"clippy::too_many_lines" = true` in every `cranky.toml` (workspace
  root + ~25 per-crate overlays).
- Historical note: `docs/archive/ui-component-150-line-refactor-checklist.md` is
  titled "150-line" because the cap started there; the active limit is 100.
  The proc-macro hard-codes 100 to match `clippy.toml`; a build.rs assertion in
  `crates/lint-gate/` fails the build if the two drift apart.

`cognitive-complexity-threshold = 15` and `too-many-arguments-threshold = 6`
also live in `clippy.toml`; the component-size lint does **not** touch those —
clippy already enforces them.

---

## 3. Mechanism — which tool drives the check

**Constraint (user ask, 2026-04-16):** both checks MUST run under `cargo check`
(and by extension `cargo clippy`, since clippy runs `check` first). A separate
`cargo xtask lint` invocation — even if wired into CI — is not acceptable;
developers forget to run it, and errors should surface on the normal edit-
compile loop.

Stable Rust already supports this via two built-in escape hatches:

1. `build.rs` is executed by cargo before every `cargo check` / `cargo build`
   / `cargo clippy`. It can emit `cargo::error=` / `cargo::warning=` directives
   (error form stabilized in Rust 1.84, 2024-11; already below our MSRV). An
   error directive fails the build the same way a rustc error does.
2. Proc-macros run during `cargo check` as part of type-checking and can emit
   `compile_error!()` from their expansion, which also fails the build at
   exactly the decorated site with a precise span.

The plan therefore splits the two checks across the mechanism best suited to
each — both surface on `cargo check` with zero extra tooling.

### 3.1 Component-size lint — via proc-macro (`compile_error!` at expansion time)

The concurrent `plan-context-menu-quality-control.md` already requires every
`#[component]` to be wrapped with exactly one of `#[context_menu(...)]` /
`#[context_menu(None)]` / `#[context_menu(allow_default)]` / `#[context_menu(inherit)]`.
Those four attribute macros live in the shared proc-macro crate that this plan
also contributes to (`crates/ui-macros/`, see §6). They already see the full
`fn Foo(props: FooProps) { … }` token stream at expansion.

Add a body-line count to the macro expansion:

```rust
// inside crates/ui-macros/src/component_size.rs
fn check_body_size(item: &ItemFn, max: usize) -> Result<(), TokenStream> {
    let body = &item.block;
    let span = body.span();
    let body_text = body.to_token_stream().to_string();
    let logical_lines = body_text.lines().filter(|l| !l.trim().is_empty()).count();
    if logical_lines > max {
        return Err(quote_spanned! { span =>
            compile_error!(concat!(
                "component body exceeds ", stringify!(#max),
                " lines (found ", stringify!(#logical_lines),
                "). Extract sub-components or add // @lint-size-skip: <reason> on the attribute."
            ));
        }.into());
    }
    Ok(())
}
```

This fires on `cargo check` automatically — no xtask, no CI step, no developer
discipline required. The macro is the gate. Error span points at the body's
opening `{`, so editors jump straight there.

Pros:
- **`cargo check` native.** Exactly what the user asked for.
- **Zero false positives** — token-stream line count is not text-regex guessing
  inside strings or comments; it counts logical lines of the parsed block.
- **Authoritative span** — error points at the function body, not a separate
  tool's stdout.
- **No toolchain additions.** Stable Rust + `syn`/`quote` (already a transitive
  workspace dep via Dioxus).
- **Compiles incrementally** — the macro runs only on touched files, so a 1-
  component edit pays a 1-component cost.

Cons / edge cases:
- **Only covers components wrapped by one of the four context-menu macros.**
  That's fine because the context-menu plan makes wrapping mandatory. If a bare
  `#[component]` slips through, the `context_menu_coverage` `#[test]` in that
  plan (Phase A) catches it — an orthogonal check on attribute presence, not
  body size.
- **Threshold is a compile-time constant in the macro.** Reading
  `too-many-lines-threshold` from `clippy.toml` at macro-expansion time is
  awkward (proc-macros can't read cargo config). Instead we hard-code it in
  the macro from the project standard (100), and a CI assertion verifies
  `clippy.toml:too-many-lines-threshold == 100` so the two can't drift.
- **Inline `// @lint-size-skip: <reason>` escape hatch** — parsed off the
  attribute's preceding line in the `TokenStream` context; `reason = "..."` is
  required, empty reasons fail. Same grammar the connected-routes plan uses for
  `via = "..."` labels.

Additional net: **clippy's own `too_many_lines` lint** is already stable and
already runs under `cargo clippy`. Turn it on in `clippy.toml` with a higher
threshold (150, say) as a **secondary, last-resort** net for components that
somehow bypassed the macro. This gives two independent checks — cheap insurance.

### 3.2 `#[allow(...)]` ban — via `build.rs` regex scan (emits `cargo::error`)

A new `crates/lint-gate/` crate (library crate, but has a `build.rs`). Every
workspace member adds `lint-gate = { path = "../lint-gate" }` as a dev-dep so
the `build.rs` runs before any package compiles.

`crates/lint-gate/build.rs`:

```rust
use std::io::{BufRead, BufReader};
use ignore::WalkBuilder;

const BANNED: &[&str] = &[
    "dead_code", "unused", "unused_imports", "unused_variables",
    "unused_mut", "unused_assignments", "unused_must_use",
    "clippy::dead_code", "warnings",
];

fn main() {
    // Walk workspace root, honoring .gitignore, skipping target/.
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let ws_root = std::path::Path::new(&root).parent().unwrap();
    println!("cargo::rerun-if-changed={}", ws_root.display());

    let mut violations = 0u32;
    for result in WalkBuilder::new(ws_root).build() {
        let entry = match result { Ok(e) => e, _ => continue };
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "rs") { continue; }
        let f = match std::fs::File::open(path) { Ok(f) => f, _ => continue };
        for (lineno, line) in BufReader::new(f).lines().flatten().enumerate() {
            if !line.trim_start().starts_with("#[") && !line.trim_start().starts_with("#![") { continue; }
            if !line.contains("allow(") { continue; }
            // Strip comments; cfg_attr(test, ...) + cfg(test) blocks pass through.
            if is_cfg_test_gated(&line) { continue; }
            if has_skip_marker(path, lineno) { continue; }
            for bad in BANNED {
                if line.contains(bad) {
                    println!(
                        "cargo::error=banned #[allow({})] at {}:{} — see plan-component-lints.md §5",
                        bad, path.display(), lineno + 1
                    );
                    violations += 1;
                }
            }
        }
    }
    if violations > 0 {
        // cargo::error was already emitted; exit-code isn't needed — cargo
        // will fail the build once it sees any error directive.
    }
}
```

Why this works under `cargo check`:
- `cargo check` always runs `build.rs` before typechecking. The emitted
  `cargo::error=` directives fail the check the same way a missing symbol does.
- `println!("cargo::rerun-if-changed=<workspace-root>")` makes cargo re-run the
  scan whenever any source file in the workspace changes, but cached between
  edits to `Cargo.toml` or `build.rs` itself.
- One crate owns the scan. Adding `lint-gate` to the root `dev-dependencies`
  (or as a dep of `crates/core`) is enough — it runs exactly once per `cargo
  check` invocation for the whole workspace.

Pros:
- **`cargo check` native**, same as the component-size macro.
- **Global scope** — a proc-macro can only see items it decorates; a build.rs
  sees the whole workspace, which is what a blanket ban needs.
- **No new CLI.** Developers don't learn a new command.
- **Works in IDEs.** rust-analyzer runs `cargo check` under the hood, so
  violations show up as red squiggles in the editor on save.

Cons:
- **Clean-build cost** — scanning the workspace adds ~100–300 ms to a clean
  `cargo check`. Incremental rebuilds pay the same cost only when source
  changes (the `rerun-if-changed` stanza).
- **Regex-level false positives** (strings containing `#[allow(dead_code)]`
  verbatim). Mitigated by checking the trimmed line starts with `#[` or `#![`;
  in practice no source file today hits this.
- **Error messages include the full path** but not a rustc span with underline/
  carets. `cargo::error` messages show as plain lines in `cargo check` output.
  Acceptable — the path and line number are precise.

### 3.3 Rejected alternatives

- **`cargo xtask lint`** — rejected per user constraint.
- **Dylint** — still listed as fallback for the component-size check if the
  proc-macro approach hits a codegen case it can't see (macro-in-macro
  expansions, for example). Does not meet the `cargo check` requirement alone
  (needs `cargo dylint`), but could be invoked from a `build.rs` wrapper in a
  future iteration.
- **Custom nightly clippy lint** — same as above, plus nightly gate. Remains
  rejected.

### 3.4 Decision

- **Component-size lint:** `compile_error!()` emitted from the shared attribute
  macros in `crates/ui-macros/` (§3.1). Secondary net via clippy's built-in
  `too_many_lines` at a looser threshold.
- **`#[allow(...)]` ban:** `build.rs` in `crates/lint-gate/` emitting
  `cargo::error` directives (§3.2).

Both surface on plain `cargo check` (and `cargo clippy`, which runs `check` as
a prerequisite). No new cargo alias, no xtask invocation, no CI-only gate.

---

## 4. Component-size lint behaviour

### 4.1 What counts as "a component"

A function marked with `#[component]` (Dioxus's component attribute). Detection
is attribute-presence, not path-based — so `#[dioxus::component]` and
`#[component]` both trigger.

### 4.2 What counts as "the body"

Source lines from the opening `{` after the signature (inclusive) through the
matching closing `}` (inclusive). This matches the manual-audit counting rule
used in the existing checklist (e.g. `ChatView` = 1129 lines).

Blank lines and comment-only lines **count**. If a developer wants smaller
numbers, they can delete blank lines — but usually they want to extract a
sub-component. This is the rule we want.

### 4.3 Exclusions

- Files with `// @generated` as their first non-shebang, non-blank line (none
  today).
- Test files (`#[cfg(test)]` modules and files under `tests/`) — tests aren't
  components in practice, and if someone does wrap a test fixture with
  `#[component]` we want to know.
- Components marked with inline `// @lint-size-skip: <reason>` on the attribute
  line (parsed by the proc-macro). `<reason>` must be ≥ 10 chars; empty reasons
  fail expansion.

### 4.4 Error format

```
component-size: FAIL
  crates/core/src/ui/favorites_sidebar.rs:374: FavoriteServerIcon — 684 lines (limit 100)
    suggest: extract server list rendering / drag-drop / context menu into sub-components
  crates/core/src/ui/account/server/context_menu.rs:15: ServerContextMenu — 286 lines (limit 100)
    suggest: split by menu section (owner actions / moderation / notification prefs)

2 / 130 components over limit
```

Extraction suggestions are hand-authored from the checklist (tier-based
heuristics: "giant rsx! block" → "extract header / list / input", "nested
context menu" → "split by section"). Not every violator gets a tailored hint —
violators without a known suggestion just print `suggest: extract repeated
blocks into sub-components`.

### 4.5 Acknowledged false-negative: macro-generated components

Some macros (see section 8) emit `#[component]` functions. Generated components
are excluded via the `// @generated` header rule — each macro must emit the
header. Concretely, `#[context_menu]` and the proposed `#[connected_route]`
macro need to emit `// @generated by poly-macros::context_menu\n` as their
first line so this lint skips them.

---

## 5. `#[allow(...)]` ban behaviour

### 5.1 The banned set

Exactly these attribute forms are hard-errors:

- `#[allow(dead_code)]`
- `#[allow(unused)]`
- `#[allow(unused_imports)]`
- `#[allow(unused_variables)]`
- `#[allow(unused_mut)]`
- `#[allow(unused_assignments)]`
- `#[allow(unused_must_use)]`
- `#[allow(clippy::dead_code)]` (defensively — clippy doesn't ship it, but if
  someone mis-types we want the failure to be loud)
- `#[allow(warnings)]`
- `#![allow(dead_code)]` and the module-level `#![allow(unused*)]` variants —
  inner-attribute form, same banned set.

Multi-lint forms — `#[allow(dead_code, unused_imports)]` — trigger the check if
any banned lint name appears in the paren-list. The regex splits on `,` inside
the parens and tests each token.

### 5.2 Pass-throughs (always allowed)

- `#[cfg_attr(test, allow(…))]` — test-only gate. The lint checks the outermost
  attribute name; if it's `cfg_attr` and the first arg is `test` or
  `any(test, …)`, the attribute passes.
- `#[cfg(test)]`-gated modules and anything under `#[cfg(test)] mod tests { … }`
  — the scanner tracks brace depth from a `#[cfg(test)]` line and passes every
  attribute inside.
- Inside `tests/` dirs and `examples/` dirs — skipped wholesale.
- `#[allow(clippy::unwrap_used)]`, `#[allow(clippy::expect_used)]`,
  `#[allow(clippy::panic)]` — permitted **only** inside `#[cfg(test)]` blocks
  (matches the `agents.md` ABSOLUTE PROHIBITION rule exactly).

### 5.3 Opt-out for genuine feature-skew cases

Inline marker required on the attribute or the line immediately above:

```rust
// lint-allow-unused: Not all feature combinations consume this helper yet.
#[allow(dead_code)]
fn register_native_signup_entries() { … }
```

Rules:
1. The marker starts with `// lint-allow-unused:` and **must** be followed by a
   non-empty free-text reason (≥ 10 chars).
2. The marker binds to the next attribute below it (no blank lines between).
3. The lint records the exception in the JSON output for audit.
4. Attempting to mark a non-banned line (e.g. a function body) is a no-op —
   the marker is only meaningful on a banned `#[allow(...)]`.

Alternative considered: a per-file TOML allowlist. Rejected for this lint —
inline markers put the justification next to the code, so when the feature
gate is removed and the allow becomes dead, grep finds it immediately. TOML
allowlists rot silently. The `baseline.json` ratchet (§6.3) serves a different
purpose — a shrinking bulk-inherited-debt list — and is expected to reach
empty, not to persist.

### 5.4 Error format

```
allow-ban: FAIL
  crates/core/src/ui/favorites_sidebar.rs:53: forbidden attribute #[allow(clippy::needless_pass_by_value)]
    fix: remove the attribute and address the underlying warning
  clients/hackernews/src/api.rs:66: forbidden attribute #[allow(dead_code)]
    fix: delete the dead item, or gate it behind #[cfg(feature = "…")], or add // lint-allow-unused: <reason>
  servers/test-lemmy/src/routes.rs:203: forbidden attribute #[allow(dead_code)]
    fix: (same as above)

45 forbidden #[allow(...)] attributes found
```

### 5.5 Proposed defaults for rollout

- `#[allow(dead_code)]` → **ban** on day 1 (45 violations; see section 7 for
  the ratchet strategy).
- `#[allow(unused)]` / `#[allow(unused_imports)]` / `#[allow(unused_variables)]`
  → **ban** on day 1 (2 violations; cleanup is trivial).
- `#[allow(unused_mut)]` / `#[allow(unused_assignments)]` /
  `#[allow(unused_must_use)]` → **ban** on day 1 (0 violations; preventative).
- `#[allow(warnings)]` → **ban** on day 1 (0 violations; preventative; CI
  already uses `-D warnings` so this is belt-and-braces).
- `#[allow(clippy::<anything>)]` outside `#[cfg(test)]` → **ban** on day 1
  (~15 violations). This is the `agents.md` rule given teeth.

---

## 6. Integration with the cargo workflow

There is no separate lint command — both checks surface on plain `cargo check`,
`cargo clippy`, and every rust-analyzer save.

### 6.1 Two crates (not one)

- `crates/ui-macros/` — shared proc-macro crate (also hosts the context-menu
  and connected-routes macros from sibling plans). Adds one module
  `component_size.rs` with the body-line counter called from every
  `#[context_menu(Foo)]` / `#[context_menu(None)]` / `#[context_menu(allow_default)]`
  / `#[context_menu(inherit)]` expansion.
- `crates/lint-gate/` — library crate whose sole job is its `build.rs`. Scans
  the workspace for banned `#[allow(...)]` attributes, emits `cargo::error=`
  lines. One `lib.rs` file with `pub fn version() -> &'static str { "1" }`
  just to give cargo something to compile. `publish = false`.

Both crates join `[workspace.members]` in the root `Cargo.toml`.

### 6.2 Wiring `lint-gate` into every crate

Two options:
- **Preferred:** add `lint-gate = { path = "../lint-gate" }` to the root
  `[workspace.dependencies]` and reference it from `crates/core/Cargo.toml`
  as `lint-gate.workspace = true` under `[build-dependencies]`. Because every
  app depends on `poly-core`, every `cargo check` path pulls in the scan.
- **Alternative:** per-crate `[build-dependencies]` entry. Heavier but gives
  per-crate rerun granularity. Skip unless the preferred path has issues with
  cached build-script outputs.

Either way: zero manual invocation, zero `xtask` alias, zero CI-only gate.

### 6.3 Baseline ratchet — still needed, different mechanism

The build.rs reads `crates/lint-gate/baseline.json` at the start of the scan.
Violations present in the baseline are downgraded to `cargo::warning=`;
violations NOT in the baseline stay as `cargo::error=`. Regenerate the
baseline with `cargo check --features regen-baseline` (the feature flips the
build script into write-mode, serializes all current violations to
`baseline.json`, then skips the error emission for that run).

This preserves the ratchet semantics — existing debt grandfathered, new
violations fail the build — while keeping everything inside `cargo check`.

### 6.4 GitHub Actions

No new CI step needed. `.github/workflows/lint-test.yml` already runs
`cargo check` and `cargo clippy`; both now automatically enforce these lints
via the mechanisms above. One-line change: confirm the existing `cargo clippy`
step has `--all-targets --all-features` so every feature combination sees the
build.rs.

### 6.5 Editor integration

Because violations flow through `cargo check`, rust-analyzer surfaces them as
native red squiggles on save. Hovering the squiggle shows the message
(`banned #[allow(dead_code)] at …` or `component body exceeds 100 lines …`).
No editor plugin needed.

### 6.6 Interaction with existing `cargo clippy`

- `clippy::too_many_lines` stays on in `clippy.toml` at a looser 150 threshold
  as a defense-in-depth net for non-component functions and for any `#[component]`
  that somehow bypassed the wrapper macros. The proc-macro is the primary gate;
  clippy is the belt to the macro's suspenders.
- A CI assertion (one `grep` in the build.rs of `lint-gate`) verifies
  `clippy.toml:too-many-lines-threshold` still exists and is `≤ 150`. Drift
  between the macro's hard-coded `100` and clippy's 150 is tolerated (we want
  two different thresholds for the two different nets). Drift that *removes*
  the clippy limit entirely is an error.

---

## 7. Rollout + ratchet

Day 1 (warn-phase, single PR):

1. Land `crates/ui-macros/` and `crates/lint-gate/` with both checks active.
2. Run `cargo check --features regen-baseline` → commit
   `crates/lint-gate/baseline.json` with the existing 45 `#[allow(dead_code)]`
   + 2 `unused_variables` + 15 `clippy::*` + (per-macro-wrap) 29 oversize
   components.
3. Normal `cargo check` / `cargo clippy` from there: baseline entries
   downgrade to `cargo::warning=`, anything new stays `cargo::error=`.

Week 1-4 (cleanup waves, one PR each):

| Wave | Target | PRs | Owner guess |
|------|--------|-----|-------------|
| 1 | Delete genuinely dead code in `clients/hackernews/*` (reduce 5 hits) | 1 | whoever owns HN |
| 2 | Fix or feature-gate `clients/forgejo/src/{lib,mapping}.rs` and parallel in github | 1 | forge-backend team |
| 3 | Convert `servers/test-*/routes.rs` stubs to `#[cfg_attr(…, allow(dead_code))]` under a feature flag, or wire the routes | 1-2 | test-server maintainers |
| 4 | Refactor the tier-1/2 oversize components (7 files, see audit checklist) | 3-5 | UI team |
| 5 | Tier-3/4 monsters (`FavoriteServerIcon`, `ChatView`) | dedicated sprint | |

After each wave, re-run `cargo check --features regen-baseline` and commit the
shrunken `baseline.json`. The file is the ratchet — it only ever decreases.

Hard deadline: baseline must be **empty** before the 1.0 release. Until then,
the check keeps regression pressure on without blocking feature work.

---

## 8. Interaction with concurrent plans

### 8.1 `plan-context-menu-quality-control.md`

That plan (separate, in-flight) introduces a `#[context_menu]` attribute
macro whose expansion emits a `#[component]` function. If our lint counts the
expanded source, those auto-generated components will dominate the report
with noise we can't fix without editing the macro.

**Resolution:** the `#[context_menu]` macro must emit
`// @generated by poly-macros::context_menu` as the first line of every file
it writes, or equivalently, must emit a `#[allow(clippy::too_many_lines)]`
*only on the generated item* — but we've banned that allow. So the
`@generated` header rule is the right path.

If the macro expands in-place into a user source file (as proc-macros usually
do) rather than into a generated file, the lint's source-text view sees the
original attribute site, not the expansion — the user's `#[context_menu]`
annotation takes one line, and component-size counts the user's source body.
This is the preferred outcome; re-confirm when the macro lands.

### 8.2 `plan-connected-routes-static-check.md`

Same concern re: macro-expanded `#[component]` items. Same resolution.

**Shared infrastructure opportunity:** the three plans land two proc-macro
contributions (context-menu decorator DSL + `#[connected]` route decorator +
this plan's `check_body_size`) in the same `crates/ui-macros/` crate. Keep
the source-walking helpers (`ignore::WalkBuilder` wrapper, span-preserving
attribute parser) in a shared `crates/ui-macros/src/scan.rs` module so
both `build.rs`-driven checks and expansion-time checks reuse the same
`.gitignore`-aware walk. Not required for day 1; note for later.

---

## 9. Open questions

1. Should the `#[allow(...)]` ban also cover `#[deny(…)]` override attrs like
   `#[forbid(dead_code)]`? (Leaning no — those tighten, not loosen.)
2. `clippy.toml`'s 100-line threshold currently applies to **all** fns, not
   just components. Is that intentional, or do we want a separate per-
   component vs per-function threshold? (Checklist wording says "100 lines
   fills a standard terminal" — one threshold seems right.)
3. Should the baseline file live at `crates/lint-gate/baseline.json` (next to
   the build script that reads it) or at `.poly/lint-baseline.json` (project
   root, like `.gitignore`)? Leaning toward crate-local so the build script
   does not reach above its own manifest dir.
4. Do we want the pre-commit hook installed by default on `jj init`-equivalent
   onboarding, or strictly opt-in? (Leaning opt-in; CI is the source of
   truth.)
5. `apps/desktop/Cargo.toml` and similar have cfg-gated dependencies — do any
   modules need `#[cfg_attr(not(feature = "…"), allow(dead_code))]`? None
   today, but the lint should treat `cfg_attr(<any feature gate>, allow(…))`
   as "accepted, print a note in `--json` output" so platform-conditional
   allows don't sneak through.

---

## 10. Out of scope

- Formatting / `rustfmt` integration (already run in CI).
- Other `clippy::*` allows not listed in section 5.1 (e.g. `#[allow(
  clippy::module_name_repetitions)]` — `cranky.toml` disables that lint
  globally; an allow would be a no-op).
- Dead-struct-field detection beyond what rustc already reports.
- Line-count limits on non-component functions (covered by
  `clippy::too_many_lines` already).
- Cross-file analysis (call-graph dead-code detection) — that's
  `cargo-udeps` / `cargo-machete` territory and is a separate plan.
- Enforcement against `.rs` files not in the cargo workspace (none today).
- Editor integration (VS Code "lint on save"). Doable via the `--json`
  output; leave to a follow-up.
