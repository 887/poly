# Slice E — Test Architecture & Cross-Cutting Build Tooling

> Survey of test infrastructure (`servers/test-*`, `clients/*/tests/`,
> `crates/lint-gate`, `tools/scripts/forbid-*.sh`, `crates/core/build.rs`,
> `TEST_HARNESS.md`) for SOLID-aligned dev-loop wins.
> **Investigation only** — no refactors performed.

Methodology: enumerated all test infrastructure files, counted shared
patterns via grep, sampled three representative test backends end-to-end
(`test-discord`, `test-matrix`, `test-stoat`), audited the lint-gate +
shell-script lint surfaces against each other, and walked the
`baked_locales.rs` regen cycle.

Date evidence: 713 commits in the last 60 days; `locales/en/main.ftl`
touched in 81 of them (≈11.4 % of all commits).

---

## E.1 — Top 5 dev-loop wins (ranked by ROI)

### Win #1 — Extract a `TestServerHarness` runtime shim ⇒ kill ~270 LOC of duplicated `main.rs` + `seed/reset/reseed` boilerplate

**Pain today (measurable):**
- 9 `main.rs` files (`servers/test-{matrix,stoat,teams,lemmy,github,
  forgejo,discord,hackernews,reddit}/src/main.rs`) are byte-for-byte
  identical except for the type names `XState` and `poly_test_X`. Sample
  diff: `test-matrix/src/main.rs` vs `test-stoat/src/main.rs` differ on
  exactly 4 tokens. Each is ~40-48 lines, so ~360 LOC of pure
  copy-paste.
- 8 `state.rs` files (test-{discord, matrix, stoat, teams, lemmy,
  github, forgejo, hackernews}) hand-roll the same triplet:
  ```rust
  pub fn seed(&self) { if !self.users.is_empty() { return } ... }
  pub fn reset(&self) { self.auth.clear(); self.users.clear(); ... }
  pub fn reseed(&self) { self.reset(); self.seed(); }
  ```
  Same with the matching `routes::seed/reset/reseed` HTTP handlers (3
  identical handlers × 8 = 24 functions, each 3 lines).
- Per-backend `lib.rs` repeats the same `#![allow(...)]` block and the
  same `.layer(middleware::from_fn_with_state(... header_inspect_middleware))
  .layer(CorsLayer::very_permissive())` chain (8× verbatim).

**Proposed change:** introduce a `BackendHarness` trait in
`servers/test-common`:

```rust
pub trait BackendHarness: Sized + Send + Sync + 'static {
    const BACKEND: &'static str;            // "matrix", "stoat", …
    const DEFAULT_PORT: u16;
    fn new(auth: AuthState) -> Self;
    fn seed(&self);                          // one-shot, idempotent
    fn reset(&self);                         // user-defined wipe
    fn router(state: Arc<Self>) -> Router;   // backend-specific routes
}
```

Then `poly_test_common::run_backend::<MatrixState>().await` does all of
binding, AuthState load, seed-on-flag, graceful-shutdown, and adds
`/seed`, `/reset`, `/reseed` automatically (they call `state.reset()` /
`state.seed()`, never duplicated again). Each `main.rs` collapses to:

```rust
fn main() -> anyhow::Result<()> { poly_test_common::run::<MatrixState>() }
```

**Effort:** ~1 day. Touches `test-common/src/lib.rs` (+~80 LOC for the
harness + auto-mounted lifecycle routes), all 9 `main.rs` (-95 %), all
8 `state.rs` (-9 LOC each: drop `pub fn reseed`), and 8 `routes.rs`
(-9 LOC each: drop the seed/reset/reseed handlers + lifecycle route
bindings in `lib.rs`).

**Follow-on benefits:**
- SOLID: Single Responsibility (each `state.rs` owns only its data,
  not the lifecycle protocol); Open/Closed (adding test-bluesky needs
  zero protocol code, only `impl BackendHarness for BlueskyState`).
- A future `--bind-port-file <path>` or `--metrics-port` flag becomes a
  one-line edit in `test-common`, not 9 PRs.
- `poly-test-runner` can call `Backend::router()` directly and become
  in-process (drop the `cargo run -p` spawn loop, ~50 LOC win, ~5 s
  faster cold start).

---

### Win #2 — Split `baked_locales.rs` per locale ⇒ stop 11 % of all PRs touching the same 4487-line file

**Pain today (measurable):**
- `crates/core/src/i18n/baked_locales.rs` is **4487 lines**, regenerated
  on every build, contains all 4 locales (`FTL_EN`, `FTL_DE`, `FTL_ES`,
  `FTL_FR`) inline as raw-string consts.
- `locales/en/main.ftl` was touched in **81 of the last 713 commits
  (11.4 %)**. Visible duplicate-shaped commits in the log
  ("Phase D — RegisterLink + AddAccountNav integration + FTL" appears
  4× as 5a6488e1, 52b77139, 40d5d0fc, dac4ec23) suggest at least some
  of these are merge-conflict re-do's after rebase.
- The generated file is **ignored from git** (no commits in
  `git log -- crates/core/src/i18n/baked_locales.rs`), but the build
  script writes it on every change to any `locales/*/main.ftl`,
  triggering full `i18n` mod recompile every time. Single-locale FTL
  edit = recompile of all four locale const initialisers.

**Proposed change:** in `crates/core/build.rs:84-109`, write 4 separate
files `baked_locales_{en,de,es,fr}.rs` each with one `pub(super) const
FTL_X: &str = r#"..."#;`. Update `i18n/mod.rs` (or
`i18n/baked_locales.rs` reduced to a re-export shim) accordingly.

**Effort:** ~30 min. Build script change is a 6-line patch; module
declarations a 5-line patch. Zero behaviour change.

**Follow-on benefits:**
- Editing `locales/de/main.ftl` only invalidates `baked_locales_de.rs`
  → only the German const recompiles (incremental wins ~3-5 s on a
  typical poly-core check, given the file is the largest in
  `crates/core/src/`).
- PR conflicts: an English-only PR and a German-only PR no longer
  contend for the same generated file (in the rare cases where the
  generated file *is* committed, e.g. somebody's branch).
- SOLID: Single Responsibility per file. Each generated module owns one
  locale.
- Optional further split: per-domain FTL bundles (chat, settings,
  agent, …) wedge into the same scheme. Defer until pain is measured.

---

### Win #3 — Unify `tools/scripts/forbid-*.sh` regex lints into the `lint-gate` build script

**Pain today (measurable):**
- 10 shell scripts in `tools/scripts/forbid-*.sh`, total **1680 LOC of
  bash regex** (sizes range 16-256 lines). Each reimplements: workspace
  root resolution, allowlist parsing, inline-allowlist parsing,
  per-file scan loop, ANSI output, exit code semantics. The
  `forbid-cross-persona-memory.sh` and `forbid-effect-self-write.sh`
  scripts share ~80 % of their boilerplate but no code.
- Parallel infrastructure:
  - `crates/lint-gate/build.rs` runs **9 Rust scanners**
    (allow_ban, action_enum_coverage, action_id_naming,
    context_menu_coverage, custom_block_usage,
    forbid_backend_slug_match, ftl_label_key_coverage,
    nav_push_ban, route_graph, ui_action_coverage).
  - `tools/lints/poly-lints/` has **2 dylint lints**
    (raw_signal_write, use_effect_spawn_cycle) — duplicating the
    intent of `forbid-signal-write.sh` and
    `forbid-use-effect-spawn-cycle.sh`.
  - `tools/scripts/forbid-*.sh` runs **10 bash scanners**, of which 2
    (`forbid-signal-write.sh`,
    `forbid-use-effect-spawn-cycle.sh`) are
    triplicate-implemented (lint-gate could absorb them too;
    currently only dylint and bash do).
- Net: same intent expressed in 3 different toolchains, run in 3
  different CI steps. CI `.github/workflows/lint-test.yml` runs
  cargo-clippy, then 10 bash scripts, then dylint — 3 cold-cache
  workspace builds.

**Proposed change:** port each `forbid-*.sh` into a
`crates/lint-gate/build/forbid_<name>.rs` module (the existing
scanners are the template — see `forbid_backend_slug_match.rs`, only
137 LOC). Allowlist files stay verbatim (parsed by a shared
`allowlist::Loader` helper in `lint-gate/build/allowlist.rs`). Drop
the bash scripts after a parity-CI run. Drop the duplicate dylint
crate or keep it as a "deeper analysis" CI step that runs only on
`main`.

**Effort:** ~2-3 days. Each script port is ~half a day; the shared
allowlist loader is ~3 hours. The win is mostly recouped by deleting
1 680 LOC of bash + 2 maintained-duplicate-lint paths.

**Follow-on benefits:**
- Single CI step (`cargo check --workspace`) gates all 19 lints
  (currently 9 lint-gate + 10 bash + 2 dylint = 21, but with 2
  duplicates).
- One canonical inline-allowlist syntax (currently
  `// poly-lint: allow <name> — <reason>` is bash-script convention,
  while lint-gate scanners don't read inline allowlists at all — see
  `allow_ban.rs` which has its own `// lint-allow-unused:` syntax).
- SOLID: Open/Closed — adding a new lint becomes "drop a `.rs` module
  in `build/`", not "write a new bash script + register in CI yaml +
  optionally write a dylint mirror".
- Adding rule × adding file = O(rules + files), not the current
  O(rules × files) where each bash script re-walks the workspace.

---

### Win #4 — Hoist inline `mod tests` blocks out of large UI files in `crates/core`

**Pain today (measurable):**
- 54 `crates/core/src/**/*.rs` files contain inline `#[cfg(test)] mod
  tests`. These trigger a full re-typecheck of the host module every
  time a test is touched — and several of the host modules are giant:
  - `plugin_admin.rs` 873 LOC
  - `account_restore.rs` 654 LOC
  - `i18n.rs` 628 LOC
  - `state/batched_signal.rs` 608 LOC
  - `ui/settings/plugin_settings.rs` 603 LOC
- `crates/core/tests/` directory **does not exist** — there is no
  integration-test split today. Every test lives inline.
- Editing a tiny `mod tests` change forces re-monomorphisation of the
  module's downstream consumers (notably anything that imports `i18n`
  or `state::batched_signal` — i.e. most of the codebase).

**Proposed change:** move pure-logic tests (the ones that don't need
crate-internal access) into `crates/core/tests/<topic>.rs` integration
tests. Keep tests that genuinely need `pub(crate)` access inline.

A first pass: `i18n.rs` tests (lines after #542) are mostly
key-presence / formatting tests that only consume the `t!` macro and
public API → move to `crates/core/tests/i18n_keys.rs`. Same for
`batched_signal.rs` tests (the `set_if_changed` semantics are testable
through public API).

**Effort:** ~1 day for the top-5 hot files. Lower priority than
Wins #1-3.

**Follow-on benefits:**
- `cargo test -p poly-core --tests` recompiles tests independently of
  the host module. ~2-3 s saved per iteration on the hot files.
- Forces SOLID: if a test needs `pub(crate)` access to test internal
  helpers, it's a sign the helper itself wants to be pulled into a
  smaller submodule whose contract is publicly testable.

---

### Win #5 — Generalise `clients/*/src/lib.rs` FTL `include_str!` boilerplate

**Pain today (measurable):**
- Identical code in 7 client crates:
  ```rust
  match locale {
      "en" => include_str!("../locales/en/plugin.ftl").to_string(),
      // ...
  }
  ```
  Files: `clients/{discord,lemmy,teams,forgejo,hackernews,reddit,
  matrix,stoat,demo,server-client,github}/src/lib.rs`. Each uses the
  same 4-arm match (en/de/es/fr) — except `reddit` which only ships
  `en` (see lines 47-49 of `clients/reddit/src/lib.rs`).
- A 12th plugin (e.g. test-bluesky) would copy the same 12-line block.
- New locale (e.g. `ja`) requires editing all 11 lib.rs files.

**Proposed change:** in `crates/poly-ui-macros` (which already exists
as a workspace dep — see `clients/discord/Cargo.toml:38`), expose
`bundle_locales!()` macro that walks `concat!(env!("CARGO_MANIFEST_DIR"),
"/locales")` at compile time and emits the match arm wholesale. Or, if
proc-macro filesystem access is too magical for taste, a `const fn` +
`include_str!` table generated by a tiny `build.rs` per client.

**Effort:** ~half a day for the macro; ~10 minutes per client to
adopt.

**Follow-on benefits:**
- New locale = edit 11 directories, not 11 source files.
- SOLID: Dependency Inversion. The plugins depend on a *macro
  contract* "give me my bundle for `locale`", not on a hardcoded
  match-on-string (the very pattern banned in
  `forbid_backend_slug_match` for backend slugs!).

---

## E.2 — Lint-gate audit

### Lints currently enforced in `crates/lint-gate/build.rs` (9)

| # | Module                        | LOC | One-liner |
|---|-------------------------------|-----|-----------|
| 1 | `allow_ban.rs`                | 142 | Bans the 17 named lints from `#[allow(...)]` outside `tests/`, `examples/`, `mcp/`, `servers/test-*`. Honours `// lint-allow-unused: <reason>` markers. |
| 2 | `action_enum_coverage.rs`     | 242 | Every `#[component]` must carry `#[ui_action(...)]`; `(None)` components forbid non-noop event handlers. |
| 3 | `action_id_naming.rs`         | 167 | `MenuItem`/`ComposerButton`/`SidebarItem` `id:` fields must be kebab-case. |
| 4 | `context_menu_coverage.rs`    | 91  | Every right-click context-menu site must declare its menu items via the registry (covered by `plan-context-menu-quality-control.md`). |
| 5 | `custom_block_usage.rs`       | 113 | `CustomBlock { … }` literal usage must stay below the per-plugin threshold (Pack G P40). |
| 6 | `forbid_backend_slug_match.rs`| 137 | Bans `match X.as_str() { "discord" => … }` ladders in `crates/core/src/ui/`. |
| 7 | `ftl_label_key_coverage.rs`   | 186 | `label_key:` strings must resolve to a real FTL message in the plugin's `en` bundle. |
| 8 | `nav_push_ban.rs`             | 56  | Bans `nav.push(...)` direct calls; route enums must be invoked via the typed wrapper. |
| 9 | `route_graph.rs`              | 301 | Every `#[route]` variant must be reachable from an `entry_point` or `programmatic` connected route. **Never grandfathered.** |
| + | `ui_action_coverage.rs`       | 243 | Empty event handlers, empty `rsx! {}` bodies, malformed `ui_noop!` calls. (Two scanners: this + #2 partly overlap on the `#[ui_action(None)]` empty-handler check.) |

### Lints currently enforced in `tools/scripts/forbid-*.sh` (10)

| # | Script                               | LOC | One-liner | Duplicates? |
|---|--------------------------------------|-----|-----------|-------------|
| a | `forbid-cross-persona-memory.sh`     | 150 | Persona-scoped DML must include `persona_slug` binding. | No |
| b | `forbid-effect-self-write.sh`        | 221 | `use_effect` body that reads + writes same signal without `_if_changed`. | No |
| c | `forbid-long-read-guard.sh`          | 218 | `Signal::read()` guard live ≥30 lines across a `.batch()/.write()` of the same signal. | No |
| d | `forbid-raw-backend-read.sh`         | 128 | Raw `backend.read().await` without `read_with_timeout`. | No |
| e | `forbid-render-time-read.sh`         | 147 | Render-time `.read()` that triggers re-render loops (CLAUDE.md hang #7). | No |
| f | `forbid-signal-write.sh`             | 204 | Bans raw `Signal::write()` in `crates/core/src/ui/`. | **YES** — duplicated by `tools/lints/poly-lints/src/raw_signal_write.rs` (dylint). |
| g | `forbid-stale-effect-capture.sh`     | 130 | Raw `use_effect(move ||)` with non-Signal capture. | No |
| h | `forbid-ui-only-persona-action.sh`   | 16  | Stub (Phase Q.3 placeholder). | Stub — delete or finish. |
| i | `forbid-unaudited-persona-tool.sh`   | 210 | `handle_meta_persona_*` must call `audit()`. | No |
| j | `forbid-use-effect-spawn-cycle.sh`   | 256 | Hang class #3: `use_effect` + `spawn(async { signal.batch(…) })`. | **YES** — duplicated by `tools/lints/poly-lints/src/use_effect_spawn_cycle.rs` (dylint). |

### Findings

- **Three implementations for the same lint.** `forbid-signal-write.sh`
  + `poly-lints/src/raw_signal_write.rs` + (potential lint-gate
  scanner) gate the same hang class #1 invariant. Same for
  `forbid-use-effect-spawn-cycle.sh` + `poly-lints/src/use_effect_spawn_cycle.rs`.
  CI runs both; dylint is `continue-on-error: true` so it's
  decorative. Pick one home.
- **No shared allowlist parser.** Each bash script re-implements the
  `# comments`, blank-line, and `path:line` mini-DSL. Bug fix in one
  → does *not* propagate.
- **`forbid-ui-only-persona-action.sh` is a 16-line stub.** Delete it
  or finish Phase Q.3.
- **`allow_ban.rs` skip list (`/tests/`, `/examples/`,
  `/servers/test-`, `/plugin-host-tests/`, `/mcp/`) is hardcoded
  string-contains** (`crates/lint-gate/build/allow_ban.rs:39-46`).
  Adding a 9th test-server crate is a no-op (good), but a new path
  pattern (e.g. `bench/`) means editing the scanner. Convert to a
  config table read from `crates/lint-gate/lint-config.toml` for
  easier opening for cross-cutting changes.
- **`baseline.json` is empty** (`crates/lint-gate/baseline.json` is 2
  bytes: `{"violations": []}`). The grandfather mechanism is
  load-bearing in the design but not actually used right now.
  Confirm: either keep as a safety net (cheap) or document it as
  unused and consider removal.
- **`route_graph.rs` is 301 LOC** of route-attribute parsing — by far
  the largest scanner. Its `parse_variants` is duplicated *verbatim*
  in `lib.rs:scanner_tests` (lines 350-446) "to make tests work
  without reaching into the build/ module path". This pattern repeats
  for **every** scanner — `lib.rs` is now 1 416 LOC almost entirely
  composed of mirrored test copies of the scanners. **Real** SOLID
  smell: the scanner logic should live in a normal `lints` library
  crate, with `build.rs` and tests both depending on it.

### Recommended consolidation

1. Create `crates/lint-gate-rules` (a normal lib crate, not a
   build-script-only one). Move every `build/<rule>.rs` into
   `lint-gate-rules/src/<rule>.rs`. `build.rs` becomes a thin driver
   that calls `lint_gate_rules::all_rules(walker)`.
2. Tests live next to rules in `lint-gate-rules/src/<rule>.rs` with
   `#[cfg(test)] mod tests` — drop the 1 416 LOC mirror in
   `lint-gate/src/lib.rs` (kill ~80 % of it; keep the public re-exports
   for any external consumer).
3. Move all 8 keep-able `forbid-*.sh` scripts into
   `lint-gate-rules/src/<name>.rs` (the existing scanners are the
   template). Allowlist parser shared.
4. Retire the dylint `tools/lints/poly-lints/` crate or reframe it as
   a "deep HIR analysis on `main` only" lane. Don't duplicate.

Estimated reduction: 1 416 (lint-gate/src/lib.rs) + 1 680
(forbid-*.sh) + ~400 (dylint duplicates) ≈ **3 500 LOC retired**.

---

## E.3 — Test-server commonality

### Patterns repeated across `servers/test-*/`

Per-crate sizes (LOC, src/ only):

| Crate | lib.rs | main.rs | routes.rs | state.rs | total |
|-------|--------|---------|-----------|----------|-------|
| test-discord    | 118 | 42 | 1535 | 798 | 2493 |
| test-matrix     |  83 | 40 | 1123 | 425 | 1671 |
| test-stoat      |  83 | 41 | 1107 | 473 | 1704 |
| test-teams      | 101 | 38 |  819 | 282 | 1240 |
| test-lemmy      |  82 | 48 | 1385 | 328 | 1843 |
| test-github     |  90 | 47 |  469 | 485 | 1091 |
| test-forgejo    |  84 | 47 |  455 | 377 |  963 |
| test-reddit     |  71 | 27 |  680 | 166 |  944 |
| test-hackernews |  -  | -  |  363+279 | -  |  642 |

### Repeated patterns (with sites)

1. **Identical `main.rs` structure** — 9× verbatim except for type
   names. See Win #1.
2. **`pub fn seed/reset/reseed` triple** in 8× `state.rs` —
   `seed()` always opens with `if !self.users.is_empty() { return }`,
   `reseed()` is always `self.reset(); self.seed();`. See Win #1.
3. **`pub async fn seed/reset/reseed` HTTP handlers** in 8× `routes.rs`
   — always 3 lines each, always `Json(json!({"ok": true}))`.
4. **Per-crate `#![allow(clippy::unwrap_used, clippy::expect_used,
   clippy::panic, clippy::indexing_slicing, dead_code)]`** in every
   `lib.rs` (8× verbatim, +1 with extra `unused_variables`,
   +1 with extra `unused_imports`). Move to a `[lints]` block in
   workspace Cargo.toml or, better, a single `pub use` of an
   `__allow_test_crate!()` macro from `test-common`.
5. **`.layer(middleware::from_fn_with_state(Arc::clone(&inspect),
   header_inspect_middleware)) .layer(CorsLayer::very_permissive())`**
   chain at the bottom of every `lib.rs::router()` function (8×).
6. **Avatar serving** is already shared
   (`poly_test_common::serve_animal`), but each crate still wires its
   own route — `routes::serve_avatar` is a 3-line shim in each. Could
   be one of the auto-mounted routes from `BackendHarness`.
7. **`POST /test/auth/token` test-only easy-signin** in 5 crates
   (matrix, stoat, discord, lemmy, github, forgejo, teams). Each
   re-implements: parse JSON body → look up user → mint token via
   `AuthState::create_token` → return `{"token": ...}`. Promotable
   into `test-common` as a generic handler that takes a user-id
   resolver closure.

### Concrete extraction proposal

`test-common` should expose:

```rust
pub trait BackendHarness: Sized + Send + Sync + 'static {
    const BACKEND: &'static str;
    const DEFAULT_PORT: u16;
    fn new(auth: AuthState) -> Self;
    fn seed(&self);
    fn reset(&self);
    fn router(state: Arc<Self>) -> Router;
}

/// Auto-mounts `/seed`, `/reset`, `/reseed`, `/health`,
/// `/test/inspect/last-headers`, plus the inspect+CORS layers, on top
/// of the backend's own router.
pub fn run<H: BackendHarness>() -> anyhow::Result<()>;

/// Generic test-only signin handler. Each backend wires it once with
/// its own user-id resolver (closure).
pub fn test_auth_token_handler<F>(resolve: F) -> impl IntoResponse
where F: Fn(&str) -> Option<String> + Send + Sync + 'static;

/// Macro to apply the standard test-crate lint allow-set.
#[macro_export]
macro_rules! test_crate_allows { () => { ... } }
```

Each `state.rs` keeps its data struct + private helpers but drops the
`reseed()` method (auto-implemented). Each `routes.rs` drops the
`seed/reset/reseed` handlers. Each `lib.rs` drops the lifecycle
routes + the layer chain. Each `main.rs` collapses to one line.

**Stays per-server (intentionally NOT shared):**
- The actual API routes (Matrix's `/_matrix/client/v3/...`,
  Stoat's `/auth/session/...`, etc.) — these are the whole point of
  the mock servers; sharing would defeat fidelity to each
  upstream's API contract.
- The data model (`MatrixState::rooms`, `DiscordState::guilds`, etc.)
  — backend-specific schemas can't be unified without losing
  fidelity.
- Backend-specific event enums — already correctly generic over the
  shared `EventBus<T>`.
- Wire format helpers (`serde_json` shapes per endpoint) — by design,
  each mock mirrors a different upstream wire protocol.

---

## E.4 — Things to LEAVE alone

### `clients/reddit/tests/fixtures/` shared with `servers/test-reddit/src/routes.rs`

Currently `test-reddit` does `include_str!("../../../clients/reddit/tests/fixtures/foo.html")`
to share HTML fixtures with the reddit parser tests. This *looks* like
a layering smell (server reaching into a sibling client's tests/), but
it's deliberate and correct: the captured HTML files (real reddit
responses) are the ground truth that *both* the mock server and the
parser must agree on. Splitting them into `clients/reddit/fixtures/`
and dual-using would just rename the dependency without removing it.
Keep the cross-crate `include_str!` — and document the convention in
`README.md` so future test-{lemmy,teams,…} backends do the same when
they need fixture parity.

(One small follow-up: add a comment at
`clients/reddit/tests/fixtures/README.md` noting that
`servers/test-reddit/src/routes.rs` *also* includes these files, so
edits must be checked against both sites. Currently nothing flags it.)

### Per-backend `state.rs` schemas

Don't try to unify `DiscordState::guilds: DashMap<String, Guild>` with
`MatrixState::rooms: DashMap<String, Room>` — they model different
upstream protocols. The 8 separate state structs are doing the right
thing.

### `tests/e2e/persona-multi-agent.sh`

Bash, not Rust, but that's right — it orchestrates real subprocess
chains (chat-mcp, mock-claude, poly-web). Porting to Rust would gain
nothing and lose familiarity for ops/CI. The Makefile wrapper
(`make e2e-personas`) is a tidy adapter.

### `crates/lint-gate/build/route_graph.rs` route-graph never-grandfather rule

The "route_graph violations are never grandfathered" carve-out at
`crates/lint-gate/build.rs:112-114` is intentional — orphan routes are
runtime dead code that compiles cleanly but is statically unreachable.
Allowing baseline grandfathering would defeat the lint. Keep the
explicit early-error path.

### Per-client `#[lib] crate-type = ["cdylib", "rlib"]`

Each `clients/*/Cargo.toml` declares the dual crate-type so the same
source can build as both a native lib (loaded by `apps/*`) and a WASM
plugin component (loaded by `poly-plugin-host`). The cfg-gating around
`twilight-model`, `tokio-tungstenite`, `getrandom04-wasm` etc. is
correctly tagged with feature gates and `[target.'cfg(...)']`
sections. It looks busy but it's load-bearing — touching it without a
matrix-build CI run is risky.

### `TEST_HARNESS.md` haiku-subagent loop

The 6-step harness (cargo check → clippy → wasm build → unit tests →
poly-web MCP → persona e2e) is correctly minimal: one verification
loop per concern, gated by what the change touches. The
"`cargo test --workspace` does not work because the repo mixes native
and WASM targets" comment at line 54 is a real constraint, not bloat.
Don't try to consolidate to a single `cargo test` — the
manual-listing of crates IS the workaround.

The only YAGNI bloat candidate: step 6's `mcp-to-ui-live-update`
fallback to `two-personas-handoff` — Phase E.3 is "not yet shipped" per
the comment. When E.3 ships, drop the fallback. Until then it's not
hurting anything.

---

## Appendix A — file-evidence index (absolute paths)

- `/home/laragana/workspcacemsg/servers/test-common/src/{lib,auth,avatars,broadcast,cli,inspect,server}.rs`
  (745 LOC, the existing shared base)
- `/home/laragana/workspcacemsg/servers/test-{matrix,stoat,teams,lemmy,
  discord,github,forgejo,hackernews,reddit}/src/main.rs` (9× duplicate
  shells — Win #1 target)
- `/home/laragana/workspcacemsg/crates/lint-gate/build.rs` (157 LOC
  driver) and `crates/lint-gate/build/*.rs` (1799 LOC of scanners)
- `/home/laragana/workspcacemsg/crates/lint-gate/src/lib.rs` (1416 LOC
  — 80 % is duplicated test copies of scanners)
- `/home/laragana/workspcacemsg/crates/lint-gate/baseline.json`
  (currently empty: `{"violations": []}`)
- `/home/laragana/workspcacemsg/tools/scripts/forbid-*.sh` (10 scripts,
  1680 LOC bash)
- `/home/laragana/workspcacemsg/tools/lints/poly-lints/src/{raw_signal_write,use_effect_spawn_cycle}.rs`
  (dylint duplicates of two bash scripts)
- `/home/laragana/workspcacemsg/crates/core/src/i18n/baked_locales.rs`
  (4487 LOC, generated; Win #2 target)
- `/home/laragana/workspcacemsg/crates/core/build.rs` (lines 71-109
  drive the bake; Win #2 patch site)
- `/home/laragana/workspcacemsg/locales/{en,de,es,fr}/main.ftl`
  (1346 / 1044 / 1043 / 1043 LOC; en touched in 81/713 recent commits)
- `/home/laragana/workspcacemsg/clients/{discord,lemmy,teams,forgejo,
  hackernews,reddit,matrix,stoat,demo,server-client,github}/src/lib.rs`
  (Win #5 target — duplicate FTL match arm)
- `/home/laragana/workspcacemsg/clients/reddit/tests/fixtures/`
  (12 HTML/JSON files, double-included by `servers/test-reddit/src/
  routes.rs` — leave alone, see E.4)
- `/home/laragana/workspcacemsg/TEST_HARNESS.md` (162 LOC, leave
  alone, see E.4)
- `/home/laragana/workspcacemsg/.github/workflows/lint-test.yml`
  (259 LOC, currently runs 10 forbid-*.sh + lint-gate + dylint as
  3 separate CI lanes)
