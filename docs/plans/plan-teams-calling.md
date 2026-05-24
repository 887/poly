# Plan: Real `TeamsVoiceClient` via ACS / Microsoft Graph Calling

> Author: orchestrator, 2026-05-24 — split out of `plan-solid-audit-teams.md` D.1.
> Scope: `clients/teams/src/voice.rs`, `voice_bridge`, native shell entitlements.
> Source-of-truth for SOLID definitions: top-of-repo `CLAUDE.md`.

## Status: NOT STARTED — design only

Carved out of `plan-solid-audit-teams.md` D.1 because the work is genuinely a
multi-week integration (~800 LoC of Rust + ~200 LoC of shell glue + a Cargo
dependency surface that does not yet exist in the Rust ecosystem). The
voice stub (`clients/teams/src/voice.rs`) ships clean `NotSupported`
returns today; that is the right placeholder until this plan lands.

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

## Phase A — Design + dependency audit

- [ ] **A.1** Audit existing voice bridge (`crates/voice-bridge/`,
  `clients/discord/src/voice.rs`) to identify reusable surface.
- [ ] **A.2** Decide bundling strategy for `@azure/communication-calling` —
  ship as bundled JS asset vs CDN load vs lazy npm install on first use.
- [ ] **A.3** Document the tenant-provisioning prerequisite: customers must
  have an ACS resource in their Azure tenant and a configured Communication
  Services User identity mapping. Without this, calling is impossible — the
  plan must surface this as a setup-screen error, not a silent fail.

## Phase B — Token acquisition

- [ ] **B.1** New endpoint wrapper:
  `POST /v1.0/users/{id}/teamwork/sendActivityNotification` → wrong endpoint.
  Correct one: `POST {acsEndpoint}/identities/{id}/access-tokens` with
  scopes `["voip"]`.
- [ ] **B.2** Wire token refresh into the auth scheduler (tokens are 24-hour
  lived; refresh at 22h with jitter).
- [ ] **B.3** Persist ACS user-identity → AAD-user-identity mapping in
  `teams.config.<account>.acs_identity` KV. First-time setup creates the
  ACS identity if missing.

## Phase C — Calling bridge

- [ ] **C.1** JS-side: import `@azure/communication-calling`, expose
  `connectVoice(callId)`, `disconnectVoice()`, `setMute(b)`,
  `getParticipants()` to Rust via postMessage.
- [ ] **C.2** Rust-side: `TeamsVoiceClient::connect_voice` posts the
  bridge request, awaits the response, surfaces errors as
  `ClientError::Network` / `ClientError::AuthFailure`.
- [ ] **C.3** Test against Microsoft's interop / lobby behavior — guests
  may land in lobby, anonymous join may be disabled.

## Phase D — UI parity

- [ ] **D.1** Incoming-call banner (Teams supports inbound — Discord-style
  call ringing UX).
- [ ] **D.2** Participant grid for Teams meetings (up to 1000 participants
  in large meetings; need windowing for >50).
- [ ] **D.3** Hold / resume / transfer controls.

---

## Acceptance

When all phases land, remove the stub from `clients/teams/src/voice.rs` and
replace with real impl. Update `plan-voice-video-calls.md` Phase I from
"stub" to "shipped".
