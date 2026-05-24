# Plan: Real `TeamsVoiceClient` via ACS / Microsoft Graph Calling

> Author: orchestrator, 2026-05-24 — split out of `plan-solid-audit-teams.md` D.1.
> Scope: `clients/teams/src/voice.rs`, `voice_bridge`, native shell entitlements.
> Source-of-truth for SOLID definitions: top-of-repo `CLAUDE.md`.

## Status: IN PROGRESS — Phase A + B shipped as Rust scaffolding in `clients/teams/src/calling/`; Phase C (JS bridge) + D (UI parity) deferred pending shell-side bundling + tenant infrastructure

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

## Phase C — Calling bridge — deferred

> **Rationale for deferral:** requires the WebView shell to host a
> hidden frame loading `@azure/communication-calling`, postMessage
> RPC wiring on both sides (Rust ↔ JS), and a tenant with a
> provisioned ACS resource for end-to-end testing. None of these are
> tractable in a single-pass scaffolding session — they need real
> Microsoft tenant credentials and shell-side asset bundling work.
> The trait surface ([`TeamsCallingClient`]) and stub default impl
> ([`StubCallingClient`]) ship today so this work can land as a
> focused swap-the-impl change without touching call sites.

- [~] **C.1** JS-side `connectVoice` / `disconnectVoice` / `setMute` /
  `getParticipants` over postMessage — needs shell-side bundling
  (Phase A.2 decision recorded; implementation gated on real tenant
  for E2E test).
- [~] **C.2** Rust-side real `TeamsCallingClient` impl — needs C.1.
  Trait + stub already shipped (see Phase A/B), so the implementation
  is a swap rather than a refactor.
- [~] **C.3** Test against Microsoft interop / lobby behavior — needs
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
When Phase C lands, `voice.rs` becomes a thin delegation to a
constructed `TeamsCallingClient` impl chosen at backend-construction
time.

## Acceptance

When all phases land, remove the stub from `clients/teams/src/voice.rs`
and replace with a real impl that delegates to a concrete
`TeamsCallingClient`. Update `plan-voice-video-calls.md` Phase I from
"stub" to "shipped".
