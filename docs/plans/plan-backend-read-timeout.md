# Plan — `backend.read_with_timeout(…)` Helper (Hang #4 Prevention)

> Status: **✅ DONE** — Phases 1-3 + 5 shipped (`66810bd1`, `a935f2a8`, `e4d3fde2`, `f6599e76`). Helper added, 8 FRAGILE + 46 SAFE sites migrated, `forbid-raw-backend-read.sh` lint blocks regressions in `crates/core/src/ui/`.
> Authors: orchestrator + audit subagent (`/tmp/poly-backend-read-timeout-audit.md`).
> Last updated: 2026-04-25.

---

## 1. Why this plan exists

CLAUDE.md § "Common WASM-hang causes" lists hang class #4:

> **`tokio::sync::RwLock::read().await` on a backend that has a perpetual
> writer.** Single-threaded WASM scheduler can starve readers. Wrap with
> `tokio::time::timeout(Duration::from_secs(5), backend.read())` and bail
> with a warning on timeout.

This plan exists to prevent that class — a `.read().await` call that never
returns because a writer perpetually holds the lock, wedging the WASM main
thread with no visible error.

**Audit findings (`/tmp/poly-backend-read-timeout-audit.md`):**

- **Zero** concrete incidents of hang #4 in production, staging, or dev.
- **Zero** `backend.write().await` call sites in the entire UI crate. The
  inner `RwLock<Box<dyn ClientBackend>>` inside each `BackendHandle` is
  effectively read-only at runtime — today.
- **54 `backend.read().await` sites** classified: **46 SAFE** (short-scoped,
  guard drops inside a tight block), **6 FRAGILE** (guard held across
  multiple `.await` points — would wedge if any writer ever lands), and
  **2 UNKNOWN** (deep nesting, flagged for manual review).

**Why ship anyway:** the 6 FRAGILE sites are bugs-waiting-to-happen in
exactly the same shape as the Teams Sheep hang was in for #3 — correct
today, wedging the moment a load path inserts a writer. The single-control-
point (`read_with_timeout`) gives us a lint-gateable surface and a place
to attach observability when it eventually bites. Preventive is cheap
(~1 focused day per §6) given we're already in the hang-prevention tour
for classes #1 and #3.

**Critical constraint the previous attempt got wrong:** the naive
`tokio::time::timeout(dur, backend.read())` wrap **panics on WASM**
because `Instant::now()` is unimplemented on `wasm32-unknown-unknown`.
Four in-tree comments document the removal of prior attempts:
`channel_list.rs:193-195`, `channel_list.rs:360-364`, `routes.rs:1067-1069`,
`draft_banner.rs:168-170`. Any shipped helper MUST `cfg`-gate:
- native → `tokio::time::timeout`
- WASM → `gloo_timers::future::TimeoutFuture` raced via `futures::select!`.

Missing the gate = reintroducing the exact panic bug we already removed.
§5 risks + §2 design spell this out.

---

## 2. Solution summary

Introduce `BackendHandle::read_with_timeout(&self, dur: Duration) -> Result<ReadGuard<'_>, Timeout>` as the **only** allowed `.read().await` surface on
backend handles:

```rust
// Native target: wraps tokio::time::timeout.
// WASM target: races the future against a gloo_timers::future::TimeoutFuture
//              or a dioxus::document::eval setTimeout promise.
pub async fn read_with_timeout(
    handle: &BackendHandle,
    dur: Duration,
) -> Result<tokio::sync::RwLockReadGuard<'_, Box<dyn ClientBackend>>, BackendReadTimeout>;
```

Key design points:

1. **WASM-safe timeout primitive.** The audit established `tokio::time::*`
   panics on `wasm32-unknown-unknown`. Implementation MUST `cfg`-gate:
   - `#[cfg(not(target_arch = "wasm32"))]` → `tokio::time::timeout`
   - `#[cfg(target_arch = "wasm32")]` → `futures::future::select` between the
     `read()` future and `gloo_timers::future::TimeoutFuture::new(ms)` (or
     `dioxus::document::eval("setTimeout(() => dioxus.send(true), N);").recv::<bool>()`).
   Failing to gate = reintroducing the exact panic we already removed.
2. **Default timeout: 5 seconds** for UI interactions. Caller may pass a
   longer duration (e.g. 30s for message-history chain loads).
3. **Timeout = tracing warn, not panic.** Starvation is recoverable by
   letting the caller give up and rerun the effect on the next user click;
   a panic would tear down the whole app.
4. **Ban raw `backend.read().await`** via a grep-based CI lint (Phase 5).
   Exceptions annotated `// poly-lint: allow raw backend.read().await —
   <reason>`.

---

## 3. Phases

Each phase lands independently behind its own commit. Phase N blocks on N−1.

### Phase 1 — Introduce the helper, no call-site changes

**Deliverable:** `crates/client-api/src/timeout.rs` (new file, ~120 lines)
OR `crates/core/src/client_manager/timeout.rs`, depending on where
`BackendHandle` lives. Target: add `BackendHandleExt::read_with_timeout`
trait impl.

Tasks:
- [ ] Add `gloo-timers = { version = "0.3", features = ["futures"] }` to
  `crates/core/Cargo.toml` under the `wasm32` cfg target, OR (if we prefer
  zero new deps) build on `dioxus::document::eval` setTimeout.
- [ ] Create `BackendReadTimeout` error type (unit struct, `Display`).
- [ ] Create `BackendHandleExt` trait with
  `async fn read_with_timeout(&self, dur: Duration) -> Result<ReadGuard<'_>, BackendReadTimeout>`.
- [ ] `cfg`-gate the implementation:
  - native: `tokio::time::timeout(dur, self.read()).await.map_err(...)`.
  - wasm: `futures::future::select(pin!(self.read()), gloo_timers::future::TimeoutFuture::new(dur_ms))`.
- [ ] `tracing::warn!("backend read timed out after {dur:?} at {location}")`
  on timeout, including a `#[track_caller]` location hint.
- [ ] Unit tests (native-only; WASM smoke-tested separately):
  - Read completes inside timeout → returns Ok.
  - Read blocks past timeout → returns Err.
  - Timeout duration of 0 → immediately Err.

Verification: `cargo check --workspace --target wasm32-unknown-unknown`
green, `cargo test -p poly-core` green, ZERO call-site diff.

### Phase 2 — Migrate FRAGILE sites (from audit)

**Deliverable:** replace `backend.read().await` with
`backend.read_with_timeout(Duration::from_secs(5)).await?` at the sites the
audit classified FRAGILE. Long-running operations (e.g. newer-messages
pagination) should use a longer timeout (30s) explicitly.

Sites to migrate (from `/tmp/poly-backend-read-timeout-audit.md` § FRAGILE):

- [ ] `crates/core/src/ui/account/common/chat_view.rs:731` — 5s (UI click)
- [ ] `crates/core/src/ui/account/common/chat_view.rs:3542` — 30s
  (MAX_CHAINED_NEWER_HISTORY_PAGES loop; longer budget)
- [ ] `crates/core/src/ui/favorites_sidebar.rs:1134` — 5s
- [ ] `crates/core/src/ui/client_ui/view/list_body.rs:238` — 5s
- [ ] `crates/core/src/ui/client_ui/view/list_body.rs:406` — 5s
- [ ] `crates/core/src/ui/client_ui/view/list_body.rs:758` — 5s
- [ ] `crates/core/src/ui/client_ui/view/tree_body.rs:210` — 5s
- [ ] `crates/core/src/ui/account/common/channel_list.rs:108` — 5s

Each migration includes an error branch that does one `chat_data.batch(|cd|
cd.loading = false)` + `tracing::warn!` and returns early.

Verification: for each migrated file, WASM smoke-test the relevant user
flow (permalink jump, history scroll, DM open, channel list expand).

### Phase 3 — Migrate remaining SAFE sites opportunistically

Not required for the hang prevention (they're already short-scoped), but
applying the helper uniformly across all 46 remaining sites makes the
lint in Phase 5 enforceable without a huge allowlist. Split into:

- [ ] `crates/core/src/ui/demo.rs` (3 sites)
- [ ] `crates/core/src/ui/account/common/*` (≈14 sites)
- [ ] `crates/core/src/ui/account/server/settings/*` (2 sites)
- [ ] `crates/core/src/ui/account/settings/mod.rs` (1 site)
- [ ] `crates/core/src/ui/account/channel/settings/mod.rs` (1 site)
- [ ] `crates/core/src/ui/client_ui/*` (≈20 sites)
- [ ] `crates/core/src/ui/favorites_sidebar.rs` (5 remaining sites)
- [ ] `crates/core/src/ui/search.rs` (1 site)

All 5-second default.

### Phase 4 — UNKNOWN review

- [ ] Manual review of `list_body.rs:238` and `tree_body.rs:210` — if
  genuinely long-scoped, migrate with 5s-30s timeout; if short-scoped,
  treat as SAFE and bulk-migrate in Phase 3.

### Phase 5 — Lint banning raw `backend.read().await`

Two tracks:

**Track A (regex CI check, fast):**
- [ ] Add `tools/scripts/forbid-raw-backend-read.sh` — grep for
  `backend\.read\(\)\.await` across `crates/core/src/ui/**/*.rs`.
- [ ] Allowlist: inline `// poly-lint: allow raw backend.read().await —
  <reason>` comment on the same line.
- [ ] Wire into `.github/workflows/ci.yml` (or whatever CI config runs).

**Track B (dylint custom lint, follow-on if we already shipped the
similar lint for Signal::write from `plan-batched-signal.md` Phase 5):**
- [ ] Add `tools/lints/poly-lints/src/forbid_raw_backend_read.rs` matching
  the `.read()` method call on `BackendHandle` via HIR — ignore
  `Signal::read`, `std::io::Read::read`, etc.
- [ ] Package under `cargo dylint`, wire into `cargo cranky`.

### Phase 6 — Documentation + cleanup

- [ ] Update CLAUDE.md § "Common WASM-hang causes" #4 to reference
  `BackendHandle::read_with_timeout` as the prescribed prevention.
- [ ] Add short dev-doc at `docs/dev/backend-locking.md` covering the
  5s-default / 30s-for-chain-loads convention.
- [ ] Remove obsolete inline comments in `channel_list.rs:193-195`,
  `channel_list.rs:360-364`, `routes.rs:1067-1069` that document the
  removed-tokio-timeout history (they're superseded by the helper).

---

## 4. Verification

After each phase:
- `cargo check --workspace --target wasm32-unknown-unknown` — clean.
- `cargo check --workspace` (native) — clean.
- `cargo test --workspace` — green.

After Phase 2:
- Manual WASM smoke: permalink jump, chain-load newer messages, DM click,
  channel list expand, favorites sidebar server swap. No hangs, no
  visual regression, no spurious timeout warnings in dev console.

After Phase 5:
- CI fails on a deliberate reintroduction of raw
  `backend.read().await` without `// poly-lint: allow …`.

---

## 5. Risks / failure modes (honest list)

1. **Timeout kills a legit long-running operation.** A slow network pulling
   500 messages over a 30s budget could still legitimately exceed the
   default 5s. Mitigation: caller-specified duration; document the
   5s/30s convention.
2. **WASM path drift.** If the WASM timeout primitive (gloo_timers /
   eval setTimeout / JsFuture) regresses or changes, silent hangs
   reappear. Mitigation: WASM integration test that spawns a fake
   10s-blocking backend and asserts the 100ms-timeout helper returns Err.
3. **Reintroducing the original panic bug.** If phase-1 forgets the
   `cfg`-gate and uses `tokio::time::timeout` on WASM, the app crashes on
   every backend read. Mitigation: explicit CI check that grep's
   `tokio::time::timeout` usage inside the helper and demands the
   cfg-gate block be present. Also: WASM smoke-test in CI.
4. **Starvation STILL possible if the timeout driver shares the starved
   executor.** If the WASM main thread is wedged by a tight CPU loop
   (hang class #1), the `setTimeout` callback can't fire either — the
   timeout helper gives no improvement. It only helps when the starvation
   is from lock contention with an async writer, which (per audit) we
   don't currently have.
5. **Cost per call.** On WASM the timeout driver adds one
   `setTimeout(0)` per read. ~54 reads per flow × a few 0ms timers is
   negligible in wall time but adds JS↔WASM bridge overhead.

---

## 6. Timeline estimate

| Phase | Budget | Agent tier |
|-------|--------|-----------|
| 1 — introduce helper | 0.5 session | sonnet-coding |
| 2 — migrate FRAGILE (8 sites) | 0.5 session | sonnet-coding |
| 3 — migrate SAFE (~46 sites) | 1 session | sonnet-coding (mechanical) |
| 4 — UNKNOWN review | 0.2 session | human or sonnet-coding |
| 5a — regex CI check | 0.2 session | sonnet-coding |
| 5b — dylint custom | 1 session (only if plan-batched-signal lint already shipped) | opus-coding |
| 6 — docs + cleanup | 0.2 session | sonnet-coding |

**Total if shipped fully: ~1 focused day.** Given the zero-incident rate,
this is cheap to pick up if an incident happens but not worth preempting.

---

## 7. Reference artifacts

- [`/tmp/poly-backend-read-timeout-audit.md`](file:///tmp/poly-backend-read-timeout-audit.md)
  — 54-site audit with per-file classification (SAFE / FRAGILE / UNKNOWN),
  tokio-runtime investigation, incident-log review.
- CLAUDE.md § "Common WASM-hang causes" #4 — the hang class.
- Sibling plan: `docs/plans/plan-batched-signal.md` — same structure,
  targets hang class #1 (which HAS real incidents).
- In-tree removed-tokio-timeout comments:
  `crates/core/src/ui/account/common/channel_list.rs:193-195`,
  `crates/core/src/ui/account/common/channel_list.rs:360-364`,
  `crates/core/src/ui/routes.rs:1067-1069`,
  `crates/core/src/ui/account/common/draft_banner.rs:168-170`.

---

## 8. Out of scope

- **`backend.write().await`** — entirely different failure mode (`write`
  starving readers rather than readers starving on a writer). Audit
  found zero write sites anyway; not worth covering until one lands.
- **General deadlocks** — two-backend / cross-lock acquisition order bugs.
  Those need a lock-ordering protocol, not a timeout.
- **Rewriting the `ClientBackend` plugin trait** — the trait contract stays
  the same; only the call-site helper changes.
- **Replacing `tokio::sync::RwLock` with `async_lock::RwLock` or
  `futures::lock::Mutex`** — different library, different API, orthogonal
  discussion; this plan stays within the existing lock type.
- **Covering non-UI code** — the helper applies only to UI-crate call
  sites. Plugin internals, host router handlers, and test harnesses keep
  raw `.read().await` (gated by allowlist on the Phase-5 lint).
