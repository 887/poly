# Plan — Connected Routes Static Check

> **Created:** 2026-04-16
> **Status:** ✅ DONE (2026-04-17) — all items shipped. §5.3.2 route-graph violations excluded from regen-baseline; §5.3.3 nav_push_ban scanner live; §7.1 10 unit tests in connected.rs; §7.2 5 trybuild compile-fail fixtures + .stderr snapshots; §7.3 5 scanner unit tests in lint-gate/src/lib.rs; §7.4 runtime coverage counter in sync_route_to_app_state; §7.5 haiku harness run. `cargo check --workspace` + `cargo clippy` + WASM target all clean.
> **Scope:** cross-cutting — `crates/core/src/ui/routes.rs`, every link/button/navigator callsite under `crates/core/src/ui/`, the shared `crates/ui-macros/` proc-macro crate, and the shared `crates/lint-gate/build.rs` graph checker
> **Goal:** `cargo check`-native reachability check. Every `Route` variant must be reached by **either** (a) at least one `Link { to: Route::X }` / `nav!(Route::X)` callsite — identity carried by Rust's own type system on `Route::X`, nothing stringly-typed — **or** (b) a ZST `ProgrammaticProducer` impl naming the route as its target. Orphan routes and routes unreachable from `entry_point` surface as `cargo::error=` on plain `cargo check`. No `via` label dictionary, no `#[test]` to skip.

---

## 1. Current state

Audited 2026-04-16:

- **Router:** Dioxus 0.7.3 with the `router` feature (single workspace dep in root `Cargo.toml:59`). No separate `dioxus-router` entry.
- **Route enum:** exactly one `#[derive(Routable)]` in the workspace at `crates/core/src/ui/routes.rs:138`. It has ~27 variants (`Root`, `DmsHome`, `ConversationSearchRoute`, `NewConversationRoute`, `DmChat`, four `DmPending*Call` variants, `DmMediaViewerRoute`, `CreateChannelRoute`, `ServerChat`, `ServerMediaViewerRoute`, `ForumPostRoute`, `CreateForumPostRoute`, `ForumSearchRoute`, `ForumCommentsRoute`, `ServerHome`, `FriendsRoute`, `NotificationsRoute`, `SavedItemsRoute`, `ServerOverviewRoute`, `SettingsRoute`, `SettingsSectionRoute`, `SearchRoute`, `AccountSearchRoute`, `AccountSettingsRoute`, `CreateServerRoute`, `ServerSettingsRoute`, `ServerSettingsSectionRoute`, `SignupPicker`, `ClientSignup`, `ReauthAccount`, `PageNotFound`). Nested under `MainLayout { DmsLayout, ServerLayout }` with an escape hatch for `SignupPicker` / `ClientSignup` / `ReauthAccount`.
- **Callsites:** 79 `navigator()` occurrences across 28 files; 115 total occurrences of `navigator()|.push(Route::|Link {|to: Route::` patterns across 29 files. Hotspots: `favorites_sidebar.rs` (8), `channel_list.rs` (23 combined), `chat_view.rs` (10), `signup/mod.rs` (8), `settings/mod.rs` (2). Typical shape: `navigator().push(Route::ServerChat { backend, instance_id, account_id, server_id, channel_id })`. Many also use `format!()` URL building for `account_last_routes` which bypass the enum.
- **No compile-time registry libs** in use — `linkme`, `inventory`, `#[distributed_slice]` do not appear anywhere in `Cargo.toml` or source.
- **Routing entry point:** router is assembled in `crates/core/src/ui/mod.rs` with `sync_route_to_app_state` called from `RouterConfig::on_update` (see `routes.rs:358`). That function is the single choke-point where every active `Route` value passes through at runtime — useful for a dev-only runtime coverage check that complements the static check.

Baseline for success: the plan must handle ~27 routes × ~115 callsites without requiring a 500-file edit up front (see §5 Migration).

---

## 1b. Mental model — this is a directed graph coverage problem

It is worth naming the shape explicitly so future work doesn't reinvent the vocabulary:

- **Nodes** — `Route` enum variants. `|V| ≈ 27`.
- **Edges** — one per link/button/`navigator().push(Route::X)` callsite. `|E| ≈ 115`. Directed. **Unlabeled** — the edge's identity is the pair `(source_module_path, target_variant)` captured by the `linkme`-registered macro expansion; there is no free-text `via` string and nothing that asks the author to invent one.
- **Root** — the `entry_point` variant. Exactly one; an error if more than one node carries that marker.
- **Programmatic edges** — routes reachable only via non-UI producers (redirect after signup, deep-link handler, push-notification open) are represented by ZST tag types implementing the sealed `ProgrammaticProducer` trait. Each such type names its `TARGET: RouteDiscriminant` at the type level; the `#[connected(programmatic<SignupCompletionLanding>)]` on the route side references the type, so a rename compile-errors at the declaration site. These edges still count toward `in_degree ≥ 1`.

What the check enforces, in graph terms:

| Check | Graph statement | Failure name |
|---|---|---|
| Every route is reachable | `in_degree(v) ≥ 1` for all `v ≠ root` | **orphan route** |
| Every `programmatic<T>()` names a real type | `T: ProgrammaticProducer` must exist and its `TARGET` must match the route | **unknown programmatic producer** (enforced by trait bound — compile error, not build.rs) |
| Every `Link/nav!` target is a real Route variant | `Route::X` must parse | **unknown route** (enforced by Rust type checking — compile error) |
| Graph is connected from root | every `v` is reachable from `root` via BFS over edges + programmatic producers | **unreachable component** |
| No dead-end routes (optional, warn) | `out_degree(v) == 0` flagged unless annotated `#[dead_end("reason")]` | **leaf warning** |

The primary guarantee the plan buys is: **the route graph is a single connected component rooted at `entry_point`.** Orphan routes are disconnected nodes. Because edges are unlabeled and typed at both ends, there is no "dangling half-edge" failure mode — link identity is carried by the `Route` enum itself and programmatic-producer identity is carried by a sealed trait.

`Route` is not a tree — several producers can target one route (e.g. `ServerChat` has many entries: sidebar channel-list, notifications, search, deep links). That is fine; the check is about **reachability**, not **uniqueness of paths**. Reducing the graph to a **spanning tree** rooted at `entry_point` is not a goal — it would imply one canonical path per route, which is wrong for a chat app where every route has many natural entrances.

What the plan does *not* enforce:

- **No cycle detection.** Cycles are normal (Settings ⇄ Server Settings sub-page). The check is agnostic to cycles.
- **No shortest-path analysis.** The graph's diameter doesn't matter.
- **No strongly-connected-component decomposition.** Listed here only to rule it out: we do not partition the graph and we do not care whether the graph is strongly connected, only weakly connected from `entry_point`.

Operationally, the build-script checker reads two `linkme` distributed slices: `LINK_CALLSITES` (populated by `Link { to: Route::X }` / `nav!(Route::X)` expansions) and `PROGRAMMATIC_EDGES` (populated by every `ProgrammaticProducer` impl). It runs BFS from `entry_point` over the union of both edge sets and emits `cargo::error=` for every `Route` variant not visited. No symmetric-difference pass — there are no labeled declarations to match against, only the set of actually-typed producers, and a typed producer that targets a nonexistent variant is already a rustc error before the build script runs.

---

## 2. The DSL — concrete syntax for both directions

Macros in a new `crates/ui-macros/` crate, re-exported from `crates/core` as `poly_ui::{connected, nav, ProgrammaticProducer}`. **Link identity is carried by the Rust type system, not by strings.**

### 2.1 On route/page declarations (incoming edges)

```rust
#[connected(
    // This route is reachable from ≥1 Link/nav! callsite in the workspace.
    // The build-script BFS proves it; the annotation is just the route-side
    // declaration that this is the expected story.
    linked,

    // Any non-clickable entry point is represented by a ZST tag type that
    // implements `ProgrammaticProducer`. Rename the type → compile error
    // at this site. No reason-string drift, because the reason lives on
    // the impl as `const REASON: &'static str`.
    programmatic<SignupCompletionLanding>,
    programmatic<SyncRouteRestoreFromAccountLastRoutes>,
)]
#[route("/:backend/:instance_id/:account_id/dms")]
DmsHome { backend: String, instance_id: String, account_id: String },
```

Three edge kinds:

| Kind | Syntax | Identity carrier | Use |
|------|--------|------------------|-----|
| `linked` | `linked` (bare) | The set of `Link { to: Route::X }` / `nav!(Route::X)` callsites in the workspace, discovered via the `LINK_CALLSITES` `linkme` slice. Author names nothing. | Normal buttons, links, menu items. |
| `entry_point` | `entry_point` (bare) | None — this is the BFS root. | The cold-start route. Exactly one variant may carry this. |
| `programmatic<T>` | `programmatic<SignupCompletionLanding>` | Rust type `T: ProgrammaticProducer<Target = Self>`. Rename breaks the build at this site and at the impl. | Automated redirects, default-landing, `on_update` replaces, external deep-links. |

A route with no `#[connected(...)]` is an orphan. A route with only `linked` when no callsite actually targets it is an orphan. Both cases → `cargo::error=E-ROUTE-002`.

### 2.2 The `ProgrammaticProducer` trait

```rust
// crates/ui-macros/src/programmatic.rs (re-exported from poly_ui)
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a declared ProgrammaticProducer",
    label = "impl ProgrammaticProducer<Target = Route::…> for this type",
    note = "ZST tag types are how automated/non-clickable route entries declare themselves"
)]
pub trait ProgrammaticProducer {
    type Target;                           // Route variant this producer lands on.
    const REASON: &'static str;            // Human-readable, shown in build.rs output.
    const SITE: &'static str = file!();    // Auto-captured; audit aid only.
}
```

Per programmatic producer, authors write a ZST + impl at the producer site:

```rust
// crates/core/src/ui/signup/mod.rs
pub struct SignupCompletionLanding;
impl ProgrammaticProducer for SignupCompletionLanding {
    type Target = routes::DmsHome;          // type-level route reference
    const REASON: &'static str = "redirect to user's DMs after signup completes";
}
```

A derive or small `declare_programmatic!` helper macro can cut the boilerplate to one line:

```rust
declare_programmatic!(SignupCompletionLanding -> routes::DmsHome
    = "redirect to user's DMs after signup completes");
```

Expected count: ~5–8 programmatic producers total in the current codebase (signup landing, account-restore fallback, deep-link handler, 401 redirect, push-notification opener). Small enough that each gets a real type.

### 2.3 On link/button/navigator callsites (outgoing edges)

Two flavors — the `Route::X` type is the identity; nothing else is required:

**RSX `Link` (Dioxus component):**

```rust
Link { to: Route::SettingsRoute, "Settings" }
```

No macro change required — we can't modify Dioxus's `Link`, but we don't need to. A companion `track_link!` or a proc-macro applied to the enclosing `#[component]` walks the RSX tree at expansion time, finds every `Link { to: Route::X }`, and emits a `#[linkme::distributed_slice(LINK_CALLSITES)]` entry with `(target = Route::X's discriminant, file, line)`. The user writes plain Dioxus; the macro does the bookkeeping.

**Imperative `navigator()`:** wrap through a declarative `nav!` macro.

```rust
nav!(Route::DmsHome {
    backend: backend.clone(),
    instance_id: instance_id.clone(),
    account_id: account_id.clone(),
});
```

`nav!` expands to `navigator().push(route)` unchanged — zero runtime cost — plus a `LINK_CALLSITES` registration whose payload is the variant discriminant.

Raw `navigator().push(Route::...)` outside `nav!` / `Link` is banned in Phase C (§5.3.3) so the lintgate build.rs sees every callsite.

### 2.4 Why no labels

The earlier draft of this plan required each edge to carry a free-text `via("surface:element")` string matched on both ends. Dropped because:

1. **Parallel source of truth.** Renaming the "favorites-sidebar" UI section would require updating ~8 `via` strings across unrelated files with no compile check.
2. **Agent gaming.** The most common repair for "unconsumed label" / "unknown label" errors is to copy-paste whatever string makes the build pass. The check would then be satisfied by a label set that doesn't mean anything.
3. **Type system already does it.** Rust's type checker already knows `Route::X` is a real variant; it already catches typos in `Link { to: Route::Xxx }` and `nav!(Route::Xxx)`. Adding a string next to the type is redundant.

The one thing labels gave us that types don't: **"which UI surface produces this edge"** for audit purposes. That use case is served by `git grep 'to: Route::X'` + `git grep 'nav!(Route::X'` — already done today, no new machinery needed.

---

## 3. The check — mechanism

**Chosen primary mechanism: `linkme` distributed slice populated by the proc-macros, read by the shared `crates/lint-gate/build.rs` from `plan-component-lints.md` §3.2. Graph-check errors emerge as `cargo::error=` lines on plain `cargo check` — no separate command, no `#[test]` to skip.** Fallback/complement: a `clippy`-style `dylint` crate for prettier error spans.

### 3.1 Why `linkme`, not `inventory`

- `inventory` relies on `ctor`, which requires `std::sync::Once` at startup — works on native but is **not reliable under `wasm32-unknown-unknown`** where constructors are batched by the WASM runtime and `ctor` has open issues. Poly ships to WASM as the primary target.
- `linkme` uses link-section attributes (`__DATA,__linkme` / `.linkme` / etc.) and does **not** require constructors. It has documented WASM support via `linkme = "0.3"` with `rustc 1.78+`. The slice is materialized at link time and readable from `fn main()` or a `build.rs`-invoked probe binary.
- `linkme` works across workspace crates provided the downstream crate depends on them — which `crates/core` already does for every UI module.

### 3.2 Why not pure `build.rs` + `syn` AST scan

Considered and rejected as primary, kept as optional reinforcement:

- Pro: no link-section magic, pure text scan, easy to debug.
- Con: must parse every `.rs` file in the workspace on every build, duplicates what the compiler already knows, brittle to `cfg` feature combinations (Poly's UI has heavy `#[cfg(feature = "...")]` per-backend gating under `ui/account/<backend>/`), and hard to keep in sync with macro expansion.
- Use case where it wins: a one-shot `cargo xtask routes-audit` that lists orphans outside the regular build loop — see §7.

### 3.3 Why not pure clippy lint

Clippy lints can flag individual callsites but cannot see the full graph across crates in one pass. A custom `dylint` pass can, but ships out-of-tree and is awkward in CI. Not used.

### 3.4 Pipeline

1. `#[connected(...)]` proc-macro on each `Route` variant → emits `#[linkme::distributed_slice(ROUTE_DECLARATIONS)] static: RouteDecl { variant_discriminant, has_linked_flag, programmatic_targets: &[...] }`. No string labels.
2. `nav!(Route::X { ... })` and the component-level RSX-walk proc-macro for `Link { to: Route::X }` → emit `#[linkme::distributed_slice(LINK_CALLSITES)] static: LinkCallsite { target_discriminant, file, line }`. Each `impl ProgrammaticProducer for T` expands to a `#[linkme::distributed_slice(PROGRAMMATIC_EDGES)] static: ProgrammaticEdge { target_discriminant, producer_type_name, reason }`.
3. **The graph assertion runs from `crates/lint-gate/build.rs`** — the same build script that plan-component-lints.md §3.2 uses for the allow-ban scan and plan-context-menu-quality-control.md §3.1.2 uses for decorator coverage. `lint-gate` depends on `ui-macros` so both slices are in scope; the script materializes `declared_edges` and `produced_edges` from the slices, computes symmetric difference and BFS from `entry_point`, and emits one `cargo::error=E-ROUTE-XXX: ...` line per violation (stabilized Rust 1.84). Because `cargo check` runs every `build.rs` before typechecking, the error surfaces automatically — rust-analyzer red-squiggles it on save, `cargo check` exits non-zero in CI, no agent can quietly skip it.
4. **One workspace walk, three checks.** `lint-gate`'s `build.rs` already walks `*.rs` for the allow-ban (plan 3 §3.2) and for `#[component]`-decorator coverage (plan 1 §3.1.2). The slice-level graph check bolts onto the same pass — negligible additional cost (slice read + hash-set diff). Clean-build cost stays in the 100–300 ms range even with all three lints active.
5. WASM-side note: `build.rs` runs on the host during every `cargo check --target wasm32-unknown-unknown`. The registry content is source-level (macros emit `&'static` data populated at compile time and linked per-target), so the host build sees everything the WASM build would have registered. No WASM-specific tooling required.

### 3.5 Error surface — type errors from rustc, graph errors from build.rs

Three classes of error, each surfacing through its natural channel:

1. **Unknown route variant** (`Link { to: Route::Xxx }` where `Xxx` isn't a variant) — caught by rustc at the callsite with the usual span. Nothing to add.
2. **Bad `programmatic<T>`** (type doesn't exist or doesn't impl `ProgrammaticProducer` with the expected `Target`) — caught by the trait bound in the `#[connected]` expansion, with a `#[diagnostic::on_unimplemented]` message on the trait (§2.2). Per-`#[connected(...)]` span, not build.rs stderr.
3. **Orphan route / unreachable component** — caught by the BFS in `lint-gate/build.rs`, emitted as `cargo::error=E-ROUTE-002: Route::X is unreachable from entry_point — either add a Link/nav!(Route::X) producer, add programmatic<T>, or mark it entry_point`.

Classes 1 and 2 hit the Rust compiler first; only class 3 needs the build script. Every error has an actionable fix listed in the message.

---

## 4. Check rules — BFS and nothing else

The build-time checker builds one directed graph `G = (V = Route variants, E = LINK_CALLSITES ∪ PROGRAMMATIC_EDGES)` and runs a single BFS from `entry_point`.

### 4.1 Rules

1. Every `Route` variant must appear in `#[connected(...)]`. Missing → **E-ROUTE-001: undeclared route**.
2. Every variant's `#[connected(...)]` must carry at least one of `linked` / `entry_point` / `programmatic<T>`. Empty → **E-ROUTE-001: undeclared route** (same class, caught at macro expansion).
3. Exactly one variant may carry `entry_point` → **E-ROUTE-004: multiple entry points** otherwise.
4. Variants reachable from `entry_point` via BFS over `LINK_CALLSITES` + `PROGRAMMATIC_EDGES` pass. Variants not reachable → **E-ROUTE-002: orphan / unreachable route**.
5. For every variant declaring `linked` in its `#[connected(...)]`, there must be ≥1 entry in `LINK_CALLSITES` targeting its discriminant → **E-ROUTE-003: `linked` declared but no producer** (a self-check on the route author's expectation, not the graph itself).

Checks 1–3 fire at macro expansion or via the trait system. Only 4 and 5 run in `build.rs`. There is no symmetric-difference pass, no label-match pass, no "half-edge" failure mode.

### 4.2 Example error text

```
error[E-ROUTE-002]: orphan route — Route::DmMediaViewerRoute is unreachable from entry_point
 --> crates/core/src/ui/routes.rs:175:13
    = note: BFS starting at Route::Root never visits this variant.
    = help: either (a) add a `Link { to: Route::DmMediaViewerRoute }` or
            `nav!(Route::DmMediaViewerRoute { ... })` callsite somewhere,
            or (b) add an impl of ProgrammaticProducer<Target = Route::DmMediaViewerRoute>
            for a ZST tag and reference it via #[connected(programmatic<YourTag>)].

error[E-ROUTE-003]: Route::SettingsRoute declares `linked` but LINK_CALLSITES has no
                    entry targeting its discriminant
 --> crates/core/src/ui/routes.rs:190:9
    = help: drop `linked` (and use `programmatic<T>` if appropriate),
            or add a Link { to: Route::SettingsRoute } callsite.

error: `SignupCompletionLanding` is not a declared ProgrammaticProducer
 --> crates/core/src/ui/routes.rs:150:26
    |
150 |     programmatic<SignupCompletionLanding>,
    |                  ^^^^^^^^^^^^^^^^^^^^^^^
    = help: impl ProgrammaticProducer<Target = Route::…> for this type.
```

### 4.3 Non-enum navigation — URL strings

`routes.rs:358` stores URL strings in `account_last_routes`, and `signup/mod.rs:209` pushes opaque `landing` routes computed at runtime. These bypass the enum. Each such choke point wraps its runtime-computed route through a ZST producer, e.g. `AccountLastRouteRestore: ProgrammaticProducer<Target = ()>` — `Target = ()` is a sentinel meaning "any variant" because the target is determined at runtime. The producer's existence still marks its caller as a reachable entry point for the BFS; the check simply doesn't narrow the reachable set by its target. Expected count of `Target = ()` producers: 2 (the two choke points above).

---

## 5. Migration path

Rolling out a hard compile error across 79+ callsites in one commit is a non-starter. Three-phase migration:

### 5.1 Phase A — infrastructure, no enforcement (1 PR)

- [x] **5.1.1** Create `crates/ui-macros/` (proc-macro crate: `#[connected]`, `nav!`, `declare_programmatic!`, the `ProgrammaticProducer` trait, the RSX-walking component macro that registers `Link { to: Route::X }` callsites) and wire the graph-check module into `crates/lint-gate/build.rs` (the shared build script introduced in plan-component-lints.md §3.2). No separate `ui-macros-build` binary — the whole-graph assertion lives alongside the allow-ban and component-decorator scans in one build script.
- [x] **5.1.2** Add `linkme = "0.3"` and `diagnostic::on_unimplemented` trait marker setup.
- [x] **5.1.3** Macros compile and register; graph-check runs but emits `cargo::warning=` instead of `cargo::error=` while the backfill is in flight. Gate through the shared `regen-baseline` feature so that known violations grandfather into `crates/lint-gate/baseline.json`; new violations still fail the build.
- [x] **5.1.4** An optional debug helper `cargo run -p lint-gate --bin dump-routes` (tiny binary inside `lint-gate` that re-uses the same slice-walking code) dumps the current graph to `target/routes.dot` for inspection. Not a gate, just a visualization aid.

### 5.2 Phase B — backfill, warn loudly (one PR per UI module, parallelizable)

- [x] **5.2.1** Backfill `#[connected(...)]` on all 27 `Route` variants (single file edit in `routes.rs`). For each variant, inspect `sync_route_to_app_state` to list known producers; when unclear, start with `programmatic("TODO: audit")` and file follow-ups.
- [x] **5.2.2** Convert bare `navigator().push(Route::...)` → `nav!(Route::...)` across hotspot files (`favorites_sidebar.rs`, `channel_list.rs`, `chat_view.rs`, `signup/mod.rs`, `search.rs`, `settings/`). Mechanical rewrite — one-line regex because there are no labels to author.
- [x] **5.2.3** Enable `deny` for **E-ROUTE-002 (orphan / unreachable)** once every `Route` variant has `#[connected(...)]`. E-ROUTE-003 (`linked` declared but no producer) defaults to warn during backfill; flip to deny in Phase C.
- [x] **5.2.4** Stand up each `ProgrammaticProducer` ZST + impl at its source module. Expected ~5–8 types; one PR per cluster (signup flow, account-restore, deep-link, 401 redirect, push-notification open).

### 5.3 Phase C — full enforcement (1 PR)

- [x] **5.3.1** Drain `crates/lint-gate/baseline.json` to empty so E-ROUTE-001 emits as `cargo::error=` on plain `cargo check`. **Shipped:** baseline.json contains `"violations": []`.
- [x] **5.3.2** Route-graph violations excluded from `regen-baseline`. *Shipped (2026-04-17).* `crates/lint-gate/build.rs` skips grandfathering for `rule == "route_graph"` violations — they always emit `cargo::error=` even under `CARGO_FEATURE_REGEN_BASELINE`. Other rules (allow_ban, context_menu_coverage, nav_push_ban) still support baseline refresh.
- [x] **5.3.3** Bare `navigator().push(Route::...)` ban scanner shipped. `crates/lint-gate/build/nav_push_ban.rs` emits `cargo::error=` for any such pattern; wired into `build.rs::main`. See `nav_push_ban::scan`.

---

## 6. Interaction with sibling plans

### 6.1 `plan-context-menu-quality-control.md`

Some context-menu items *are* navigation (e.g. "Open in new tab", "Go to Settings"). Menu items that navigate use plain `nav!(Route::X)` inside their handler body — no extra DSL required. The `nav!` expansion registers the callsite in `LINK_CALLSITES` exactly as any other navigator would, so the BFS sees the edge:

```rust
#[context_menu(ServerContextMenu)]
#[component]
fn ServerIcon(props: ServerIconProps) -> Element {
    rsx! {
        /* … */
        // inside a menu-item handler:
        // onclick: move |_| nav!(Route::ServerSettingsRoute { server_id }),
    }
}
```

No integration glue needed between the two plans — both plans contribute expansions to the same `crates/ui-macros/` crate, and both register via `linkme` slices that the shared `crates/lint-gate/build.rs` reads.

### 6.2 `plan-component-lints.md`

No direct overlap (size/dead-code is orthogonal to reachability), but both plans introduce a proc-macro crate. **Single shared crate `crates/ui-macros/`** with module layout:

```
crates/ui-macros/src/
├── lib.rs          # #[proc_macro_derive] / #[proc_macro_attribute] entry points
├── connected.rs    # this plan — #[connected(...)] route-side attr
├── nav.rs          # nav!(Route::X { ... }) macro
├── programmatic.rs # ProgrammaticProducer trait + declare_programmatic! helper
├── link_walk.rs    # RSX walker that finds Link { to: Route::X } inside #[component]
├── context_menu.rs # sibling plan
└── component_lint.rs # sibling plan (size caps, dead-code)
```

Shared: `syn` parsing helpers, `linkme` registration helpers, error-span utilities. No crate-level conflict.

---

## 7. Testing

- [x] **7.1** 10 unit tests in `crates/ui-macros/src/connected.rs` and `context_menu.rs` covering: valid edges, empty edge list, duplicate `entry_point`, unknown ident, malformed `programmatic` (missing `<>`), qualified paths, trailing commas, multiple edges.
- [x] **7.2** 5 trybuild compile-fail fixtures under `crates/ui-macros/tests/compile-fail/` with generated `.stderr` snapshots: `multiple_entry_points.rs`, `unknown_ident.rs`, `empty_connected.rs`, `programmatic_no_angle.rs`, `context_menu_empty.rs`. Wired via `tests/compile_fail_tests.rs`. `cargo test -p poly-ui-macros --all-targets` passes (18 unit + 1 trybuild).
- [x] **7.3** 5 scanner unit tests in `crates/lint-gate/src/lib.rs::scanner_tests` covering: variant count, entry_point detection, programmatic producer detection, orphan detection, callsite extraction. `cargo test -p poly-lint-gate` passes.
- [x] **7.4** Runtime coverage counter shipped. `crates/core/src/ui/routes.rs` calls `record_route_visit(route)` under `#[cfg(debug_assertions)]`; records visits to a `OnceLock<Mutex<HashSet<&'static str>>>` and logs unvisited variants on repeat visits.
- [x] **7.5** Haiku subagent harness run completed. `cargo check --workspace` ✓, `cargo clippy` ✓, WASM build ✓, unit tests ✓.

---

## 8. Open questions

- **OQ-1** ~~`via` label grammar~~ — dropped; identity comes from the `Route` enum and `ProgrammaticProducer` types. No labels to name.
- **OQ-2** How do we handle `Route::PageNotFound { segments }` and `Route::Root`? Likely: `Root` carries `entry_point`; `PageNotFound` gets `programmatic<RouterNotFoundFallback>` with `const REASON = "router fallback for unknown URLs"`.
- **OQ-3** Dynamic route construction in `account_last_routes` (`format!("{route}")` in `routes.rs:361`) — treat each as a distinct `ProgrammaticProducer<Target = ()>` ZST (two expected: `AccountLastRouteRestore` and `SignupCompletionLanding`). `Target = ()` sentinel per §4.3.
- **OQ-4** `linkme` across `#[cfg(feature = "...")]`-gated backend modules: does a slice entry in a disabled feature disappear cleanly? Expected yes (the module doesn't compile → no registration), but needs a trybuild test in §7.
- **OQ-5** IDE/rust-analyzer — the `#[diagnostic::on_unimplemented]` errors should surface in hover, but `linkme` slice-mismatch errors only appear at link time. Need to check whether `cargo check` is sufficient or whether `cargo build` is required.
- **OQ-6** External deep links (mobile intent handlers, future OS URL-scheme handlers) — treat as a synthetic `external` producer or as `programmatic("external")`? Leaning `programmatic` to keep the producer side in-tree only.

---

## 9. Out of scope

- Runtime permission checks (e.g. "can this user reach this route") — this plan is purely about **structural reachability**, not authorization.
- i18n of error messages — `diagnostic::on_unimplemented` strings stay English; they're developer-facing.
- Automatic rewrite of existing `navigator().push(...)` → `nav!(...)` — provide a one-shot codemod script but do not commit automatic rewrites.
- Visualizing the route graph in the UI (route-debugger overlay) — possible follow-up built on the same `RouteDecl` slice, but separate plan.
- Validating route **parameter values** (e.g. `backend` matches a known `BackendType` slug) — the macro only checks variant identity. Runtime parameter validation stays where it already is in `sync_route_to_app_state`.
- Changes to the `Route` URL scheme itself.

---

## Files this plan will touch

- `crates/ui-macros/` *(new)* — proc-macro crate (shared with plans 1 + 3)
- `crates/lint-gate/` *(new, shared with plan 3)* — `build.rs` runs the graph assertion alongside the allow-ban and decorator-coverage scans; all three checks piggyback on a single workspace walk
- `crates/core/src/ui/routes.rs` — add `#[connected(...)]` to every variant
- `crates/core/src/ui/favorites_sidebar.rs`, `channel_list.rs`, `chat_view.rs`, `search.rs`, `signup/mod.rs`, `settings/mod.rs`, `create_server.rs`, `create_channel.rs`, `create_forum_post.rs`, `main_layout.rs`, `voice_banner.rs`, `server_overview.rs` — convert call sites
- `crates/core/src/ui/account/**/*.rs` — convert call sites
- `Cargo.toml` (workspace) — add `linkme`, `ui-macros`, `ui-macros-build`
- `docs/INDEX.md` — add an entry under section 4 (UI) once Phase A lands
