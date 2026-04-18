# Plan — Client-UI Polish Follow-Ups

> **Created:** 2026-04-19
> **Status:** 🔴 OPEN
> **Parent:** `plan-client-ui-polish.md` (✅ 10 packs shipped but with known gaps)
> **Why this plan exists:** the parent polish plan was marked ✅ COMPLETE based on `cargo check --workspace` + `cargo test --workspace --lib` — which skips integration & e2e tests. A full `cargo test --workspace` reveals **30 failures** in `poly-plugin-loader-tests` and several deferred items across packs. This plan collects every named follow-up in one honest list.

---

## 0. Honest accounting

The previous status report claimed "363 passing." That count came from `--lib` only. Full-suite reality:

```
cargo test --workspace  →  463 passed, 30 failed, 14 ignored
```

**Every failure is in `poly-plugin-loader-tests::client_e2e`** — the WASM-plugin-host harness. Each test panics with:

```
WASM plugin not found: /home/laragana/workspcacemsg/target/wasm32-wasip1/debug/poly_demo.wasm
Build with: cargo component build -p <crate> --target wasm32-wasip2
```

These are **pre-existing infrastructure failures** — the e2e harness was always designed to require a separate `cargo component build` step. The polish plan filled in harness bodies (layer (d) per §1.2) but never proved them runnable. That's the gap. Not "code bugs I introduced" — but also not "tests that exist and pass," which is what I implied.

---

## 1. Item inventory

### 🔴 F1 — WASM plugin harness can't run without manual build step

**Symptom:** 30 `client_e2e` tests fail with "WASM plugin not found" unless user runs `cargo component build -p poly-demo --target wasm32-wasip2` (and the other 5 plugins for their suites) first.

**Impact:** The harness bodies filled across Packs A / B / C / D / E are technically unverified. They compile, but assertions never ran. Any bug in the plugin → host round-trip that a harness test would have caught is still lurking.

**Fix options (ranked):**
1. **Add a `build.rs` or xtask** that runs `cargo component build -p poly-demo -p poly-stoat -p poly-matrix -p poly-discord -p poly-teams -p poly-server-client --target wasm32-wasip2` before the e2e test binary links. One-time pain; works automatically thereafter.
2. **CI pipeline step** that does the `cargo component build` explicitly before `cargo test`. Doesn't help local dev but stops green-washing in CI.
3. **Harness tests gracefully skip** when artifact missing (emit a warning, return `Ok`). Safer but hides regressions.

**Recommended:** combine 1 + 3 — build.rs attempts the build; if it fails, harness tests `#[ignore]` themselves with a clear message.

**Pre-req knowledge:** the `poly-plugin-host-tests` crate's Cargo.toml probably lists the 6 plugins as dev-dependencies with specific targets. Verify before writing build.rs.

**Estimated size:** medium. The build.rs is ~30 lines; the harness skip-if-missing guards need editing across `crates/plugin-host-tests/tests/client_e2e/{demo,discord,matrix,stoat,teams,server}.rs`.

---

### 🟠 F2 — D12 `BackendCapabilities` flag removal (carried from Pack H)

**Symptom:** `clients/client/src/types.rs` has 7 `TODO(D12)` markers on fields (`presence`, `typing_indicators`, `reactions`, `search_messages`, `attachments`, `create_server`, `create_channel`). Pack H deferred removal because parallel Pack E agents would have raced.

**Fix:** Pack E is now landed. Remove each field, update all readers. Each field has ~5-10 readers across the workspace.

**Steps:**
1. For each field: `grep -rn "caps.{field}" crates/ clients/` — inventory readers.
2. Decide replacement: if the reader used the field to gate UI, either (a) hard-code to "always true" if the feature is universally wanted, or (b) replace with a plugin-declared surface (menu item / toolbar option / etc.) per the new WIT.
3. Remove from the struct. Update `capabilities_for_slug` in types.rs.

**Test matrix:** per §1.2 Pack H (a) + (f) + (d). The `forbid_backend_slug_match_in_ui` lint should stay green.

**Estimated size:** small-medium. ~1 day of careful removal + test updates.

---

### 🟠 F3 — UI snapshot goldens infrastructure

**Symptom:** Per §1.2 layer (h), every pack was supposed to ship Playwright-driven snapshot diffs. Zero goldens were committed. Visual-polish packs (A, D, G, J) are unverified against regressions.

**Fix:** Dedicated "snapshot-infra" pack:
1. Install Playwright test runner.
2. Wire a `tests/snapshots/<backend>/<surface>.html` directory structure.
3. For each existing pack's demo-friendly backend (demo_forum, demo, discord-mock via plugin): record initial DOM snapshots.
4. CI job diffs on every PR; golden refresh is an explicit commit action.

**Blockers:** non-demo backends (stoat/matrix/teams/server-client) need credentials or test servers. demo + demo_forum are the achievable baseline; defer others.

**Estimated size:** large. Playwright integration + per-backend surface enumeration + CI wiring is ~2-3 days of setup work.

---

### 🟠 F4 — Matrix `m.space.parent`/`m.space.child` tree nesting

**Symptom:** `SpacesRoomsLayout` (Pack D) renders spaces-and-rooms with depth=1 only. Real Matrix spaces can nest N deep via `m.space.parent` and `m.space.child` state events.

**Fix:** requires the Matrix backend (`clients/matrix/src/lib.rs`) to expose `get_space_children(space_id)` → `Vec<Space>` OR extend `get_servers()` to return a hierarchical structure. Then SpacesRoomsLayout recursively descends.

**Blocker:** WIT interface change (`client-sidebar` would need to express nested spaces) OR a Matrix-specific extension. Might also be expressible as `SidebarLayoutKind::Custom` + declared `sidebar-sections` with `parent_id` hooks (already in the WIT).

**Recommended:** use the existing Custom layout pattern. Matrix plugin declares its space tree via `sidebar-declaration.sections` with parent_id links; no WIT change needed. `CustomSidebar` host component already reconstructs trees from flat+parent_id.

**Estimated size:** medium. Matrix plugin needs to fetch + return the tree; `CustomSidebar` rendering already works.

---

### 🟡 F5 — GitHub discussions tab (Pack E.3)

**Symptom:** `get_view_rows(..., tab_id=Some("discussions"))` returns empty for GitHub. GitHub discussions use GraphQL, not REST.

**Fix:** implement a GraphQL client in `clients/github/src/api.rs` OR use the REST `issues` endpoint with `filter=discussions` if GitHub adds it (they haven't as of writing).

**Estimated size:** medium (new GraphQL client) or defer indefinitely. Current empty placeholder is acceptable.

---

### 🟡 F6 — HN discussions / comments beyond depth-1 (Pack E.2)

**Symptom:** `get_view_detail` for HackerNews returns only top-level comments (depth-1). Real HN threads go deep.

**Fix:** recursive comment fetch in `get_view_detail`. HN's Firebase API provides `kids: [id, id, ...]` per item; need to fetch each recursively. Be careful of request volume — cache + limit depth.

**Estimated size:** small-medium.

---

### 🟡 F7 — Custom-block usage lint scanner

**Symptom:** Pack G's plan called for a `custom_block_usage.rs` lint-gate scanner that counts `CustomBlock { ... }` literal constructions per plugin and warns if any plugin exceeds a threshold (plan §4.7 P40). Pack G agent landed shadow-root + sanitizer tests but skipped this scanner.

**Fix:** mirror `action_id_naming.rs` in `crates/lint-gate/build/` — scan `clients/*/src/**.rs` for `CustomBlock {` literal sites, count per plugin, emit warning violation when count > 5.

**Estimated size:** small. ~60 lines of scanner + unit tests.

---

### 🟡 F8 — Custom-block compile-fail trybuild fixture

**Symptom:** §1.2 Pack G layer (g) called for a trybuild fixture that rejects `CustomBlock` constructions with `<script>` in `sanitized_html` at compile time. Pack G agent skipped.

**Fix:** add `crates/ui-macros/tests/compile-fail-client-ui/custom_block_with_script.rs` that constructs a CustomBlock literal with `<script>`. The lint needs a proc-macro or build-script check to catch this — Rust's type system alone can't detect HTML content in a string. Either make it a lint-gate check (same as F7 expansion) or drop the trybuild layer for this item and document why.

**Recommended:** drop the trybuild; replace with a runtime assertion in `sanitize_html` that debug-asserts no `<script>` survives. Runtime test already exists (`script_tag_stripped`).

**Estimated size:** trivial (drop + doc) or medium (real compile-time check via proc-macro).

---

### 🟡 F9 — Host-api KV for plugin settings (Pack C polish)

**Symptom:** Pack C wired `SettingsStorageCell` as an in-memory `HashMap<String, String>`. Settings round-trip within a session but don't persist across process restart. The polish plan called for `host-api.kv_get` / `kv_set` — real persistent storage.

**Fix:** each plugin's `SettingsStorageCell` calls `host_api::kv_get(key)` / `kv_set(key, value)` instead of the in-memory HashMap. Backing file is the app's SurrealKV store.

**Blocker for WASM guests:** plugins compiled as WASM cdylibs don't have direct access to host_api imports from `SettingsStorageCell` (which lives in poly-client, which is used natively AND as a WASM dep). The cleanest path is a WIT-surface extension or a host-side settings service that the plugin calls via an existing WIT import.

**Estimated size:** medium-large. Requires design discussion.

---

### 🟡 F10 — State-aware menu items for remaining backends (Pack E polish)

**Symptom:** Pack E wired state-aware menus for Lemmy (Subscribe/Unsubscribe), GitHub/Forgejo (Star/Unstar). Discord/Stoat/Matrix/Teams menus still return static items (plan P44).

**Fix:** each chat backend's `get_context_menu_items` should conditionally return Mute/Unmute, Favorite/Unfavorite, Block/Unblock based on local state. Requires the plugin to track those states or query them per menu-open.

**Estimated size:** per-backend, small-medium each (~1 hr per backend).

---

### 🔵 F11 — Non-English locale real translations

**Symptom:** Pack I seeded `locales/{de,es,fr}/plugin.ftl` for every plugin, but every entry is prefixed with a `# TODO(translate)` comment and uses the English value as fallback. ~30 files × ~50 entries each = ~1500 strings awaiting translation.

**Fix:** actual human translation OR machine-translation pass. Deliberately deferred — the stubs are runtime-correct (bundles are valid FTL, English renders), just not localized.

**Estimated size:** out of scope for engineering; a content task.

---

### 🔵 F12 — Chat-view UTF-8 em-dash mojibake (pre-existing)

**Symptom:** Documented in plan-client-ui-polish.md §4.15 P69. User text containing em-dashes (—) renders as `â` in the chat view's markdown→ammonia→`dangerous_inner_html` path. The forum path avoids this since post detail uses the existing ForumPostView renderer.

**Fix:** trace where UTF-8 bytes become Latin1-interpreted. Suspects: bindgen string conversion, ammonia's inner buffer handling, Dioxus's HTML insertion. Worth a dedicated debug session with `println!` at each boundary.

**Estimated size:** debug-heavy; could be 2 hours or 2 days depending on where the bug is.

---

## 2. Proposed sequencing

1. **Ship F1 first** — stop lying about test counts. Every future pack's status is only trustworthy once the e2e suite actually runs.
2. **F2 (D12 flags)** — small cleanup, unblocked, measurable.
3. **F7 + F8** — small lint-gate follow-ups.
4. **F4 (Matrix nesting)** — use CustomSidebar pattern; meaningful UX win.
5. **F10 (state-aware menus)** — per-backend, small increments.
6. **F5 + F6 + F9** — bigger per-backend or architectural work.
7. **F3 (snapshot infra)** — dedicated day.
8. **F11 + F12** — ongoing / debug-driven.

## 3. Acceptance criteria for this plan's completion

Each F-item lands with its own test set (per §1.2 of the parent plan). The plan closes when:

- `cargo test --workspace` (no `--lib` filter) reports zero failures.
- `cargo test --workspace --all-features` same.
- The polish plan's `docs/plans/plan-client-ui-polish.md` status table is trustworthy again — specifically the "layer (d) E2E via WASM host" column shows real pass counts.
- All `TODO(D12)` markers in `clients/client/src/types.rs` are gone.

## 4. What went wrong

Calling out the meta-lesson so future sessions don't repeat it: **`cargo test --lib` is not enough for "tests pass" claims.** It skips integration tests (which includes `tests/*.rs` files), so backend round-trip tests and the plugin-host e2e suite get silently excluded from the pass count.

Mandatory going forward: any "tests pass" claim must come from `cargo test --workspace` with no filter, OR explicitly state scope ("--lib only; integration tests require the WASM build step in F1").
