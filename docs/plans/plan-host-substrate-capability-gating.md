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

## Phase D — Re-narrow the wasmtime plugin sandbox after the 45 → 47 bump

Surfaced by the dependency-upgrade agent, same trust boundary, same plan.

- [ ] **D.1** `crates/plugin-host/src/engine.rs:34` `create_engine` sets only
  `wasm_component_model(true)` and `consume_fuel(true)`, inheriting
  `Config::new()` defaults. wasmtime 46 enabled component-model-async + WASI
  0.3.0 by default and 47 enabled the **GC** and **exception-handling**
  proposals by default, so the same source now accepts three proposals it
  previously rejected. Decide explicitly: add `config.wasm_gc(false)` /
  `config.wasm_exceptions(false)` (and any other proposal not required by
  `poly:messenger@0.1.0` guests), or record a dated rationale in the function
  doc for leaving them on. Do not leave the decision implicit.
- [ ] **D.2** Add a test that instantiating a component using a disabled
  proposal fails — otherwise D.1's narrowing is unverified and silently
  reverts on the next bump.
- [ ] **D.3** `crates/plugin-host/src/registry.rs:175,186,191,216,219,225,340,630`
  all use `drop(store.set_fuel(1_000_000_000))`, discarding the `Result`. A
  failed refuel is invisible and surfaces later as a misleading "all fuel
  consumed" trap on the *next* guest call. Propagate the error (or log at
  `warn` with the call site) so the failure is attributed where it happens.
  `:157` already uses the `?` form — make the rest match.

## Phase E — Supply-chain gate in CI (not applied by the upgrade agent)

The upgrade agent explicitly did not apply this: it is not a dependency bump and
`.github/workflows/` was outside its file ownership. 37 advisory hits are
currently ungated. There is no `cargo-audit` or `cargo-deny` step anywhere in
`.github/workflows/`.

- [ ] **E.1** Add `deny.toml` at the repo root with the advisory database
  configured, `unmaintained = "warn"` initially, and an `[advisories.ignore]`
  block carrying the eleven gtk-rs 0.18 RUSTSEC ids **with a pointer to
  `docs/plans/plan-desktop-gtk-stack-bump.md`** so the ignore has an expiry
  story rather than being permanent.
- [ ] **E.2** Add a `cargo deny check advisories bans sources` job to
  `.github/workflows/lint-test.yml` (alongside the `Clippy + Format` job at
  `:16` and the lint gate at `:68`). Non-blocking for one week, then flip to
  blocking.
- [ ] **E.3** Once `plan-desktop-gtk-stack-bump.md` Phase C lands, delete the
  ignore block from `deny.toml` and confirm the job is still green.

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
