# Plan: Stoat (Revolt/Vortex) video WASM — research + phased implementation

> Created 2026-05-17. Sibling to `plan-stoat-voice-wasm.md` (audio-only, shipped).
> Counterpart to the discord video chain (`voice_bridge.rs` Phase Y — wasm H.264
> capture/playback + mock video signaling, change `720c8f32` on main).

## Status: PENDING — opus agent design, ready for implementation

The Vortex-mock path that powers `clients/stoat/src/voice_wasm.rs` is audio-only.
This plan covers adding video to stoat. The headline finding from Phase A.1
research: **Revolt's *legacy* Vortex is audio-only and deprecated; Revolt's
*new* voice (`feat: voice chats v2`, PR
[revoltchat/backend#414](https://github.com/revoltchat/backend/pull/414), merged
on `main`) is a LiveKit-SFU rewrite that supports video, screen-share, and
multi-source publishing.** Stoat's production deployment status is undocumented
(same caveat as `docs/dev/stoat-voice-protocol.md`), but the upstream protocol
direction is unambiguous: **the realistic stoat-video path is LiveKit, not a
custom video-over-Vortex extension.**

## Why this exists

`docs/plans/plan-stoat-voice-wasm.md` brought stoat to audio parity with
discord on wasm32 by porting native `voice.rs` (audiopus + tokio_tungstenite)
to a sibling `voice_wasm.rs` (host-bridge opus + gloo_net::WebSocket) using
the test-stoat mock's Vortex-alike WS protocol. **Stoat video has not been
investigated at all** — the WASM stoat backend cannot publish or receive any
video stream, even against a hypothetical mock.

Discord's wasm video chain (`clients/discord/src/voice_bridge.rs:1580` —
`pub mod video_capture`, `:1824` — `pub mod video_playback`) ships:
`getUserMedia({video:…})` → `MediaStreamTrackProcessor` → `VideoEncoder`
(H.264) → host-bridge `POST /host/video/encode_h264` → RTP-over-UDP with
the negotiated video SSRC. Stoat has nothing analogous.

## Protocol summary — Vortex legacy vs Revolt voice v2 (LiveKit) vs Discord

| Aspect | Discord (shipped) | Stoat Vortex legacy (shipped audio) | Revolt voice v2 / LiveKit (this plan) |
|---|---|---|---|
| Signaling | gateway WS + voice WS | HTTP `POST /channels/{id}/join_call` → bearer token + WS URL | HTTP `POST /channels/{id}/voice/join` → LiveKit JWT + LiveKit server URL |
| Media transport | UDP + RTP + xchacha20_poly1305 AEAD | WS binary frames (8-byte uid prefix + opus) | WebRTC (DTLS-SRTP) — LiveKit SDK manages |
| Video support | Yes (H.264, op 12/21 SSRC negotiation) | **None** | **Yes — camera + screen_share via LiveKit grants** |
| Server side | UDP fan-out | WS broadcast | LiveKit SFU (`livekit-server` daemon) |
| Codec | H.264 (openh264 via host-bridge) | n/a (audio: opus) | LiveKit defaults — VP8 / H.264 / AV1 negotiated; we'd advertise H.264 to reuse host-bridge encoder |
| Auth | DTLS-SRTP + voice WS hello | bearer token in WS query string | LiveKit JWT with `VideoGrant { can_publish_sources: [camera, screen_share, microphone], ... }` |

## Phase A.1 — Web research findings (citations)

All URLs accessed 2026-05-17.

1. **Legacy Vortex is dead.** `revoltchat/vortex` README ([github.com/revoltchat/vortex](https://github.com/revoltchat/vortex)):
   > "DEPRECATED, rewrite on new branch. Please do not use Vortex in any
   > capacity until the rewrite is complete."

   The repo has only two branches (`master`, `vortex`) — no `livekit` branch
   exists in `revoltchat/vortex`; the rewrite moved into the monorepo
   `revoltchat/backend`.

2. **Vortex never had video.** Issue
   [revoltchat/vortex#4](https://github.com/revoltchat/vortex/issues/4)
   ("Add support for screen sharing", opened 2021, closed) — closed without
   implementation. WebSearch summary: *"Vortex is considered legacy software
   and is no longer supported. Currently there is no support for screen
   sharing in a voice chat with Vortex."*

3. **Voice v2 ships with video as a v1 functional requirement.** Tracking
   issue [revoltchat/backend#313](https://github.com/revoltchat/backend/issues/313)
   ("Voice Overhaul and Video Calling"). Ticked v1 requirements include:
   > - [x] Users may share one or more video streams, such as their camera,
   >       a game, or their screen.
   > - [x] Users are able to turn off their video streams at any moment.
   > - [x] A server owner may restrict who can share video by using the
   >       "Video" permission.

4. **Implementation is LiveKit-SFU.** PR
   [revoltchat/backend#414](https://github.com/revoltchat/backend/pull/414)
   ("feat: voice chats v2") body:
   > "Supercedes #318. Porting voice services to livekit."

   Status: closed. PR landed on branch `feat/livekit` → merged to `main`.
   Backend monorepo `main` (accessed via `gh api`) now ships:
   - `crates/delta/src/routes/channels/voice_join.rs` (HTTP join endpoint)
   - `crates/delta/src/routes/channels/voice_stop_ring.rs`
   - `crates/daemons/voice-ingress/` (LiveKit webhook ingest daemon —
     `api.rs`, `guard.rs`, `main.rs`)
   - `crates/core/database/src/voice/voice_client.rs` (token issuance)

5. **Token issuance grants video sources.** `crates/core/database/src/voice/voice_client.rs`
   `VoiceClient::create_token` (per WebFetch on raw.githubusercontent.com):
   - `can_publish: true`
   - `can_publish_data: false`
   - `can_publish_sources: allowed_sources` — populated from a
     `get_allowed_sources()` helper, includes camera / microphone /
     screen_share LiveKit `TrackSource` enum values
   - `can_subscribe: <conditional on Listen permission>`
   - `room_join: true`

6. **Voice-ingress daemon processes video tracks.** PR file analysis
   (`crates/daemons/voice-ingress/src/api.rs`) shows handling for:
   > `if track.r#type == TrackType::Video as i32 { if user_limits.video_resolution[0] != 0 ... }`

   and webhook events `"track_published" | "track_unpublished" |
   "track_unmuted" | "track_muted"` — confirms full video track lifecycle,
   resolution/aspect-ratio enforcement, not audio-only.

7. **Stoat fork status — undocumented.** Per existing
   `docs/dev/stoat-voice-protocol.md` (2026-05-14):
   > "Stoat's internal deployment details are not public. […] Since Stoat's
   > real Vortex deployment is undocumented and potentially still in flux,
   > Phase F implements a test-mock Vortex protocol that exercises the
   > wiring without protocol fidelity."

   It is not publicly verified whether Stoat has shipped the LiveKit
   migration on its production servers, but the protocol surface they will
   adopt (or have adopted) is whatever upstream Revolt's `main` ships,
   which is now LiveKit + video.

### Verdict

**Revolt voice v2 supports video — via LiveKit SFU with JWT grants for
camera/screen_share/microphone publishing.** The legacy Vortex protocol
(which test-stoat mocks and `voice.rs` / `voice_wasm.rs` speak) is
deprecated and audio-only.

For stoat video this gives us **three honest choices**, ranked:

- **Option (a): Skip stoat video until we adopt LiveKit signaling.**
  Defensible. The wasm voice path just landed; the audio surface is
  workable; ROI on a custom video-over-Vortex extension is low because
  upstream is already moving away. Recommended baseline if engineering
  bandwidth is constrained.
- **Option (b): Implement custom video over Vortex WS as a Poly-specific
  extension.** Risky — non-standard, will conflict with upstream when
  Stoat ships LiveKit. Only viable if we can prove Stoat's production
  servers are committed to staying on Vortex indefinitely (no evidence
  for this).
- **Option (c): Implement against a LiveKit-shaped mock now (test-stoat
  Phase G), and a real LiveKit client when WASM `livekit-rust-sdks`
  matures.** Best long-term alignment. Cost: significant — needs a
  WASM-compatible LiveKit client (or a thin WebRTC/PeerConnection
  reimplementation in `web-sys`), test-stoat mock work, and an actual
  LiveKit room in the test runner.

**This plan codifies Option (c) as the target, with Option (a) as the
fallback if Phase B research surfaces a wasm-LiveKit blocker.** Option
(b) is rejected: poor stewardship of a fork that's diverging from upstream.

## Discord WASM video pieces — reuse inventory

What the discord chain (commit `720c8f32` and ancestors) already ships
that stoat-video can reuse, and what is discord-specific:

| Discord asset | Path | Reuse for stoat? |
|---|---|---|
| `/host/video/encode_h264` HTTP endpoint | `crates/host-bridge/src/video.rs` (640 LoC) | **Yes — directly.** Host-bridge is backend-agnostic; the openh264 encoder session model (`session_id` keyed `HashMap`) works for any caller. |
| `/host/video/decode_h264` HTTP endpoint | same file | **Yes — directly.** |
| `/host/video/close_session` | same file | **Yes — directly.** |
| `VideoBridgeClient` (native typed client) | `crates/host-bridge/src/video_client.rs` (150 LoC) | Yes for native callers; WASM uses the HTTP path directly. |
| `getUserMedia({video:…})` → `MediaStreamTrackProcessor` → `VideoEncoder` pipeline | `clients/discord/src/voice_bridge.rs:1580-1820` `pub mod video_capture` | **Partial — extract into a sharable helper.** Currently lives inside discord's `voice_bridge.rs`. Will need extraction into either `clients/common/wasm_video.rs` or copy-adapted into a new `clients/stoat/src/video_wasm_capture.rs`. The pipeline itself (camera → BGRA frame → host-bridge encode → opaque encoded chunk) is fully discord-agnostic; what's discord-specific is the downstream RTP wrapping + AEAD + UDP send. For LiveKit we'd hand encoded chunks to the LiveKit publish API instead. |
| `VideoBridgeHandles` + `start_video_capture` shutdown channel | `voice_bridge.rs:1721` | Pattern reusable; concrete struct is discord-specific (carries RTP SSRC + UDP socket). |
| Video playback (`pub mod video_playback` at `voice_bridge.rs:1824`) | `voice_bridge.rs:1824-1976` | **Partial — same story as capture.** Decode side: `VideoDecoder` (browser) or host-bridge `/host/video/decode_h264` → `<canvas>` or `<video>` element draw. Stoat-side adaptation: same canvas-draw, different upstream packet source (LiveKit subscribe vs discord's RTP/AEAD/UDP recv). |
| op 12 / op 21 video stream negotiation | `voice_bridge.rs:672-720` (`negotiate_video_stream`) | **NOT REUSABLE.** Discord-protocol-specific (RTP SSRC allocation). LiveKit replaces this entirely with the JWT `can_publish_sources` grant + LiveKit room publish API. |
| RTP header building + AEAD for video frames | `clients/discord/src/voice/video.rs` (578 LoC) + `voice_bridge.rs:701-720` | **NOT REUSABLE.** LiveKit handles SRTP and packetization internally; we hand it raw encoded chunks. |
| Mock video signaling (test-discord) | `servers/test-discord/src/` (per commit `720c8f32`) | **Pattern reusable for test-stoat.** The mock exposes the op-12/op-21 video negotiation in JSON. test-stoat would need an analogous mock-LiveKit (or mock-Vortex-extended) endpoint. |

What the existing **stoat** wasm code already ships that video work builds on:

| Stoat asset | Path | Role in video plan |
|---|---|---|
| `voice_common.rs` (shared constants + error enum) | `clients/stoat/src/voice_common.rs` | Add video-side constants here (resolution, bitrate, frame rate) — keep cfg-free. |
| `voice_wasm.rs` (Vortex WS client, opus host-bridge, per-user decoder cache) | `clients/stoat/src/voice_wasm.rs` (462 LoC) | If we go LiveKit (Option c), the WS-based voice path becomes legacy and a new `livekit_wasm.rs` sibling appears. If we go custom-extension (Option b, rejected), video frames could ride the same WS as audio. Either way, voice_wasm.rs is not modified by this plan in Phase A. |
| `voice_wasm_audio_capture.rs` / `voice_wasm_audio_playback.rs` | `clients/stoat/src/` | Pattern templates for the new `video_wasm_capture.rs` / `video_wasm_playback.rs` files (same `MediaStreamTrackProcessor` + `wasm_bindgen_futures::spawn_local` shape, different track kind). |

## Phases — checkbox + jj change-id discipline per CLAUDE.md

### Phase A — research + decision

- [x] **A.1 ✅ shipped in change `xqnlstmx` — web research complete (this commit).**
      Vortex legacy = audio only and deprecated. Revolt voice v2 (on
      `revoltchat/backend` `main`, PR #414) = LiveKit SFU with JWT video
      grants (`can_publish_sources: [camera, screen_share, microphone]`).
      Stoat fork's production deployment status of voice v2 is undocumented
      but upstream direction is unambiguous. Three options enumerated; this
      plan targets **Option (c) — LiveKit-shaped path**, with Option (a)
      as fallback. See "Verdict" section above.
- [ ] **A.2** Investigate WASM-compatible LiveKit client options. Inventory
      what `livekit/client-sdk-rust` ships for `wasm32-unknown-unknown`
      today (probably nothing — likely needs `livekit/client-sdk-js`
      called via `wasm_bindgen` JS interop), or what it would take to
      drive `RTCPeerConnection` directly via `web-sys`. **Output:**
      a one-pager `docs/dev/livekit-wasm-feasibility.md` with a go/no-go
      verdict. If no-go → flip plan to **Option (a)** and document.
- [ ] **A.3** Confirm Stoat production server protocol. If a public
      `/api/voice/join` (or equivalent v2) endpoint is reachable on
      `https://api.stoat.chat` and returns a LiveKit JWT, mark Option (c)
      as production-aligned. If it still returns a legacy Vortex bearer
      token, document the upstream gap and plan a test-only path against
      a mock-LiveKit instead.
- [ ] **A.4** Decide capture/playback module layout: extract discord's
      `voice_bridge::video_capture` + `video_playback` into a shared
      `clients/common/wasm_video.rs` crate **or** duplicate into per-client
      files. Same trade-off as the audio decision (B.3/B.4 in
      `plan-stoat-voice-wasm.md`) — recommendation: **duplicate for now**
      (one more reuse confirms the shared API; ship pressure low).

### Phase B — implementation (gated on A.2 verdict)

Phase B fan-out structure assumes A.2 says "LiveKit-via-JS-interop viable" or
"web-sys WebRTC reimplementation acceptable scope". If A.2 says no-go,
Phase B becomes "document the gap and close the plan as DEFERRED-UPSTREAM".

- [ ] **B.1** Add `clients/stoat/src/livekit_wasm.rs` skeleton: connect to
      the LiveKit room URL using the WASM client identified in A.2,
      authenticate with the JWT, expose `publish_camera()` /
      `unpublish_camera()` / `subscribe_remote_video(participant_id)`
      methods. Mirror discord `voice_bridge::start_video_capture` /
      `stop_video_capture` shape.
- [ ] **B.2** Wire `/host/video/encode_h264` calls (or LiveKit's built-in
      VideoEncoder if the JS SDK handles encode internally — likely the
      case). Path A: use host-bridge encoder, hand raw H.264 NAL units to
      LiveKit publish API as a custom track. Path B: configure LiveKit
      JS SDK to use browser `VideoEncoder` and let it manage codec
      negotiation. Pick during B.2 implementation based on the actual
      LiveKit JS SDK surface.
- [ ] **B.3** Add `clients/stoat/src/video_wasm_capture.rs` — either by
      copy-adapting `voice_bridge.rs:1580-1820` (Option A.4 = duplicate)
      or by importing from `clients/common/wasm_video.rs` (Option A.4 =
      shared crate). `getUserMedia({video:{width:1280,height:720}})` →
      `MediaStreamTrack` → handed to B.1's publish path.
- [ ] **B.4** Add `clients/stoat/src/video_wasm_playback.rs` — receive
      remote video tracks from B.1's subscribe API, attach to a
      `<video>` HTML element (LiveKit JS SDK provides
      `track.attach(element)`) OR draw decoded frames to a `<canvas>`
      via host-bridge `/host/video/decode_h264` (only needed if we go
      Path A in B.2).
- [ ] **B.5** Wire `clients/stoat/src/lib.rs` `IsBackend` trait surface:
      add `join_video_call(channel_id)`, `start_video_capture()`,
      `stop_video_capture()`, `subscribe_video(participant_id)`.
      Discord precedent at `clients/discord/src/lib.rs`.
- [ ] **B.6** Test-stoat mock: add `POST /channels/{id}/voice/join` that
      returns a LiveKit-shape response `{ "token": "<jwt>", "url":
      "ws://localhost:<port>" }`. Either (i) stand up an actual LiveKit
      server in the mock harness (heavy — adds a docker dep) or
      (ii) record a fixture JWT and have the WASM client log "would
      connect to LiveKit at <url>" without actually opening the
      WebRTC connection (minimal smoke). Recommend (ii) for Phase D;
      (i) only if the live smoke needs end-to-end media verification.

### Phase C — UI integration

- [ ] **C.1** Add a "Camera" / "Screen Share" toggle button to the
      in-voice UI for stoat (mirror discord's video UI in
      `crates/core/src/ui/voice_view.rs` or wherever the discord video
      button is rendered).
- [ ] **C.2** Render remote video tiles in the voice view — discord
      precedent for the `<video>` / `<canvas>` element layout applies
      directly. May need a `clients/stoat`-specific tile because the
      participant model differs (Vortex 8-byte uid vs LiveKit
      participant identity string).
- [ ] **C.3** Permission gating: respect the Stoat server's "Video"
      permission (added in voice v2 per issue #313). Surface a
      "Video disabled by server" message when grant denies camera/screen.

### Phase D — smoke

- [ ] **D.1** Live smoke: poly-web stoat account joins a voice channel,
      clicks Camera toggle, console logs show LiveKit publish succeeded,
      mock receives the publish webhook (if Option B.6.i) or logs the
      intended publish (Option B.6.ii). No WASM hang, no infinite
      re-render.
- [ ] **D.2** Cross-shell mutual video: poly-web (otter) publishes camera;
      poly-electron (beaver) subscribes; tile renders in beaver's UI.
      Only meaningful if B.6.i (real LiveKit) is chosen. With B.6.ii
      (fixture), D.2 reduces to "tile placeholder appears with correct
      participant ID".

## Estimated scope

| Phase | LoC delta | Time |
|---|---|---|
| A (research + feasibility) | ~0 (notes + 1 dev doc) | 60-120 min wall, opus agent |
| B (impl) — Path mock-only (Option B.6.ii) | ~600-900 | 3-4 sonnet agents in parallel, 60-120 min each |
| B (impl) — Path real-LiveKit (Option B.6.i) | ~1000-1500 + docker LiveKit harness | unbounded — gated on LiveKit-rust-wasm maturity |
| C (UI) | ~80-150 | 1 sonnet agent, 30-60 min |
| D (smoke) | ~0 | orchestrator, 30-60 min |

**Realistic ship target if A.2 says go and B.6.ii is acceptable:**
1-2 sessions. **If A.2 says no-go (no viable WASM LiveKit client),
plan flips to Option (a) — DEFERRED UPSTREAM — and closes.**

## Open architectural questions for the next agent

1. **A.2 WASM LiveKit client.** Does `livekit/client-sdk-rust` compile
   on `wasm32-unknown-unknown` in 2026, or is JS-interop via
   `livekit-client` npm package the only path? If interop, accept the
   `wasm_bindgen` JS surface as a permanent dependency.
2. **B.2 encoder ownership.** Is it acceptable to let the LiveKit JS SDK
   own video encoding (browser `VideoEncoder` API), or do we need to
   route through `/host/video/encode_h264` for codec-licensing /
   consistency reasons? Defaulting to LiveKit-SDK-owned encoder is
   simpler; the host-bridge encoder is only load-bearing if we ever
   want a non-WebRTC fallback transport.
3. **A.4 capture/playback file layout.** Extract to
   `clients/common/wasm_video.rs` now (a third reuse confirms the API)
   or wait for matrix voice to surface as the third caller? See same
   question for audio in `plan-stoat-voice-wasm.md` decision B.3/B.4 —
   that one shipped as per-client duplication; consistency says do the
   same here.
4. **D scope.** Is "mock-only with fixture JWT" enough for first ship,
   or does the user want a docker-livekit harness in `servers/test-runner`?
   Recommend mock-only first; docker-livekit if/when a real e2e is
   prioritized.
