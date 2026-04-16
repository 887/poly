# Plan — Connected Routes Static Check

> **Created:** 2026-04-16
> **Status:** 🔵 drafted
> **Scope:** cross-cutting — `crates/core/src/ui/routes.rs`, every link/button/navigator callsite under `crates/core/src/ui/`, the shared `crates/ui-macros/` proc-macro crate, and the shared `crates/lint-gate/build.rs` graph checker
> **Goal:** `cargo check`-native, bidirectional reachability check — every `Route` variant declares its incoming edges and every link/button/`navigator()` call declares its destination. Mismatches surface as `cargo::error=` lines on plain `cargo check` (no `#[test]` to skip, no xtask to forget); orphan routes must supply a human-readable `programmatic(...)` reason.

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
- **Edges** — one per link/button/`navigator().push(...)` callsite, labeled with a `via` surface string. `|E| ≈ 115`. Directed, since routes link to routes; not symmetric (a sidebar → chat link does not imply a chat → sidebar link).
- **Root** — the `entry_point` variant (`Root`). Exactly one; an error if more than one node carries that marker.
- **Programmatic edges** — routes reachable only via non-UI producers (redirect after signup, deep-link handler, push-notification open) are edges whose producer is a `programmatic("reason")` marker, not a `Link`/`navigator!()` callsite. These still count toward in-degree ≥ 1.

What the check enforces, in graph terms:

| Check | Graph statement | Failure name |
|---|---|---|
| Every route is reachable | `in_degree(v) ≥ 1` for all `v ≠ root` | **orphan route** |
| Every declared incoming edge has a producer | every edge in the route's `incoming(...)` list must be matched by a producer-side edge with the same `via` label | **unconsumed declaration** |
| Every producer has a consumer | every `Link { to, via }` / `nav!(via, to)` must match a declaration on the target route | **undeclared edge** |
| No dead-end routes (optional, warn) | `out_degree(v) == 0` flagged unless annotated `#[dead_end("reason")]` | **leaf warning** |
| Graph is connected from root | every `v` is reachable from `root` via BFS over edges + programmatic markers | **unreachable component** |

The primary guarantee the plan buys is: **the route graph is a single connected component rooted at `entry_point`.** Orphan routes are disconnected nodes; unconsumed declarations and undeclared edges are dangling half-edges that would corrupt the graph if ignored.

`Route` is not a tree — several producers can target one route (e.g. `ServerChat` has many entries: sidebar channel-list, notifications, search, deep links). That is fine; the check is about **reachability**, not **uniqueness of paths**. Reducing the graph to a **spanning tree** rooted at `entry_point` is not a goal — it would imply one canonical path per route, which is wrong for a chat app where every route has many natural entrances.

What the plan does *not* enforce:

- **No cycle detection.** Cycles are normal (Settings ⇄ Server Settings sub-page). The check is agnostic to cycles.
- **No shortest-path analysis.** The graph's diameter doesn't matter.
- **No strongly-connected-component decomposition.** Listed here only to rule it out: we do not partition the graph and we do not care whether the graph is strongly connected, only weakly connected from `entry_point`.

Operationally, the check materializes two sets during a build: `declared_edges` (route-side) and `produced_edges` (callsite-side). Both live in `linkme` distributed slices. The build-script-run checker computes `set(declared) △ set(produced)` (symmetric difference) and a BFS from `entry_point`. Any non-empty output is a compile error.

---

## 2. The DSL — concrete syntax for both directions

Two paired proc-macros in a new `crates/ui-macros/` crate, re-exported from `crates/core` as `poly_ui::{connected, links_to, nav}`.

### 2.1 On route/page declarations (incoming edges)

Attach a `#[connected(...)]` attribute to either the `Route` enum variant or the page component function. The attribute enumerates every **producer** of a navigation to that route.

```rust
#[connected(
    // Normal incoming edges — must be matched by a `#[links_to(...)]`
    // or `nav!()` callsite somewhere in the workspace.
    via("favorites-sidebar:account-icon"),
    via("channel-list:dm-row"),
    via("search:dm-result"),
    via("signup:on-complete-landing"),

    // Programmatic/automated entry that has no clickable producer.
    // The string is a free-form justification captured in compile output.
    programmatic("sync_route_to_app_state fallback when account_last_routes empty"),
)]
#[route("/:backend/:instance_id/:account_id/dms")]
DmsHome { backend: String, instance_id: String, account_id: String },
```

Three edge kinds:

| Kind | Syntax | Consumer required? | Use |
|------|--------|--------------------|-----|
| `via("label")` | `via("sidebar-accounts")` | **Yes** — must match a `#[links_to(..., via = "sidebar-accounts")]` somewhere | Normal buttons, links, menu items |
| `entry_point` | `entry_point` (bare) | No | The one root route loaded on cold start. Only one variant may carry this. |
| `programmatic("reason")` | `programmatic("401 redirect from AccountBar")` | No, but reason string is **mandatory** and lint-checked for non-empty | Automated redirects, default-landing, `on_update` replaces, external deep-links |

A route with no `#[connected(...)]` at all is an orphan and fails the check. A route with only `via(...)` edges that never resolve is also an orphan.

### 2.2 On link/button/navigator callsites (outgoing edges)

Three flavors, one per call style already present in the codebase:

**RSX `Link` (Dioxus component):** add a `via:` prop that the macro reads.

```rust
Link {
    to: Route::SettingsRoute,
    via: "favorites-sidebar:gear-button",
    "Settings"
}
```

**Imperative `navigator()`:** wrap through a declarative `nav!` macro that emits the same `navigator().push(...)` at runtime but records the edge at compile time.

```rust
nav!(via = "favorites-sidebar:account-icon",
     Route::DmsHome {
         backend: backend.clone(),
         instance_id: instance_id.clone(),
         account_id: account_id.clone(),
     });
```

`nav!` expands to `navigator().push(route)` unchanged — zero runtime cost — plus a `#[linkme::distributed_slice]` registration entry.

**Attribute on plain handlers / context-menu items** (the sibling plan's surface):

```rust
#[links_to(Route::ServerChat { .. }, via = "channel-list:chat-row")]
fn on_click_channel_row(...) { ... }
```

The `..` in the `Route::X { .. }` pattern means "any variant args" — the static check only cares about the variant identity, not its runtime field values.

### 2.3 Label grammar

`via` labels are slash-separated hierarchical tags: `"<surface>:<element>[:<qualifier>]"`. Enforced by a const regex in the macro (`^[a-z0-9][a-z0-9-]*(:[a-z0-9][a-z0-9-]*){1,2}$`). Surfaces: `favorites-sidebar`, `channel-list`, `chat-view`, `settings`, `signup`, `account-bar`, `context-menu`, `search`, `notifications`, … Extensible; the set is just the union of what shows up in the code.

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

Clippy lints can flag individual callsites but cannot see the full graph across crates in one pass. A custom `dylint` pass can, but ships out-of-tree and is awkward in CI. Use `dylint` only for *span-level* hints pointing at the exact missing `via` on a `Link`, not for the global graph check.

### 3.4 Pipeline

1. `#[connected(...)]` proc-macro on each `Route` variant → emits `#[linkme::distributed_slice(ROUTE_DECLARATIONS)] static: RouteDecl { variant: "DmsHome", edges: &[...] }`.
2. `#[links_to(...)]` and `nav!(...)` and `Link { via: ... }` → emit `#[linkme::distributed_slice(LINK_CALLSITES)] static: LinkCallsite { target: "DmsHome", via: "favorites-sidebar:account-icon", file, line }`.
3. **The graph assertion runs from `crates/lint-gate/build.rs`** — the same build script that plan-component-lints.md §3.2 uses for the allow-ban scan and plan-context-menu-quality-control.md §3.1.2 uses for decorator coverage. `lint-gate` depends on `ui-macros` so both slices are in scope; the script materializes `declared_edges` and `produced_edges` from the slices, computes symmetric difference and BFS from `entry_point`, and emits one `cargo::error=E-ROUTE-XXX: ...` line per violation (stabilized Rust 1.84). Because `cargo check` runs every `build.rs` before typechecking, the error surfaces automatically — rust-analyzer red-squiggles it on save, `cargo check` exits non-zero in CI, no agent can quietly skip it.
4. **One workspace walk, three checks.** `lint-gate`'s `build.rs` already walks `*.rs` for the allow-ban (plan 3 §3.2) and for `#[component]`-decorator coverage (plan 1 §3.1.2). The slice-level graph check bolts onto the same pass — negligible additional cost (slice read + hash-set diff). Clean-build cost stays in the 100–300 ms range even with all three lints active.
5. WASM-side note: `build.rs` runs on the host during every `cargo check --target wasm32-unknown-unknown`. The registry content is source-level (macros emit `&'static` data populated at compile time and linked per-target), so the host build sees everything the WASM build would have registered. No WASM-specific tooling required.

### 3.5 Error surface — use `#[diagnostic::on_unimplemented]` (stable 1.78+)

Wrap the marker types in a sealed trait so typos in `via = "..."` produce a readable compiler error pointing at the exact RSX line:

```rust
#[diagnostic::on_unimplemented(
    message = "no Route declares `via(\"{Via}\")` as an incoming edge",
    label = "add `via(\"{Via}\")` to the target Route's #[connected(...)]",
    note = "or use #[connected(programmatic(\"reason\"))] if this link is automated"
)]
trait ViaDeclared<const Via: &'static str> {}
```

This gives **per-callsite** diagnostics with proper spans, not just build.rs stderr.

---

## 4. Bidirectional matching logic

The build-time checker collapses both slices into one directed graph `G = (V = Routes, E = edges)`:

- Producer side: each `LinkCallsite { target, via }` is a candidate edge `(?, target, via)`.
- Consumer side: each `RouteDecl { variant, edges: [via("x"), programmatic("…"), entry_point] }` is a set of *expected* edges into `variant`.

### 4.1 Match rules

1. For every `via("L")` on a `RouteDecl`, there must be **≥1** `LinkCallsite` with `target = variant ∧ via = L`. If zero → **error: unconsumed incoming edge**.
2. For every `LinkCallsite { target, via }`, there must be **≥1** `RouteDecl` for `target` that contains `via("L")` with matching `L`. If zero → **error: unexpected outgoing link**.
3. Every variant of `Route` must appear in at least one `RouteDecl`. Missing → **error: undeclared route**.
4. Each variant's `RouteDecl` must have ≥1 edge of any kind (`via` / `programmatic` / `entry_point`). A `RouteDecl` with an empty edge list → **error: orphan without reason**.
5. Exactly one variant may carry `entry_point` → **error: multiple entry points** otherwise.
6. `programmatic("reason")` must have a non-empty `reason` string (trimmed). Checked at macro expansion → **error: programmatic reason required**.

### 4.2 Example error text

```
error[E-ROUTE-001]: Route::DmsHome declares `via("favorites-sidebar:account-icon")`
                    but no link, button, or `nav!(...)` in the workspace targets it
 --> crates/core/src/ui/routes.rs:150:9
    |
150 |     via("favorites-sidebar:account-icon"),
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    = help: either remove this `via(...)` from #[connected(...)],
            or add `via: "favorites-sidebar:account-icon"` to a Link/nav! targeting DmsHome.

error[E-ROUTE-002]: orphan route — Route::DmMediaViewerRoute has no #[connected(...)]
 --> crates/core/src/ui/routes.rs:175:13
    = help: add #[connected(via("chat-view:attachment-thumbnail"))]
            or #[connected(programmatic("reason"))] if you reach it some other way.

error[E-ROUTE-003]: link targets Route::SettingsRoute with via="foo-bar",
                    but Route::SettingsRoute only declares via("favorites-sidebar:gear-button")
 --> crates/core/src/ui/favorites_sidebar.rs:321:21
    = help: did you mean via="favorites-sidebar:gear-button"?
```

### 4.3 Non-enum navigation — URL strings

`routes.rs:358` stores URL strings in `account_last_routes`, and `signup/mod.rs:209` pushes opaque `landing` routes computed at runtime. These bypass the enum. The plan treats them as **programmatic** and requires their call sites be wrapped in `nav_dynamic!(reason = "restore-last-account-route")` which registers against a synthetic `*` target and does not require a matching `via`.

---

## 5. Migration path

Rolling out a hard compile error across 79+ callsites in one commit is a non-starter. Three-phase migration:

### 5.1 Phase A — infrastructure, no enforcement (1 PR)

- [ ] **5.1.1** Create `crates/ui-macros/` (proc-macro crate: `#[connected]`, `#[links_to]`, `nav!`, `nav_dynamic!`, derive helpers) and wire the graph-check module into `crates/lint-gate/build.rs` (the shared build script introduced in plan-component-lints.md §3.2). No separate `ui-macros-build` binary — the whole-graph assertion lives alongside the allow-ban and component-decorator scans in one build script.
- [ ] **5.1.2** Add `linkme = "0.3"` and `diagnostic::on_unimplemented` trait marker setup.
- [ ] **5.1.3** Macros compile and register; graph-check runs but emits `cargo::warning=` instead of `cargo::error=` while the backfill is in flight. Gate through the shared `regen-baseline` feature so that known violations grandfather into `crates/lint-gate/baseline.json`; new violations still fail the build.
- [ ] **5.1.4** An optional debug helper `cargo run -p lint-gate --bin dump-routes` (tiny binary inside `lint-gate` that re-uses the same slice-walking code) dumps the current graph to `target/routes.dot` for inspection. Not a gate, just a visualization aid.

### 5.2 Phase B — backfill, warn loudly (one PR per UI module, parallelizable)

- [ ] **5.2.1** Backfill `#[connected(...)]` on all 27 `Route` variants (single file edit in `routes.rs`). For each variant, inspect `sync_route_to_app_state` to list known producers; when unclear, start with `programmatic("TODO: audit")` and file follow-ups.
- [ ] **5.2.2** Backfill `Link { via }` + `nav!` in hotspot files first: `favorites_sidebar.rs`, `channel_list.rs`, `chat_view.rs`, `signup/mod.rs`, `search.rs`, `settings/`. Use codemod (`cargo fix`-compatible macro rewrites) to convert bare `navigator().push(...)` → `nav!(via = "TODO-<file>-<line>", ...)`.
- [ ] **5.2.3** Enable `deny` for **E-ROUTE-002 (orphan)** and **E-ROUTE-003 (unknown via)**; keep **E-ROUTE-001 (unconsumed)** as warn so half-migrated routes don't block the build.
- [ ] **5.2.4** Drive TODO count to zero — grep for `"TODO-"` via labels in CI.

### 5.3 Phase C — full enforcement (1 PR)

- [ ] **5.3.1** Drain `crates/lint-gate/baseline.json` to empty so E-ROUTE-001 emits as `cargo::error=` on plain `cargo check`.
- [ ] **5.3.2** Remove the `regen-baseline` warn downgrade path for route-graph violations.
- [ ] **5.3.3** Add the bare `navigator().push(Route::...)` pattern to the `lint-gate` build.rs banned-pattern list (same mechanism as the `#[allow(dead_code)]` scan): any occurrence outside a `nav!`/`Link` macro expansion emits `cargo::error=`, closing the bypass loophole permanently on the same `cargo check` surface.

---

## 6. Interaction with sibling plans

### 6.1 `plan-context-menu-quality-control.md`

Some context-menu items *are* navigation (e.g. "Open in new tab", "Go to Settings"). Both DSLs must compose. Proposal:

```rust
#[context_menu(
    label = "Go to server settings",
    links_to(Route::ServerSettingsRoute { .. }, via = "context-menu:server:settings"),
)]
fn server_settings_menu_item(...) { ... }
```

The `links_to(...)` slot inside `#[context_menu(...)]` delegates to the same macro internals as standalone `#[links_to]` and emits the same `LinkCallsite`. Integration point: `crates/ui-macros/src/context_menu.rs` re-exports helpers from `crates/ui-macros/src/links_to.rs`.

### 6.2 `plan-component-lints.md`

No direct overlap (size/dead-code is orthogonal to reachability), but both plans introduce a proc-macro crate. **Single shared crate `crates/ui-macros/`** with module layout:

```
crates/ui-macros/src/
├── lib.rs          # #[proc_macro_derive] / #[proc_macro_attribute] entry points
├── connected.rs    # this plan
├── links_to.rs     # this plan
├── nav.rs          # nav!, nav_dynamic!
├── context_menu.rs # sibling plan
└── component_lint.rs # sibling plan (size caps, dead-code)
```

Shared: `syn` parsing helpers, `linkme` registration helpers, error-span utilities. No crate-level conflict.

---

## 7. Testing

- [ ] **7.1** Unit tests in `crates/ui-macros/tests/` covering: valid `#[connected]`, empty edge list, invalid `via` label grammar, duplicate `entry_point`, `programmatic` with empty string, mismatched type pattern in `#[links_to]`.
- [ ] **7.2** `trybuild`-based compile-fail fixtures under `crates/ui-macros/tests/compile-fail/`:
  - `orphan_route.rs` — Route variant with no `#[connected]`
  - `unconsumed_via.rs` — Route declares `via("x")` but nothing produces it
  - `unknown_via.rs` — Link declares `via = "x"` but target Route doesn't expect it
  - `multiple_entry_points.rs`
  - `empty_programmatic.rs`
- [ ] **7.3** Integration test crate `crates/ui-macros-tests/` (NOT `crates/core`) with a miniature `Route` enum and 3 page components, deliberately orphaning one; assert the checker binary exits non-zero and produces the expected diagnostic.
- [ ] **7.4** Runtime coverage counter in `sync_route_to_app_state` under `#[cfg(debug_assertions)]` that records which variants are actually visited during a dev session — lets us compare *declared* edges against *exercised* edges. Optional "dead route" warning.
- [ ] **7.5** Run the harness via a haiku subagent per `TEST_HARNESS.md` after Phase A and after Phase C lands.

---

## 8. Open questions

- **OQ-1** Should `via` labels be generated or hand-written? Hand-written is verbose but greppable. Generated from file+line is automatic but brittle across refactors. **Tentative: hand-written, with a lint that flags duplicates.**
- **OQ-2** How do we handle `Route::PageNotFound { segments }` and `Route::Root`? Likely: both are `entry_point`-adjacent. `Root` gets `entry_point`; `PageNotFound` gets `programmatic("router fallback for unknown URLs")`.
- **OQ-3** Dynamic route construction in `account_last_routes` (`format!("{route}")` in `routes.rs:361`) — how granular does the programmatic-reason requirement get? Proposal: one `nav_dynamic!(reason = ...)` at each choke point (currently two: `sync_route_to_app_state` on_update restore, and `signup` on-complete-landing).
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
