# Plan — Component-Size and Dead-Code Lints

> **Created:** 2026-04-16
> **Status:** 🔵 drafted
> **Scope:** workspace-wide tooling; no production code behavior change
> **Goal:** Give teeth to the two rules AI agents keep breaking — the 100-line
> `#[component]` cap and the ban on `#[allow(dead_code)]` / `#[allow(unused*)]` —
> by adding a `cargo xtask lint` check that CI runs alongside `cargo clippy`.

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
  The xtask lint reads **directly from `clippy.toml`** so changing the threshold
  in one place updates both.

`cognitive-complexity-threshold = 15` and `too-many-arguments-threshold = 6`
also live in `clippy.toml`; the component-size lint does **not** touch those —
clippy already enforces them.

---

## 3. Mechanism — which tool drives the check

Three candidates, evaluated against this codebase:

### 3.1 Option A — regex via `rg` + `cargo xtask lint` (RECOMMENDED PRIMARY)

**How:** a new binary crate `xtask/` with one entry point that runs two checks:

1. Walk every `*.rs` in the workspace, skip `target/`, `vendor/`, anything
   matched by `.gitignore`, and any file the first line of which starts with
   `// @generated`. Use the `ignore` crate (pulls in `.gitignore` for free) —
   not a `walkdir` + manual-filter stack.
2. Invoke both checks in parallel; report violations as a single table.

Pros:
- Zero new toolchains. `cargo xtask lint` runs on stable, no nightly, no
  `clippy_utils`, no dylint driver.
- Matches the user's stated "regex is fine" bar.
- Fast (<1 s on this workspace today; scales).
- Trivial to add opt-out paths, exception markers, and deterministic output.
- Works offline — nothing in CI or the dev loop has to reach the network.
- Easily diff-able — the CLI prints stable `path:line:col: message` lines.

Cons:
- Regex for `#[component]` body size cannot count **logical** lines through a
  proc-macro expansion — it counts the source text between `fn Foo(...) { … }`.
  This is fine: the cap is a *readability* cap, not a codegen-size cap. The
  existing manual audit uses exactly the same counting rule.
- String / comment false positives for the `#[allow(...)]` regex. Mitigated by
  (a) requiring the `#[` token at column-0 modulo whitespace, (b) skipping
  lines inside `//` comments, (c) skipping content between `r#"…"#` / `"…"`
  triple-quoted blocks. In practice the code-vs-comment distinction handles
  every current occurrence — the 45 real hits are all attributes.

### 3.2 Option B — `dylint` with a custom lint crate

**How:** write a dylint lint crate that sees the HIR, walks
`#[component]`-decorated items, and counts lines of the function body post-
expansion. A second lint walks `#[allow(…)]` attrs and hard-errors on the
banned list.

Pros:
- Real AST — no false positives from strings/comments.
- Dylint runs on stable; no nightly required.

Cons:
- Adds `cargo-dylint` as a required dev-dependency (one more binary to install,
  one more cache to keep warm in CI).
- Dylint compiles against a pinned rustc internals crate — breaks on Rust
  toolchain bumps more often than xtask would.
- Line counting after macro expansion counts the Dioxus-generated scaffolding,
  which gives a higher number than what the developer actually typed. Either
  we map spans back to the source file (hard) or we count source spans only
  (at which point we're doing what option A already does).
- Overkill for two lints that each fit in a screen of Rust.

### 3.3 Option C — custom clippy lint (nightly `clippy_utils`)

Rejected. Requires a nightly toolchain pin for the whole workspace just to run
lints. CI currently uses `stable`; introducing a nightly gate would force every
developer to install a second toolchain and complicates cache keys in
`.github/workflows/lint-test.yml`. Not worth it for two lints.

### 3.4 Decision

**Primary:** `cargo xtask lint` (option A).
**Fallback / escalation:** if false-positive rate creeps above 1%, upgrade the
component-size check to option B (dylint) while keeping the `#[allow(...)]`
check as regex — that one is genuinely trivial.

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
- Files listed in the per-workspace `xtask/exclusions.toml` — empty on day 1,
  reserved for escape hatches that get a mandatory `reason = "…"` string.

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

Alternative considered: a per-file TOML allowlist (`xtask/exclusions.toml`).
Rejected for this lint — inline markers put the justification next to the
code, so when the feature gate is removed and the allow becomes dead, grep
finds it immediately. TOML allowlists rot silently.

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

## 6. CI + local wiring

### 6.1 New crate

`xtask/` as a `[workspace.members]` entry, `publish = false`, single binary.
Dependencies: `ignore`, `regex`, `toml`, `anyhow`, `serde`, `serde_json`.
`cargo xtask` convention pattern (see
[matklad/cargo-xtask](https://github.com/matklad/cargo-xtask)).

Entry points:

- `cargo xtask lint` — runs both checks, exits non-zero on any violation.
- `cargo xtask lint --only component-size`
- `cargo xtask lint --only allow-ban`
- `cargo xtask lint --json` — machine-readable output (for editor plugins).
- `cargo xtask lint --baseline write` — write current violations to
  `xtask/baseline.json` (see section 7).
- `cargo xtask lint --baseline check` — fail only on **new** violations vs.
  the baseline.

### 6.2 Cargo alias

`.cargo/config.toml`:

```toml
[alias]
xtask = "run --package xtask --"
lint-local = "xtask lint --baseline check"
```

So developers type `cargo lint-local` before pushing and get only their own
regressions, not the existing 45+ debt.

### 6.3 GitHub Actions

Add a step to `.github/workflows/lint-test.yml` in the `lint` job, after the
existing `cargo clippy` step:

```yaml
      - name: Custom lints (component size, allow ban)
        run: cargo xtask lint --baseline check
```

For the PR that first lands this tooling, CI runs with `--baseline write` and
commits `xtask/baseline.json` in the same PR so main stays green.

### 6.4 Pre-commit hook (optional, opt-in per developer)

`scripts/install-hooks.sh` registers a `.jj/hooks/pre-push` (or
`.git/hooks/pre-commit`) that runs `cargo xtask lint --baseline check` on
staged files only. Not mandated — CI is the enforcement point.

### 6.5 Interaction with existing `cargo clippy`

- `clippy::too_many_lines` stays on in `cranky.toml`. It catches non-component
  functions (which this lint does not scan) at 100 lines, and serves as a
  defense-in-depth against the xtask lint being disabled.
- The xtask `component-size` check exists because `clippy::too_many_lines`
  doesn't differentiate components from arbitrary helper fns and doesn't
  produce extraction-suggestion output tuned to Dioxus patterns.

---

## 7. Rollout + ratchet

Day 1 (warn-phase, single PR):

1. Land the `xtask/` crate and the CI step.
2. Run `cargo xtask lint --baseline write` → commit `xtask/baseline.json` with
   the existing 45 `#[allow(dead_code)]` + 2 `unused_variables` + 15
   `clippy::*` + 29 oversize components.
3. CI runs in `--baseline check` mode: new violations fail, existing
   violations pass through.

Week 1-4 (cleanup waves, one PR each):

| Wave | Target | PRs | Owner guess |
|------|--------|-----|-------------|
| 1 | Delete genuinely dead code in `clients/hackernews/*` (reduce 5 hits) | 1 | whoever owns HN |
| 2 | Fix or feature-gate `clients/forgejo/src/{lib,mapping}.rs` and parallel in github | 1 | forge-backend team |
| 3 | Convert `servers/test-*/routes.rs` stubs to `#[cfg_attr(…, allow(dead_code))]` under a feature flag, or wire the routes | 1-2 | test-server maintainers |
| 4 | Refactor the tier-1/2 oversize components (7 files, see audit checklist) | 3-5 | UI team |
| 5 | Tier-3/4 monsters (`FavoriteServerIcon`, `ChatView`) | dedicated sprint | |

After each wave, re-run `cargo xtask lint --baseline write` and commit the
shrunken baseline. The JSON is the ratchet — it only ever decreases.

Hard deadline: baseline must be **empty** before the 1.0 release. Until then,
`--baseline check` keeps regression pressure on without blocking feature work.

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

**Shared infrastructure opportunity:** if either plan spins up a
`crates/poly-macros/` crate for proc-macros, the source-walking logic
(`ignore` + per-file AST-lite) is worth lifting into a small
`crates/poly-lintlib/` crate that both `xtask` and any future per-crate
build-script lints can consume. Not required for day 1; note for later.

---

## 9. Open questions

1. Should the `#[allow(...)]` ban also cover `#[deny(…)]` override attrs like
   `#[forbid(dead_code)]`? (Leaning no — those tighten, not loosen.)
2. `clippy.toml`'s 100-line threshold currently applies to **all** fns, not
   just components. Is that intentional, or do we want a separate per-
   component vs per-function threshold? (Checklist wording says "100 lines
   fills a standard terminal" — one threshold seems right.)
3. Should the baseline file live at `xtask/baseline.json` or at
   `.poly/lint-baseline.json`? User to pick — no technical preference.
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
