# Plan: Real `TeamsVoiceClient` via ACS / Microsoft Graph Calling

> Author: orchestrator, 2026-05-24 — split out of `plan-solid-audit-teams.md` D.1.
> Scope: `clients/teams/src/voice.rs`, `voice_bridge`, native shell entitlements.
> Source-of-truth for SOLID definitions: top-of-repo `CLAUDE.md`.

## Status: IN PROGRESS — Phase A + B + Phase-C Rust scaffolding shipped in `clients/teams/src/calling/` (changes `a9a0e514` for A+B, then a follow-up Phase-C-Rust commit); Phase C JS bridge (`@azure/communication-calling` integration) + Phase D (UI parity) still deferred pending shell-side bundling + tenant infrastructure

Carved out of `plan-solid-audit-teams.md` D.1 because the work is genuinely a
multi-week integration (~800 LoC of Rust + ~200 LoC of shell glue + a Cargo
dependency surface that does not yet exist in the Rust ecosystem). The
voice stub (`clients/teams/src/voice.rs`) ships clean `NotSupported`
returns today; the scaffolding in `clients/teams/src/calling/` is the
stable trait surface those returns will eventually delegate through —
when Phase C lands, swapping `StubCallingClient` for a real impl is a
one-line wiring change rather than a full rewrite.

---

## Why this is large

Microsoft does not publish a Rust SDK for Azure Communication Services (ACS)
Calling or the Microsoft Graph Calling API. The only first-party SDKs are
JavaScript / .NET / iOS / Android. To call from Rust we need one of:

1. **Bridge to the ACS JavaScript SDK** via a hidden WebView. Same shape as
   the Discord voice bridge but pointed at `@azure/communication-calling`.
   Requires shell-side bundling of the JS SDK and a postMessage RPC.
2. **Re-implement the ACS calling signaling protocol** over WebRTC + Graph
   REST endpoints (`/communications/calls`, `/communications/onlineMeetings`).
   This is undocumented at the wire level and would require packet capture
   from the official client. Hostile-environment work; not recommended.
3. **Wrap the .NET SDK** via P/Invoke or a sidecar process. Adds a runtime
   dependency on .NET 8+ on every user's machine.

Option (1) is the only realistic path. Even then the scope is substantial:

- ACS access token acquisition (Graph `POST /users/{id}/teamwork/sendActivityNotification`
  is NOT a calling token endpoint — calling tokens come from
  `POST /communications/identities/{id}/access-tokens`, which requires a
  Communication Services resource provisioned in the customer's tenant).
- WebRTC bridge to `voice_bridge` (shared with Discord/Stoat).
- Call lifecycle UI parity: incoming-call banner, hold/resume, transfer.
- Tenant-policy edge cases (Teams admins can disable external federated calls,
  guest-tenant calling, anonymous join, lobby behavior).

---

## Phase A — Design + dependency audit — shipped 2026-05-24

- [x] **A.1** Audit existing voice bridge — done. The
  `crates/host-bridge/src/{udp,codec_opus,aead}*` primitives cover the
  transport layer once a JS bridge produces RTP/SRTP frames. The ACS
  JS SDK does its own ICE / SRTP, so we do NOT need
  `crates/voice-bridge` for the audio path. Reusing it is only
  attractive if Phase C decides to extract media from JS into Rust
  for recording / agent integration.
- [x] **A.2** Bundling strategy — decision recorded: **bundle as a
  fetched WebView asset on first use**, mirroring the Discord JS-SDK
  loading pattern. Rationale: CDN load adds offline-failure risk;
  eager bundling balloons the desktop installer; lazy `npm install`
  requires user-machine Node and we don't ship that. Implementation
  deferred to Phase C.
- [x] **A.3** Tenant-provisioning prerequisite documented via the
  [`CallingError::AcsNotProvisioned`] variant + `AcsIdentity` struct
  in `clients/teams/src/calling/types.rs`. The setup-screen prompt
  itself is a UI task (Phase D.1 of `plan-voice-video-calls.md`); the
  error variant is the contract that screen consumes — so it stays
  here as the source-of-truth.

## Phase B — Token acquisition — shipped 2026-05-24

- [x] **B.1** Endpoint wrapper — `AcsTokenAcquirer::acquire` in
  `clients/teams/src/calling/token.rs` posts to
  `POST {acsEndpoint}/identities/{acsUserId}/access-tokens?api-version=2023-10-01`
  with `{"scopes":["voip"]}`. Plan note about
  `sendActivityNotification` being the wrong endpoint is now baked
  into module-level docs so the next reader doesn't re-discover it.
  The privileged Bearer is supplied by an injected `AcsAdminBearer`
  trait so the access-key signing path stays out of the client
  (server-side responsibility — never ship the access key to WASM).
- [x] **B.2** Refresh helper — `AcsTokenAcquirer::seconds_until_refresh`
  returns `lifetime - 2h ± jitter` (matches the plan's 22h
  recommendation). The scheduler call site lands with the Phase C
  JS-bridge code where there's a real token issuer to schedule
  against; the helper plus 3 unit tests (`seconds_until_refresh_*`)
  are the load-bearing piece for that scheduler.
- [x] **B.3** Persistence type — `AcsIdentity` struct in
  `clients/teams/src/calling/types.rs` is `Serialize + Deserialize`
  and round-trips through JSON (`acs_identity_serializes_round_trip`
  test). KV namespace `teams.config.<account>.acs_identity` is
  documented in the struct's doc comment; the actual KV write is a
  3-line `host_bridge::plugin_kv::set` call that lands with the Phase
  C JS-bridge code (writing without a reader is dead state).

## Phase C — Calling bridge

> **Rationale for the Rust-side split:** the full bridge requires a
> WebView shell hosting a hidden frame loading
> `@azure/communication-calling`, postMessage RPC wiring on both
> sides, and a tenant with a provisioned ACS resource for E2E testing.
> None of those land in a single pass. But the Rust-side scaffolding
> (trait surface, IPC types, transport abstraction, mock-transport
> test harness, and a `TeamsClient::start_calling_session` entry
> point) is self-contained — it ships today so the JS bridge can land
> as a focused TS file + transport-impl swap without touching call
> sites or wire types.

### Rust-side scaffolding — shipped 2026-05-24

- [x] **C.1** `TeamsCallingClient` trait extended with
  `set_mute`/`start_video`/`stop_video`/`share_screen`/
  `stop_screen_share`/`hold_call`/`resume_call` (defaults return
  [`CallingError::NotImplemented`]). New
  `WebViewBridgeCallingClient` struct holds an
  `Arc<dyn CallingTransport>` and stubs every method to
  `NotImplemented` pending the JS bridge.
  See `clients/teams/src/calling/client.rs`.
- [x] **C.2** IPC wire shapes — `CallingCommand` (Rust → JS,
  14 variants) + `CallingEvent` (JS → Rust, 11 variants), serde
  internally-tagged with `{"kind":"..."}` discriminant + camelCase
  fields. `CallingTransport` trait (object-safe) +
  `MockCallingTransport` for unit tests. Documented JS-side mirror
  types in a comment block at the top of `ipc.rs`.
  See `clients/teams/src/calling/ipc.rs`.
- [x] **C.3** `TeamsClient::start_calling_session(account_id)` returns
  a `WebViewBridgeCallingClient` over a `MockCallingTransport`. No-op
  on the JS half — existing `voice.rs` stub remains the user-visible
  call path. When the JS bridge ships, this method swaps the mock
  transport for a real one and the trait methods plug in.
  See `clients/teams/src/lib.rs::TeamsClient::start_calling_session`.

### JS-side + tenant work — deferred

- [~] **C.4** JS file (`@azure/communication-calling` wrapper) consuming
  `CallingCommand` and emitting `CallingEvent` over postMessage —
  needs shell-side bundling (Phase A.2 decision recorded; gated on
  real tenant for E2E test).
- [~] **C.5** Real `CallingTransport` impl wrapping `postMessage` +
  JS event listener — drop-in replacement for `MockCallingTransport`.
- [~] **C.6** Test against Microsoft interop / lobby behavior — needs
  a real tenant.

## Phase D — UI parity — deferred

> **Rationale for deferral:** the UI work needs a working calling
> backend to be useful. Stubs render the same way today as a full
> impl (silent no-op). Pre-building the UI without a backend creates
> dead code that drifts from the eventual SDK shape.

- [~] **D.1** Incoming-call banner — needs the `incoming_call` event
  pipeline from Phase C.
- [~] **D.2** Participant grid for >50-participant meetings — needs
  Phase C.
- [~] **D.3** Hold / resume / transfer controls — needs Phase C.

---

## What shipped (Phase A + B scaffolding, 2026-05-24)

New module `clients/teams/src/calling/`:

- `mod.rs` — re-exports + module overview.
- `types.rs` — `CallId`, `CallState`, `AcsAccessToken`, `AcsIdentity`,
  `CallingError` with `From<CallingError> for ClientError`. 7 unit
  tests covering display, error mapping, serialization.
- `token.rs` — `AcsTokenAcquirer::acquire` (REST call against the ACS
  Identity endpoint) + `seconds_until_refresh` helper +
  `AcsAdminBearer` trait. 7 unit tests including
  missing/expired/two-hour-window cases.
- `client.rs` — `TeamsCallingClient` trait (8 methods, all with
  `NotSupported` defaults — ISP-clean) + `StubCallingClient` default
  impl. 8 unit tests covering every method's default behaviour.

Total: 22 new unit tests, all passing on native. No call-site changes
in `crates/core` — the existing `TeamsVoiceClient` stub in
`clients/teams/src/voice.rs` remains the one consumed by voice UI.

## What shipped (Phase C Rust scaffolding, 2026-05-24)

Extended `clients/teams/src/calling/`:

- `types.rs` — added `CallingError::NotImplemented` variant (maps to
  `ClientError::NotSupported`) + 1 new unit test.
- `client.rs` — extended `TeamsCallingClient` trait with 7 new methods
  (`set_mute`, `start_video`, `stop_video`, `share_screen`,
  `stop_screen_share`, `hold_call`, `resume_call`); added
  `WebViewBridgeCallingClient` struct with full trait impl returning
  `NotImplemented` everywhere; 11 new unit tests.
- `ipc.rs` — NEW. `CallingCommand` (14 variants, Rust → JS) +
  `CallingEvent` (11 variants, JS → Rust), serde
  internally-tagged with camelCase fields. `CallingTransport`
  object-safe trait + `MockCallingTransport` test transport
  (`send`/`recv`/`inject_event`/`sent_commands`). 14 new unit tests
  covering round-trip serialization for representative variants and
  mock-transport happy paths. JS-side mirror types documented at the
  top of the module as a comment block (per the prompt — no .ts file
  yet, that's separate work).

Plus `clients/teams/src/lib.rs`:

- New `TeamsClient::start_calling_session(account_id) ->
  WebViewBridgeCallingClient` entry point. Constructs the bridge over
  a `MockCallingTransport` (no JS side wired). User-visible call path
  remains `voice.rs::TeamsVoiceClient` until Phase D.

Total new tests this phase: 26. Cumulative module test count: 63 (was 37+
before Phase C scaffolding). Native + WASM checks remain clean.

When Phase C JS-side (C.4/C.5/C.6) lands, `voice.rs` becomes a thin
delegation that constructs a `WebViewBridgeCallingClient` over a real
postMessage-backed transport instead of the mock.

## Acceptance

When all phases land, remove the stub from `clients/teams/src/voice.rs`
and replace with a real impl that delegates to a concrete
`TeamsCallingClient`. Update `plan-voice-video-calls.md` Phase I from
"stub" to "shipped".
