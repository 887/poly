# Plan: Host-substrate capability gating (`/host/*` authn + exec/kv confinement)

## Status: 📋 PLANNED

> Opened 2026-07-28 from the multi-agent review fan-out. Two **critical**
> findings survived adversarial verification and were judged too invasive to fix
> in the review pass; a third (CORS) was explicitly reported as BLOCKED by the
> host-bridge fix agent because `apps/` was outside its edit scope.
>
> Related landed work (already shipped, do not redo): the SSRF guard on
> `/host/http` (`crates/host-bridge/src/net_guard.rs`), manual redirect
> re-validation (`crates/host-bridge/src/lib.rs` `send_guarded` / `next_hop`),
> and the `/host/udp/send` connected-peer pin (`crates/host-bridge/src/udp.rs`).
> Those narrowed the *outbound* blast radius. This plan closes the *inbound*
> one.

---

## Why this plan exists

`poly-host` mounts the full `/host/*` route set on a fixed localhost port
(3000 / 3001 / 3002 / 9333) with `CorsLayer::allow_origin(Any)` and **no caller
authentication of any kind**. Three separate routes on that surface are
unconfined:

| Route | Confinement today | Consequence |
|---|---|---|
| `/host/exec` | none — no allowlist, no canonicalisation, no consent | arbitrary program execution |
| `/host/kv/*` | none — flat `poly_kv` table, no key prefix restriction | plaintext OAuth/bearer/refresh tokens readable and writable |
| `/host/plugin-kv/*` | caller-supplied `plugin` field | plugin namespaces are advisory, not enforced |

Because CORS is `Any` and there is no auth, every one of these is reachable
**cross-origin from any web page the user has open**, not merely from a plugin
the user chose to install. The `crates/plugin-host/src/host_impl.rs:470` comment
arguing that ungated exec is acceptable ("the host does not enforce that
declaration at runtime") assumes an installed-plugin trust boundary that the
HTTP twin does not have.

This also fails SOLID gate item 7 (test seams at every IO boundary) in spirit:
the boundary exists but carries no policy object, so there is nothing to
substitute in a test that proves a caller is confined.

---

## Phase A — Caller identity: a per-shell bearer token for `/host/*`

Nothing downstream can be scoped until requests carry an identity. This phase
adds one and is a prerequisite for Phases B, C and D.

- [ ] **A.1** Add a `HostAuth` struct to `crates/host-bridge/src/lib.rs` (next to
  the `ROUTE_*` consts at `:129`): a random 32-byte token minted at daemon start,
  plus `fn verify(&self, header: Option<&str>) -> Result<CallerId, HostAuthError>`.
  `CallerId` distinguishes `Shell` (the app's own WASM bundle) from
  `Plugin { id: String }`. Include an in-memory constructor for tests.
- [ ] **A.2** Persist/expose the token to the shell only: write it to the
  `HostState` in `apps/poly-host/src/lib.rs` and serve it from the Dioxus
  server half at page-render time (an injected `<meta>` or a `/host/status`
  field readable only from same-origin — decide in A.2 and record the choice in
  this file). It must **not** be obtainable by a cross-origin `fetch`.
- [ ] **A.3** Add an axum middleware layer in `apps/poly-host/src/lib.rs` that
  runs `HostAuth::verify` on every `/host/*` route and rejects with `401` on
  failure. Apply it to the router built at `:138` and to the three additional
  routers at `:181`, `:199`, `:216`.
- [ ] **A.4** Replace `CorsLayer::allow_origin(Any)` at
  `apps/poly-host/src/lib.rs:138-141`, `:181-184`, `:199-206` and `:216-219`
  with an explicit origin list (`http://127.0.0.1:{3000,3001,3002,9333}` and
  `http://localhost:` equivalents), derived from the bound port rather than
  hard-coded. `Any` must not survive anywhere in the crate.
- [ ] **A.5** Tests in `apps/poly-host/src/lib.rs`: a request with no token
  → 401; wrong token → 401; correct token → 200; a request with an
  `Origin` header outside the allowlist is rejected by the CORS layer.

## Phase B — `/host/exec` program allowlist + user consent

- [ ] **B.1** Extend the plugin manifest schema so a plugin declares the
  programs it needs (mirroring the existing `http_hosts` field). Record the
  parsed list on the registry entry in `crates/plugin-host/src/registry.rs`.
- [ ] **B.2** Add `fn check_exec(caller: &CallerId, program: &str) -> Result<PathBuf, ExecDenied>`
  to `crates/host-bridge/src/lib.rs`: resolves `program` to an absolute
  canonical path (rejecting relative paths, `..`, and symlinks that escape the
  resolved target), then matches it against the caller's declared list. Default
  deny with an explicit error — never a silent fallthrough.
- [ ] **B.3** Wire `check_exec` into `exec_command`
  (`crates/host-bridge/src/lib.rs:870`) before `Command::new` is constructed.
  The function must take the `CallerId` from Phase A rather than trusting the
  request body.
- [ ] **B.4** Persist a one-time consent record per `(plugin_id, program)` in
  `poly_kv` under a key the KV surface cannot rewrite (see C.2). Deny until
  consent exists; surface the prompt through the existing notification sink.
- [ ] **B.5** Retire the legacy exec branch: `apps/poly-host/src/lib.rs:533`
  `host_exec` and the `POST /host` tagged-union `dispatch` path both reach
  `exec_command`. Remove the legacy `POST /host` exec arm so there is exactly
  one gated entry point, and delete the "kept one release cycle" note from
  `CLAUDE.md` once it is gone.
- [ ] **B.6** Tests: unlisted program → denied; listed program reached via a
  relative path or a `..` traversal → denied; listed program with consent →
  allowed; the legacy `POST /host` exec shape → 404/410, not execution.

## Phase C — `/host/kv/*` namespacing and credential separation

- [ ] **C.1** Introduce a reserved key-prefix policy in
  `apps/poly-host/src/lib.rs`: `kv_get` (`:424`), `kv_set` (`:442`),
  `kv_delete` (`:449`) and `kv_clear` (`:456`) must reject any key that is not
  under the caller's own namespace. For `CallerId::Plugin`, derive the
  `plugin:{id}:…` prefix **server-side** from the verified identity instead of
  the request's `plugin` field (`plugin_kv_key` at `:522` currently takes it as
  an argument).
- [ ] **C.2** Move the credential rows off the general KV surface:
  `ACCOUNT_TOKENS_KEY` (`:631`) and `APP_SETTINGS_KEY` (`:630`) become
  host-internal keys that no `/host/kv/*` request can name. The only reads/
  writes go through the existing typed accessors (`:1021`, `:1124`, `:1142`,
  `:1174`).
- [ ] **C.3** Encrypt `account_tokens` at rest with an OS-keychain-held key
  (`keyring` crate, native only), so a raw SQLite read or a future KV escape
  yields ciphertext. Provide an in-memory `SecretSealer` seam for tests
  (SOLID item 7).
- [ ] **C.4** Strengthen `plugin_kv_cross_plugin_isolation`
  (`apps/poly-host/src/lib.rs:1548` region): the current test only proves two
  callers derive distinct keys. Add a test that plugin A **cannot read** a key
  in plugin B's namespace, and that `/host/kv/get` with `key = "account_tokens"`
  returns an error rather than the token array.
- [ ] **C.5** Re-check the redaction rationale at
  `crates/host-bridge/src/lib.rs:524` (`accounts_list` deliberately omits
  tokens). Once C.1/C.2 land, update that comment to state the redaction is now
  actually load-bearing rather than cosmetic.

## Phase D — Re-narrow the wasmtime plugin sandbox after the 45 → 47 bump — ✅ shipped in this PR

Surfaced by the dependency-upgrade agent, same trust boundary, same plan.

- [x] **D.1** `crates/plugin-host/src/engine.rs:34` `create_engine` sets only
  `wasm_component_model(true)` and `consume_fuel(true)`, inheriting
  `Config::new()` defaults. wasmtime 46 enabled component-model-async + WASI
  0.3.0 by default and 47 enabled the **GC** and **exception-handling**
  proposals by default, so the same source now accepts three proposals it
  previously rejected. Decide explicitly: add `config.wasm_gc(false)` /
  `config.wasm_exceptions(false)` (and any other proposal not required by
  `poly:messenger@0.1.0` guests), or record a dated rationale in the function
  doc for leaving them on. Do not leave the decision implicit.
  → **Decided: deny.** `engine::apply_sandbox_policy` now sets every proposal
  explicitly. Enabled: component-model, bulk-memory, multi-value,
  reference-types, simd, fuel. Denied: gc, function-references, exceptions,
  threads, shared-everything-threads, stack-switching, relaxed-simd, memory64,
  multi-memory, custom-page-sizes, wide-arithmetic, extended-const, tail-call,
  and all nine component-model sub-proposals (async, more-async-builtins,
  async-stackful, threading, error-context, cm-gc, map, fixed-length-lists,
  implements). Setter names verified against the vendored wasmtime 47.0.2
  `src/config.rs`, not guessed. `async_support` was deliberately not called —
  it is `#[deprecated]` and a no-op in 47 (host async is always available), so
  calling it would fail `-D warnings`.
- [x] **D.2** Add a test that instantiating a component using a disabled
  proposal fails — otherwise D.1's narrowing is unverified and silently
  reverts on the next bump.
  → `engine::tests` — nine `rejects_*` tests (gc, typed function references,
  exceptions, shared memory, memory64, multi-memory, tail calls,
  extended-const, custom page sizes), five `accepts_*` tests proving the
  policy is not over-narrowed (mvp, bulk-memory, reference-types, multi-value,
  simd), `accepts_components`, `fuel_metering_is_enabled`,
  `policy_overrides_a_pre_widened_config`, and
  `upstream_defaults_are_wider_than_our_policy` (a stock `Config::new()`
  control that documents *why* the policy exists). Plus
  `registry::tests::plugin_linker_builds_on_the_narrowed_engine`, which builds
  the real WASI-p2 + async host-API linker on the narrowed engine — the guard
  on the one narrowing with a plausible blast radius,
  `wasm_component_model_async(false)`.
- [x] **D.3** `crates/plugin-host/src/registry.rs:175,186,191,216,219,225,340,630`
  all use `drop(store.set_fuel(1_000_000_000))`, discarding the `Result`. A
  failed refuel is invisible and surfaces later as a misleading "all fuel
  consumed" trap on the *next* guest call. Propagate the error (or log at
  `warn` with the call site) so the failure is attributed where it happens.
  `:157` already uses the `?` form — make the rest match.
  → Zero `set_fuel` results are discarded anywhere in the crate. The six sites
  on the instantiation path propagate through a new
  `set_call_fuel(&mut store, site) -> Result<(), String>`. The shared
  `refuel()` helper (which the 46 `IsBackend` call sites and the WS-forwarding
  loop route through) now logs at `warn` with the caller's own
  `file!():line!()`, supplied by a new `refuel!` macro so no call site had to
  grow an argument. The magic `1_000_000_000` is now `engine::CALL_FUEL`.

## Phase E — Supply-chain gate in CI (not applied by the upgrade agent)

The upgrade agent explicitly did not apply this: it is not a dependency bump and
`.github/workflows/` was outside its file ownership. 37 advisory hits are
currently ungated. There is no `cargo-audit` or `cargo-deny` step anywhere in
`.github/workflows/`.

- [x] **E.1** Add `deny.toml` at the repo root with the advisory database
  configured, `unmaintained = "warn"` initially, and an `[advisories.ignore]`
  block carrying the eleven gtk-rs 0.18 RUSTSEC ids **with a pointer to
  `docs/plans/plan-desktop-gtk-stack-bump.md`** so the ignore has an expiry
  story rather than being permanent. — shipped in this PR.
  Two documented deviations from the text above:
  1. `unmaintained = "warn"` is not a legal value any more — cargo-deny
     removed the per-class severity strings in 0.16, and 0.20 accepts
     `"all" | "workspace" | "transitive" | "none"`. `"all"` is used, because
     the gtk-rs cluster is entirely transitive and `"workspace"` would have
     hidden exactly what this gate exists to surface.
  2. Ten gtk-rs ids plus `RUSTSEC-2024-0370` (proc-macro-error) are ignored.
     The eleventh, `RUSTSEC-2024-0429` (glib 0.18.5 soundness), is
     **deliberately absent**: the current advisory-db does not match it
     against our locked glib 0.18.5, so listing it would only emit an
     `advisory-not-detected` warning — and if the db ever does start matching,
     a soundness bug underneath `crates/host-sandbox` (a CLAUDE.md SOLID
     item 7 security boundary) should fail this gate rather than have been
     pre-silenced.
  The file also configures `[licenses]` (permissive allow-list, per-crate
  BUSL-1.1 exceptions for the seven surrealdb crates), `[bans]`
  (`multiple-versions = "deny"` with the 139 currently-locked duplicate
  versions grandfathered entry-by-entry), and `[sources]` (crates.io only).
- [x] **E.2** Add a `cargo deny check advisories bans sources` job to
  `.github/workflows/lint-test.yml` (alongside the `Clippy + Format` job at
  `:16` and the lint gate at `:68`). Non-blocking for one week, then flip to
  blocking. — shipped in this PR as the `supply-chain` job.
  Split into two steps rather than one, so the classes that are already clean
  gate immediately instead of waiting out the grace period:
  - **blocking now:** `cargo deny check bans sources` — verified `bans ok,
    sources ok` locally against the current lock.
  - **non-blocking (`continue-on-error: true`):** `cargo deny check advisories
    licenses` — four findings are open and are deliberately NOT ignored in
    `deny.toml`, because each is a one-line fix rather than accepted risk.
    See E.3 for the flip.
- [ ] **E.3** Delete `continue-on-error` from the `Advisories + licenses` step
  and confirm the job is green. Blocked on four fixes, none of which are in
  `deny.toml`'s or `crates/plugin-host/`'s file ownership:
  - [ ] **E.3a** `cargo update -p ammonia` (≥ 4.1.4) — clears
    `RUSTSEC-2026-0193` (mXSS via MathML `annotation-xml`) and
    `RUSTSEC-2026-0213` (XSS via SVG `animate`/`set`). Locked at 4.1.2.
  - [ ] **E.3b** `cargo update -p crossbeam-epoch` (≥ 0.9.20) — clears
    `RUSTSEC-2026-0204` (invalid pointer dereference in the `fmt::Pointer`
    impl for `Atomic`/`Shared`).
  - [ ] **E.3c** `cargo update -p spin` — clears the yanked-crate error.
  - [ ] **E.3d** Add `license.workspace = true` to
    `crates/ui-types/Cargo.toml`. `poly-ui-types` is the only workspace member
    that is neither `publish = false` nor licence-bearing, so `[licenses]
    private = { ignore = true }` does not cover it and it reports as
    `unlicensed`.
- [ ] **E.4** Once `plan-desktop-gtk-stack-bump.md` Phase C lands, delete the
  gtk-rs ignore block from `deny.toml` and confirm the job is still green.
  (This is the original E.3; renumbered because the `continue-on-error` flip
  is a separate, earlier trigger.)

## Phase F — Verify (QA gate — iterate until a clean round)

- [ ] **F.1** `cargo clippy --workspace -- -D warnings` clean.
- [ ] **F.2** `cargo test --workspace` green; `cargo check -p poly-lint-gate` rc=0
  with **zero baseline entries added**.
- [ ] **F.3** Adversarial re-verification, run from a *browser page on a
  different origin* against a live shell: `/host/exec`, `/host/kv/get` with
  `key=account_tokens`, `/host/kv/set` on `app_settings`, `/host/plugins/add`,
  and `/host/plugin-kv/get` with a forged `plugin` field must all fail. Record
  the actual responses in this file — a green unit suite is not evidence for
  this phase.
- [ ] **F.4** Re-run F.1–F.3 after the last fix. Only tick the plan DONE off a
  round that surfaced nothing new.
