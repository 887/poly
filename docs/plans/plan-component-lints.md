# Plan — Component-Size and Dead-Code Lints

> **Created:** 2026-04-16
> **Status:** 🔵 drafted
> **Scope:** workspace-wide tooling; no production code behavior change
> **Goal:** Give teeth to the two rules AI agents keep breaking — (1) the ban on
> `#[allow(dead_code)]` / `#[allow(unused*)]`, (2) the size of `rsx!` macro
> bodies inside `#[component]` functions (clippy misses this entirely because
> it doesn't see into macro expansions; it's the actual failure mode behind
> the 684-line `FavoriteServerIcon` and 1129-line `ChatView`). Both enforced
> natively under `cargo check` — proc-macro `compile_error!` at each oversize
> `rsx!` span, `build.rs` emitting `cargo::error=` for the allow ban.

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

- **130 components measured** (whole-function line count)
- **29 components over the old 100-line *function* cap** (22% failure rate)
- **101 components compliant under the old rule**

Five worst offenders (from the audit log):

| Component | File | Lines |
|-----------|------|-------|
| `FavoriteServerIcon` | `crates/core/src/ui/favorites_sidebar.rs` | **684** |
| `ServerContextMenu` | `crates/core/src/ui/account/server/context_menu.rs` | **286** |
| `DMFriendsView` | `crates/core/src/ui/account/common/channel_list.rs` | **203** |
| `AccountServerIcon` | `crates/core/src/ui/account/common/account_server_bar.rs` | **198** |
| `ServerBanner` | `crates/core/src/ui/account/common/channel_list.rs` | **187** |

**Caveat — the numbers above count the whole function body, which is the
wrong metric.** Inspecting the five worst offenders: ≥90% of each file's line
count lives inside a single `rsx! { ... }` invocation. Under the revised check
(§3.1 — primary cap on `rsx!` body size, not fn body) the same components
would fail in almost the same ratio, but the error message would point the
fix at the right place (extract a chunk of markup, not "extract a helper").
Clippy's own `too_many_lines` misses all of these today because it doesn't
see into macro expansions; that is precisely the blind spot this plan closes.

The audit covered `crates/core` only. Running the revised lint on the full
workspace will almost certainly find more — there are ~420 `#[component]` hits
workspace-wide (client backends, `apps/*`, mock servers).

`ChatView` (1129 lines) has since been partially refactored but remains over
the rsx! cap.

---

## 2. Declared limits (cite, do not invent)

Two thresholds, two different scopes:

- **`rsx!` body cap — 100 lines, hard error.** Primary gate. Proc-macro in
  `crates/ui-macros/` hard-codes this constant. Rationale: 100 roughly fills
  a standard terminal, and by that point any `rsx!` block benefits from being
  sliced into a child component.
- **Function body cap — 250 lines, hard error.** Secondary gate for pathological
  non-markup bloat. Proc-macro hard-codes this too. `too-many-lines-threshold`
  in `/home/laragana/workspcacemsg/clippy.toml` is set to `250` so clippy
  agrees. `lint-gate`'s build.rs asserts the two values stay in sync (the macro
  constant and the clippy config).
- Pairs with `"clippy::too_many_lines" = true` in every `cranky.toml` (workspace
  root + ~25 per-crate overlays). Clippy acts as a defense-in-depth duplicate
  of the 250 fn cap; it cannot enforce the 100 rsx! cap because it can't see
  into macro bodies.
- Historical note: `docs/archive/ui-component-150-line-refactor-checklist.md` is
  titled "150-line" because the cap started there; the legacy whole-function
  number is now obsolete under the revised rsx!-primary rule.

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

### 3.1 Component-size lint — primary check is `rsx!` body size, not fn body

**Intent correction (2026-04-16):** the real thing that rots the UI is giant
`rsx! { ... }` invocations — the tree of markup the component renders. Clippy's
`too_many_lines` does **not** see into macro bodies; it counts only the
function's Rust statements. That's why today's 684-line `FavoriteServerIcon` and
1129-line `ChatView` pass clippy despite being catastrophically over-grown:
almost all of those lines live inside a single `rsx! { ... }` block that clippy
treats as one statement.

Fix: primary gate counts lines inside each `rsx!` invocation. Secondary
(looser) gate counts the function body for pathological non-markup bloat.

#### 3.1.a Primary — per-`rsx!` body line cap (hard limit: 100)

The `#[context_menu(...)]` attribute macros (which every `#[component]` carries,
per `plan-context-menu-quality-control.md`) see the full body `TokenStream` at
expansion. We walk the `syn::Block` and inspect every `Expr::Macro` /
`Stmt::Macro` whose path resolves to `rsx` / `dioxus::rsx` / `dioxus_core::rsx`.
For each hit, count logical lines inside the delimiter:

```rust
// crates/ui-macros/src/rsx_size.rs
const MAX_RSX_LINES: usize = 100;

fn check_rsx_blocks(body: &Block) -> Result<(), TokenStream> {
    let mut offenders = Vec::new();
    visit_rsx_macros(body, |mac| {
        let text = mac.tokens.to_string();
        let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
        if lines > MAX_RSX_LINES {
            offenders.push((mac.span(), lines));
        }
    });
    if let Some((span, lines)) = offenders.into_iter().next() {
        return Err(quote_spanned! { span =>
            compile_error!(concat!(
                "rsx! body exceeds ", stringify!(#MAX_RSX_LINES),
                " lines (found ", stringify!(#lines),
                "). Extract the largest top-level tag into its own #[component]. ",
                "Each sub-component is also a Dioxus re-render boundary, so this ",
                "also narrows re-render scope.",
            ));
        }.into());
    }
    Ok(())
}
```

Why `rsx!` body, not function body:
- **Harder to game than fn body.** To shrink an `rsx!` block, an agent has to
  actually extract markup into a sub-component — which, by construction, creates
  a real new component with props. You cannot "delete blank lines" out of
  structural JSX-style markup the way you can out of statements.
- **Right signal for the damage.** The render cost is the rsx! tree. The diff
  churn is the rsx! tree. The unreadability is the rsx! tree.
- **Extraction is mechanically guided.** The error message names a single
  remedy ("extract the largest top-level tag"), not a vague "extract repeated
  blocks" that invites hollow sub-components.
- **Matches Dioxus architecture.** In Dioxus 0.7.3 each `#[component]` is a
  re-render boundary. Splitting rsx! across components is not a cosmetic fix —
  it actually isolates signal reads and reduces re-renders.

Multiple `rsx!` blocks in one fn (e.g. `if cond { rsx! { … } } else { rsx! { …
} }`) are each checked independently against their own cap. Total-across-blocks
is not tracked.

#### 3.1.b Secondary — fn body cap (soft limit: 250 lines, warn-only)

A much looser backstop for the rare component that is pathologically bloated
outside rsx! — giant match, deeply nested hook chains, huge `use_memo` closures.
Emits `compile_error!` only over 250 lines (vs the primary rsx! cap of 100).
Most components will hit 3.1.a long before 3.1.b.

Rationale for keeping 3.1.b at all rather than dropping the fn-body idea:
catches cases where an agent hollowed out an `rsx!` into `rsx! { { build_tree(ctx) } }`
and dumped a 500-line node-builder fn into the same component. Unlikely but
possible; 50 lines of macro is cheap insurance.

#### 3.1.c Mechanics

Both checks fire on `cargo check` automatically via the `#[context_menu(...)]`
macro expansion — no xtask, no CI step, no developer discipline required.

Pros:
- **`cargo check` native.** Errors surface in rust-analyzer on save.
- **Counts what actually matters** — markup size, not statement count.
- **Can't be gamed by blank-line deletion** inside rsx! (whitespace collapses
  but the tag tree is still the tag tree).
- **Authoritative span** — error points at the specific rsx! macro call, not
  the whole fn body. Editor jumps straight to the right place.

Cons / edge cases:
- **Only covers components wrapped by one of the four `#[context_menu(...)]`
  attribute macros.** Fine because the context-menu plan makes wrapping
  mandatory; the `context_menu_coverage` check in that plan catches bare
  `#[component]` slips.
- **`rsx!` detection is path-based.** If a backend uses
  `use dioxus::rsx as render;` and calls `render! { ... }`, the check misses
  it. Mitigation: `clippy.toml` disallows renaming `dioxus::rsx` imports;
  build.rs in `lint-gate` scans for `use dioxus::rsx as` and emits
  `cargo::error=` if anyone tries.
- **Thresholds are compile-time constants in the macro.** Proc-macros can't
  read `clippy.toml`, so the rsx! cap (100) and fn cap (250) are hard-coded in
  `crates/ui-macros/`. `lint-gate`'s build.rs asserts
  `clippy.toml:too-many-lines-threshold == 250` so the secondary fn-body cap
  and clippy stay in sync.
- **Inline `// @lint-size-skip: <reason>` escape hatch** still available on
  the attribute line. `<reason>` must be ≥ 10 chars; empty reasons fail.
  Applies to both 3.1.a and 3.1.b.

Additional net: clippy's own `too_many_lines` lint remains on at 250 as a
defense-in-depth duplicate of 3.1.b. Three checks total, zero configuration
work for developers.

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

### 4.2 What gets counted (and why)

Two separate counters run per component, both at macro expansion time:

1. **`rsx!` body line count (primary, cap 100).** Inside every `rsx! { ... }`
   invocation reachable from the component body, count logical lines
   (non-blank, inside the macro delimiter). Each `rsx!` block is checked
   independently against its own 100-line cap. A component with several
   conditional `rsx!` branches passes as long as each branch is ≤ 100, even
   if they sum to more.
2. **Function body line count (secondary, cap 250).** Lines from the opening
   `{` after the signature through the matching `}`. Captures pathological
   non-markup bloat.

Blank lines and comment-only lines count for both — deleting comments to hit
the limit is a tell that the component needed extraction anyway. But for
`rsx!`, agents can't meaningfully shrink by whitespace edits because the tag
tree drives the structure; the counter is really counting tags and attrs.

### 4.3 Exclusions

- Files with `// @generated` as their first non-shebang, non-blank line (none
  today).
- Test files (`#[cfg(test)]` modules and files under `tests/`).
- Components marked with inline `// @lint-size-skip: <reason>` on the
  `#[context_menu(...)]` attribute line. `<reason>` must be ≥ 10 chars; empty
  reasons fail expansion. Applies to both the rsx! and fn caps.

### 4.4 Error format

Primary (rsx! cap):

```
error: rsx! body exceeds 100 lines (found 684)
 --> crates/core/src/ui/favorites_sidebar.rs:402:5
    |
402 |     rsx! {
    |     ^^^^^^
    = help: extract the largest top-level tag into its own #[component].
            Each sub-component is also a Dioxus re-render boundary, so this
            also narrows re-render scope.
```

Secondary (fn cap):

```
error: component function body exceeds 250 lines (found 412)
 --> crates/core/src/ui/some_control.rs:88:43
    = help: split out non-markup helpers (match arms, memoized closures,
            event handlers) into free functions or a sibling module.
```

The extraction hint is always the same — for rsx!, "extract the largest
top-level tag"; for fn, "move non-markup helpers out." No per-component
tailored hints: the hint is implied by what actually exceeded the cap.

### 4.5 Acknowledged false-negative: macro-generated components

Some macros (see section 8) emit `#[component]` functions. Generated components
are excluded via the `// @generated` header rule — each macro must emit the
header. Concretely, `#[context_menu]` and any future component-emitting macro
need to emit `// @generated by poly-macros::context_menu\n` as the first line
of their expansion so this lint skips them.

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

- `clippy::too_many_lines` stays on in `clippy.toml` at threshold `250` —
  matching the proc-macro's secondary fn-body cap (3.1.b). Clippy is a
  defense-in-depth duplicate of that secondary gate for any `#[component]`
  that bypassed the wrapper macros, and covers non-component functions.
  Clippy **cannot** enforce the primary rsx!-body cap (3.1.a) because it
  doesn't see into macro expansions; that's the proc-macro's exclusive job.
- A build.rs assertion in `lint-gate` reads `clippy.toml` and errors if
  `too-many-lines-threshold` is missing, > 250, or drifts away from the
  `MAX_FN_LINES` constant in `crates/ui-macros/src/rsx_size.rs`. The two
  must stay in sync because they're enforcing the same fn-body rule via
  two tools. The rsx! cap (`MAX_RSX_LINES = 100`) has no clippy counterpart
  and is not part of that assertion.

---

## 7. Rollout + ratchet

Day 1 (warn-phase, single PR):

1. Land `crates/ui-macros/` and `crates/lint-gate/` with both checks active.
2. Run `cargo check --features regen-baseline` → commit
   `crates/lint-gate/baseline.json` with the existing 45 `#[allow(dead_code)]`
   + 2 `unused_variables` + 15 `clippy::*` + (per-macro-wrap) the oversize-
   rsx! offenders. The old fn-cap audit counted 29 components over 100 lines;
   the rsx!-primary check will flag a similar-but-not-identical set (the
   rsx!-dominant offenders like `FavoriteServerIcon`, `ChatView`, `ServerContextMenu`
   all still fail; any component that was 110-line-fn-with-30-line-rsx passes
   under the revised rule). Expect the oversize count after the regen run to
   drop slightly — the full number lands when the check runs in anger.
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
2. `clippy.toml`'s `too-many-lines-threshold` applies to **all** fns, not just
   components. Under the revised plan we lift it to 250 (matching the fn-body
   secondary cap) so it aligns with 3.1.b for every function, component or
   not. The primary rsx!-body cap (100) is enforced only inside `#[component]`
   wrappers because rsx! outside a component is rare and unidiomatic — revisit
   if that stops being true.
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
