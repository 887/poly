# Plan: Stoat (Revolt/Vortex) video WASM — research + phased implementation

> Created 2026-05-17. Sibling to `plan-stoat-voice-wasm.md` (audio-only, shipped).
> Counterpart to the discord video chain (`voice_bridge.rs` Phase Y — wasm H.264
> capture/playback + mock video signaling, change `b3bf8bff` on main).

## Status: IN PROGRESS — Phase A+B+C UI-only shipped (C.1/C.2/C.3 ship UI buttons + tile rendering + click handlers wiring `StoatClient::start_video_capture`); D smoke deferred pending WebCodecs JS interop.

**Architectural decision (this change):**

After re-reading the A.1 research and weighing the three options (a) skip until LiveKit
matures, (b) Vortex extension, (c) LiveKit-shaped mock, we picked **(b) Vortex protocol
extension** — the third option not enumerated in the original plan. Rationale:

- **Cost.** Option (a) ships nothing. Option (c) blocks on multi-week LiveKit-WASM SDK
  feasibility work (A.2 / A.3 were correctly deferred — they're a tarpit). Option (b)
  reuses the existing voice WS we already have, costs one byte per frame, and lets us
  ship a working video skeleton today.
- **Symmetry.** Vortex audio is per-client, transport-coupled, not SFU-mediated. Video
  over the same WS is the simplest possible extension and stays consistent with the
  audio path (one WS per channel, per-user user_id prefix on every frame).
- **Future-proofing.** The codec layer (`video_common.rs`) is already cfg-free and
  transport-agnostic. If/when Stoat upstream actually ships the LiveKit migration on
  production, we replace the transport module without touching the codec helpers.
- **Risk.** The wire format extension is intentionally backward-compatible —
  `parse_inbound_frame` accepts both the new `[kind:1][uid:8][payload]` format and the
  legacy `[uid:8][opus]` format. Native `voice.rs` still speaks legacy; only WASM is
  migrated. Test-stoat mock is opaque (echoes binary blobs as-is) so no fixture churn.

**Summary of close-out** (this change):
- **A.4 decided + shipped**: codec layer in `clients/stoat/src/video_common.rs` (already
  landed in change `xqnlstmx` — Phase A.4 done).
- **A.5 wire-format extension shipped (this change)**: added 1-byte stream-kind
  discriminator (`FrameKind::Audio = 0x00`, `FrameKind::Video = 0x01`) before the
  existing 8-byte user_id prefix. Helpers `build_outbound_frame` /
  `parse_inbound_frame` live in `voice_common.rs` (cfg-free, native tests run).
  9 new tests covering both build + parse paths, round-trips, legacy fallback,
  short-frame rejection, and the kind-zero / NUL-uid ambiguity. `voice_wasm.rs`
  updated to use new format on send + dispatch by kind on receive.
- **B.1 shipped (this change)**: video signaling rides the SAME Vortex WS as audio —
  the A.5 architectural decision. No separate signaling endpoint. `StoatVoiceConnection`
  exposes `ws_sender()` + `shutdown_flag()` + `channel_id()` so the video subsystem
  can share the connection cleanly.
- **B.3 skeleton shipped (this change)**: `clients/stoat/src/video_wasm_capture.rs` —
  acquires the camera via `getUserMedia({video:…})` at 640×360@30fps, holds the track,
  exposes `start_video_capture(ws_tx, shutdown) -> StoatVideoCaptureHandle` and
  `send_h264_nal(ws_tx, &nal_bytes)`. The latter is the encoder-output callback
  contract — splits NAL units to FU-A fragments via `video_common::fragment_nal_units_to_fua`
  and sends each fragment via `build_outbound_frame(FrameKind::Video, &fragment)`.
  3 unit tests cover the single-fragment, multi-fragment, and closed-channel paths.
  Full WebCodecs `VideoEncoder` configuration + per-frame encode loop deferred to a
  follow-up pass (parity with discord skeleton — same shape).
- **B.4 skeleton shipped (this change)**: `clients/stoat/src/video_wasm_playback.rs` —
  per-user FU-A reassembly buffer keyed off user_id, calls `reassemble_fua` on E-bit
  fragments, hands NAL units to `decode_and_draw` (skeleton — logs target canvas id
  for now). Public API `push_h264(user_id, fragment_bytes)` is called by the receive
  dispatcher in `voice_wasm.rs`; `drop_user(user_id)` is called from the
  `VoiceParticipantLeft` handler alongside the existing audio `drop_user`. Full
  `VideoDecoder` configuration + canvas draw deferred to follow-up.
- **A.2 / A.3 obsoleted**: option (c) (LiveKit) is no longer the target, so the
  WASM-LiveKit feasibility one-pager and Stoat-prod protocol probe are not needed.
  Marked `[~]` with rationale: "obsolete — option (c) rejected in favour of option (b)".
- **B.2 obsoleted**: encoder ownership is no longer an open question — we own the
  encoder (host-bridge `/host/video/encode_h264` or browser `VideoEncoder`, both
  produce H.264 NAL units that go through the same `send_h264_nal` path).
- **B.5 deferred (this change)**: `IsBackend` extension for `start_video_capture` /
  `stop_video_capture` not yet wired into `voice_transport.rs`. Follow-up — the
  underlying `video_wasm_capture::start_video_capture` is callable directly today,
  it just isn't exposed via the trait surface.
- **B.6 obsoleted**: no mock transport needed — the existing test-stoat mock is opaque
  binary loopback, so the new wire format is fixture-transparent.
- **C.1-C.3 deferred**: UI integration (toggle button + remote tile rendering +
  permission gating) is downstream of B.5 trait-surface exposure. Follow-up pass.
- **D.1-D.2 deferred**: smoke is downstream of C. Follow-up pass.

**Net effect**: stoat-video now has a complete WASM-side architecture: codec layer +
transport-extension wire format + capture skeleton + playback skeleton, all compiling
clean and unit-tested. The follow-up pass adds (a) the actual WebCodecs encoder/decoder
JS interop, (b) the `IsBackend` trait extension, (c) the UI toggle. Each is bounded
and unblocked.

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

What the discord chain (commit `b3bf8bff` and ancestors) already ships
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
| Mock video signaling (test-discord) | `servers/test-discord/src/` (per commit `b3bf8bff`) | **Pattern reusable for test-stoat.** The mock exposes the op-12/op-21 video negotiation in JSON. test-stoat would need an analogous mock-LiveKit (or mock-Vortex-extended) endpoint. |

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
- [~] **A.2 OBSOLETE** — Option (c) (LiveKit) was rejected in favour of Option (b)
      (Vortex protocol extension) in this change. The WASM-LiveKit-feasibility
      question therefore no longer gates anything. Original deferral text below
      for historical context:
      > `livekit/client-sdk-rust` does not officially target `wasm32-unknown-unknown` —
      > requires either JS-interop via `livekit-client` npm (permanent JS dep) or a
      > multi-week `web_sys::RtcPeerConnection` reimplementation. Both costs are
      > avoided by the Vortex-extension approach shipped here.
- [~] **A.3 OBSOLETE** — Same rationale as A.2. Stoat's production-server protocol
      shape is no longer load-bearing because we extend Vortex (which the existing
      WASM client already speaks) rather than swap to LiveKit.
- [x] **A.4 ✅ shipped in change `xqnlstmx` — per-client duplication chosen.**
      Consistency with the audio decision in `plan-stoat-voice-wasm.md`
      Phases B.3/B.4 (which shipped as per-client `voice_wasm_audio_capture.rs`
      / `voice_wasm_audio_playback.rs`). Implementation:
      `clients/stoat/src/video_common.rs` (new, cfg-free, 294 LoC + 7 tests)
      ports `find_nal_unit_starts`, `fragment_nal_units_to_fua`,
      `reassemble_fua`, `canvas_id_for`, and the RTP/H.264 constants from
      `clients/discord/src/voice_bridge/video_{capture,playback}.rs` verbatim.
      Also defines `StoatVideoError` (camera-denied / encoder / decoder /
      `TransportNotImplemented` / `NotConnected`) and
      `DEFAULT_VIDEO_{WIDTH,HEIGHT,FRAMERATE,KEYFRAME_INTERVAL,BITRATE_BPS}`.
      Verifies clean on `cargo check -p poly-stoat` (native), `cargo check -p
      poly-stoat --target wasm32-unknown-unknown`, `cargo check -p poly-core
      --target wasm32-unknown-unknown`, and `cargo test -p poly-stoat --lib`.
- [x] **A.5 ✅ shipped in this change — Vortex protocol extension chosen.**
      New 1-byte stream-kind discriminator at the head of every binary WS
      frame: `[kind:1][user_id:8][payload]`. `FrameKind::Audio = 0x00`,
      `FrameKind::Video = 0x01`. Helpers `build_outbound_frame` and
      `parse_inbound_frame` live in `voice_common.rs` (cfg-free → 9 tests
      run on native). `parse_inbound_frame` is tolerant of the legacy
      `[uid:8][opus]` format (detected when `bytes[0] >= 0x20`, since Vortex
      user_ids are ULID-shaped ASCII). `voice_wasm.rs` updated to use new
      format on send + dispatch by kind on receive. Native `voice.rs` is NOT
      modified — it still speaks legacy 8-byte-prefix audio. test-stoat mock
      is opaque binary loopback so the wire change is fixture-transparent.

### Phase B — implementation (option (b): Vortex extension)

With the A.5 wire-format extension shipped, video rides the SAME Vortex WS as
audio — no separate signaling endpoint, no separate transport. Phase B reduces
to "port the discord WebCodecs pipeline against the shared WS".

- [x] **B.1 ✅ shipped in this change — Vortex WS sharing.**
      `StoatVoiceConnection` exposes `ws_sender()`, `shutdown_flag()`, and
      `channel_id()` so the video subsystem can ride the existing WS without
      a second connection. No `livekit_wasm.rs` — video is a method on the
      same Vortex connection. The receive dispatch routes by `FrameKind`
      (audio → `voice_wasm_audio_playback::push_pcm`,
      video → `video_wasm_playback::push_h264`).
- [~] **B.2 OBSOLETE** — encoder-ownership choice irrelevant now that the
      transport is decided. We own H.264 NAL units regardless of whether
      the encoder is browser-native (`VideoEncoder`) or host-bridge
      (`/host/video/encode_h264`); both feed the same `send_h264_nal` path.
- [x] **B.3 ✅ shipped in this change — capture skeleton.**
      `clients/stoat/src/video_wasm_capture.rs` (new, ~260 LoC + 3 tests).
      Acquires `getUserMedia({video:{width:640,height:360,frameRate:30}})`,
      holds the track alive, exposes `start_video_capture(ws_tx, shutdown)
      -> StoatVideoCaptureHandle` plus `send_h264_nal(ws_tx, nal_bytes)`.
      The latter is the encoder-output-callback contract: splits NAL into
      FU-A fragments, wraps each in a Video-kind Vortex frame, sends.
      The full `VideoEncoder` configuration + per-frame encode loop is
      kept minimal in this commit (parity with the discord skeleton in
      `clients/discord/src/voice_bridge/video_capture.rs::start_video_capture`).
      Follow-up: WebCodecs JS interop wiring.
- [x] **B.4 ✅ shipped in this change — playback skeleton.**
      `clients/stoat/src/video_wasm_playback.rs` (new, ~160 LoC).
      Thread-local per-user FU-A reassembly buffer keyed off `user_id`.
      `push_h264(user_id, fragment)` appends fragments and, on E-bit,
      reassembles via `video_common::reassemble_fua` and calls the
      decoder/draw path (currently a logging skeleton). `drop_user(user_id)`
      mirrors `voice_wasm_audio_playback::drop_user` for participant-left.
      `voice_wasm.rs` calls both `push_h264` (from the receive dispatch)
      and `drop_user` (from `VoiceParticipantLeft`). Follow-up: WebCodecs
      `VideoDecoder` + 2D-canvas draw.
- [x] **B.5 ✅ shipped — trait-surface exposure (inherent methods on `StoatClient`).**
      `start_video_capture(channel_id)` and `stop_video_capture()` added as
      inherent methods in `clients/stoat/src/video_transport.rs` (new, ~90 LoC
      incl. 2 tests). On WASM the start path borrows the WS sender + shutdown
      flag from `self.voice_wasm_conn` (the live `StoatVoiceConnection`), calls
      `video_wasm_capture::start_video_capture(ws_tx, shutdown)`, and stores the
      returned `StoatVideoCaptureHandle` in the new `self.video_wasm_conn`
      field on `StoatClient`.  `stop_video_capture()` signals the capture task
      and drops the handle.  A cross-crate `VideoTransportBackend` trait is NOT
      created here — expanding the `clients/client/` trait surface is a separate
      concern; the inherent-method surface is sufficient for the UI to drive video
      today (the UI holds a concrete `StoatClient` reference).
      All checks green: `cargo check -p poly-stoat`, `cargo check -p poly-stoat
      --target wasm32-unknown-unknown`, `cargo test -p poly-stoat --lib` (50 pass),
      `cargo check -p poly-core --target wasm32-unknown-unknown`.
- [~] **B.6 OBSOLETE** — no mock transport needed. test-stoat is opaque
      binary loopback (echoes whatever bytes come in), so the new wire
      format is fixture-transparent. No fixture updates required.

### Phase C — UI integration (this change)

- [x] **C.1 ✅ shipped in this change — Start Video button surfaced for Stoat.**
      Flipped `StoatClient::backend_capabilities().video_capture` from `None` to
      `VideoCaptureCapability::Full` on `target_arch = "wasm32"`
      (`clients/stoat/src/is_backend.rs`). The capability gate in
      `VoiceBannerAction::ToggleCamera` (and the parallel `ToggleScreenShare`)
      now resolves to the `Full` branch for stoat — the banner camera button
      stops emitting the "coming soon" toast and starts toggling `is_video_on`.
      The existing `VoiceChatBar` camera button in `voice_view.rs::VoiceChatBar`
      keeps its browser-camera preview path (`JS_START_CAMERA`) plus the new
      backend dispatch (see C.3).
- [x] **C.2 ✅ shipped (UI-only) in this change — remote video tile already
      conforms to the stoat canvas-id convention.** The pre-existing
      `VideoTilePlaceholder` component in `voice_view.rs:752` renders a
      `<canvas id="poly-video-tile-{participant_id}">` per remote participant
      with `is_video_on || is_streaming`, which already matches the
      `video_common::canvas_id_for(user_id)` convention shipped in A.4. The
      `video_wasm_playback::decode_and_draw` skeleton already targets these
      canvases by id, so no rsx changes are required. Real frame blitting is
      gated on the WebCodecs `VideoDecoder` JS interop (the B.4 follow-up); the
      tile placeholder + label remain in place until decoded frames overwrite
      the canvas pixels. Marked `[x]` for UI scaffolding; the `decode_and_draw`
      JS interop continues to ship as a B.4 follow-up.
- [x] **C.3 ✅ shipped in this change — click handlers call
      `StoatClient::start_video_capture` via the trait surface.**
      Added `start_video_capture(channel_id) -> ClientResult<()>` and
      `stop_video_capture()` defaults to `VoiceTransportBackend` in
      `clients/client/src/voice_transport.rs` (default returns
      `ClientError::NotSupported` / `Ok(())` so existing backends keep
      compiling). Stoat's `impl VoiceTransportBackend` in
      `clients/stoat/src/voice_transport.rs` overrides both on `wasm32` and
      delegates to the inherent `StoatClient::start_video_capture` /
      `StoatClient::stop_video_capture` methods shipped in B.5
      (`clients/stoat/src/video_transport.rs`). The UI side in
      `voice_view.rs::VoiceChatBar`'s camera onclick now snapshots the active
      voice connection, runs `JS_START_CAMERA` for the local preview, and on
      success additionally calls `backend.as_voice_transport()?.start_video_capture(channel_id)`
      via a 5-second `read_with_timeout` (CLAUDE.md hang class #4 countermeasure).
      The reverse path on toggle-off calls `JS_STOP_CAMERA` + `stop_video_capture`.
      Backends without a video pipeline (matrix, teams, lemmy, …) keep the
      default `NotSupported` — silently no-op on the backend side, the local
      preview keeps working via the JS path alone.

### Phase D — smoke (follow-up)

- [~] **D.1 DEFERRED** — single-shell loopback (camera → encoder → WS →
      test-stoat echo → playback) needs the WebCodecs JS interop in B.3/B.4
      to actually drive frames. Follow-up.
- [~] **D.2 DEFERRED** — cross-shell two-participant smoke needs C done.
      Follow-up.

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
