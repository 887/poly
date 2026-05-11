# Voice & Video Calls — Discord (full) + Stoat (full) + Teams (stub)

## Status: PLANNED — not started

_Last updated: 2026-05-11_

## Goal

Replace the current pseudo-backend voice/video implementation (see
`docs/plans/direct-calls-and-temporary-calls.md`) with **real backend
transport** for Discord and Stoat, while keeping the existing
`VoiceConnection` / `VoiceParticipant` UI model intact. Teams ships only
as a UI-renderable stub; full ACS / MS Graph calling is a follow-up plan.

This plan extends the model already shipped in `clients/client/src/types/voice.rs`
(`VoiceConnection { kind: VoiceConnectionKind, dm_id, participant_user_ids, … }`)
and the held-call rules in `ChatData.held_voice_connections`. It does
**not** introduce a parallel call subsystem.

## Non-goals (be loud about these)

- **Cross-backend bridging.** A Discord call can't bridge into a Stoat
  call, period. Each `VoiceConnection` is single-account, single-backend.
  Future agents: do not attempt this — the codec, signaling, and
  identity surfaces don't compose. If a user wants to be in two calls at
  once they use the held-call swap mechanism (already shipped).
- **Teams full impl.** Phase I ships only a stub. Real ACS / Microsoft
  Graph calling lives in a follow-up plan (`plan-teams-calling.md`,
  not yet written).
- **Group video conferencing UI** beyond the existing add-people
  affordance. Selective forwarding unit (SFU) support is implicit in
  Discord/Stoat protocols but the per-tile video grid layout is a
  separate UI plan.
- **Recording** of calls. Out of scope for legal/UX reasons.

## Cross-references

- `docs/plans/direct-calls-and-temporary-calls.md` — current 1:1 DM
  pseudo-backend model (DESIGN/REFERENCE doc, the model this plan
  upgrades to real transport).
- `docs/plans/plan-discord-anti-ban.md` — anti-ban touch-points
  referenced in Phase B (concurrent voice connection rule).
- `clients/client/src/types/voice.rs` — `VoiceConnection`,
  `VoiceConnectionKind`, `VoiceParticipant` types — extended (not
  replaced) here.
- `clients/client/src/lib.rs:377-387` — existing
  `get_voice_participants` trait method (default returns `vec![]`).
- `crates/core/src/ui/voice_banner.rs`,
  `crates/core/src/ui/account/common/voice_bar.rs`,
  `crates/core/src/ui/account/common/voice_view.rs`,
  `crates/core/src/ui/account/common/direct_call.rs`,
  `crates/core/src/ui/account/common/direct_call_overlay.rs` —
  existing UI surface to integrate with.
- `clients/discord/src/lib.rs:870`, `clients/discord/src/guest.rs:282` —
  current Discord stub.
- `clients/stoat/src/lib.rs:791` — Stoat TODO at the noted line.
- `clients/stoat/src/api.rs:480-486` — `StoatVoiceInformation`
  (currently only `max_users`; Stoat voice protocol fields TBD in
  Phase F).

---

## Phase A — `AudioBackend` trait + per-shell impls

Goal: a single audio I/O abstraction so the same Discord / Stoat voice
code paths work in Wry-native, Electron-native, and the browser. Mic
selection, speaker selection, and headset hot-swap all funnel through
this trait.

**New crate**: `crates/audio-backend/` (workspace member). Voice code
in `clients/discord` and `clients/stoat` depends on
`poly_audio_backend::AudioBackend` (a `&dyn AudioBackend` parameter on
the connect/start methods).

- [ ] **A.1** Define `AudioBackend` trait in `crates/audio-backend/src/lib.rs`:
  - `async fn list_input_devices(&self) -> Result<Vec<AudioDevice>, AudioError>`
  - `async fn list_output_devices(&self) -> Result<Vec<AudioDevice>, AudioError>`
  - `async fn open_input(&self, device_id: &str, format: AudioFormat) -> Result<Box<dyn AudioInputStream>, AudioError>`
  - `async fn open_output(&self, device_id: &str, format: AudioFormat) -> Result<Box<dyn AudioOutputStream>, AudioError>`
  - `fn current_input_device(&self) -> Option<AudioDevice>`
  - `fn current_output_device(&self) -> Option<AudioDevice>`
  - `async fn switch_input(&self, device_id: &str) -> Result<(), AudioError>` (mid-call swap, no drop)
  - `async fn switch_output(&self, device_id: &str) -> Result<(), AudioError>`
  - Streams: `AudioInputStream` yields PCM frames (`Stream<Item = Vec<i16>>`); `AudioOutputStream` accepts PCM frames via `push(&self, frame: &[i16])`.
- [ ] **A.2** `AudioFormat`: 48 kHz, mono or stereo, signed-16. Discord
  voice uses 48 kHz stereo Opus; Stoat is TBD but 48 kHz mono is the
  safe default and resampler lives in the backend impl.
- [ ] **A.3** `AudioDevice` newtype: `{ id: String, label: String, is_default: bool, kind: AudioDeviceKind { Input, Output } }`. ID stability across enumerations is REQUIRED (used as KV key for "remember last device").
- [ ] **A.4** Native impl: `crates/audio-backend/src/cpal_backend.rs`
  using `cpal`. Used by both Wry (`apps/desktop`) and Electron's main
  process (when we expose audio there — but see A.7 first; Electron may
  use the renderer's WebAudio path instead).
- [ ] **A.5** Web impl: `crates/audio-backend/src/web_backend.rs` cfg-gated
  to `wasm32-unknown-unknown`. Uses `web-sys` `MediaDevices.getUserMedia`,
  `AudioContext`, `AudioWorkletNode`. Mic input via
  `MediaStreamAudioSourceNode` → `AudioWorkletProcessor` posting PCM
  frames over `MessagePort`. Output via `AudioBufferSourceNode` /
  custom worklet.
- [ ] **A.6** Per-call device persistence: store last-used input/output
  device IDs in `poly_kv` under `voice.last_input_device.<account_id>`
  and `voice.last_output_device.<account_id>`. Restore on next call
  open.
- [ ] **A.7** **Open question — Electron audio path.** Electron has both
  a native main process (could use cpal via NAPI) and a renderer with
  WebAudio. Recommended: renderer-side WebAudio, same impl as
  `apps/web`. Justification: simpler permission story (browser already
  prompts mic permission), no NAPI binding needed. Document the choice
  in `apps/desktop-electron-web/electron/main.js` boot path.
- [ ] **A.8** Echo cancellation / noise suppression: rely on
  `getUserMedia` constraints (`echoCancellation: true,
  noiseSuppression: true`) on web; on native cpal there is no built-in
  AEC — defer to Phase J (acceptable to ship without on native v1, but
  document loudly).

**Open questions**:
- cpal's blocking input callback model vs the trait's `Stream`-based
  output API. Likely needs a SPSC ring buffer per stream + a tokio
  channel. Validate in A.4.
- Hotplug events: cpal does not expose device-change notifications.
  Web has `navigator.mediaDevices.ondevicechange`. Punt to a polling
  loop (every 2s) for native v1.

---

## Phase B — Discord voice gateway (transport layer)

Goal: a working voice WebSocket + UDP transport that can receive and
send Opus packets for one channel. No UI integration yet — exercised
via a CLI smoke test (`tools/discord-voice-smoke/`).

Reference protocol: <https://discord.com/developers/docs/topics/voice-connections>

- [ ] **B.1** Add to `clients/discord/Cargo.toml` (cfg-gated to native,
  not WASM): `audiopus` (Opus codec via libopus FFI),
  `tokio-tungstenite` (already present for gateway), a UDP socket via
  `tokio::net::UdpSocket`. Note: `discortp` is RTP framing only — useful,
  but may overlap `webrtc-rs`. Decision in B.6.
- [ ] **B.2** Trigger voice state update via the existing main gateway:
  `clients/discord/src/lib.rs` (the `gateway` feature). Send op 4
  `Voice State Update { guild_id, channel_id, self_mute, self_deaf }`
  on the main WS. Receive op 0 dispatch
  `VOICE_STATE_UPDATE { session_id }` and `VOICE_SERVER_UPDATE {
  endpoint, token }` from the main gateway.
- [ ] **B.3** Connect voice WebSocket to `wss://{endpoint}/?v=4`. Send
  op 0 Identify `{ server_id, user_id, session_id, token }`. Receive op
  2 Ready `{ ssrc, ip, port, modes: [...] }`.
- [ ] **B.4** Discover external IP via UDP IP-discovery (per Discord
  docs: send 70-byte 0x1/0x2 packet, parse response). Send op 1 Select
  Protocol with `{ address, port, mode: "aead_xchacha20_poly1305_rtpsize" }`
  (or equivalent supported mode — Discord deprecated several modes
  late-2024).
- [ ] **B.5** Receive op 4 Session Description (key for encryption).
  Maintain heartbeat (op 3) with the heartbeat interval received in op
  8 Hello.
- [ ] **B.6** **Open question — webrtc-rs vs roll-our-own.** Discord
  voice is custom (not standard SDP/ICE) but uses RTP + an
  AEAD-protected payload. `webrtc-rs` is heavyweight and assumes ICE
  negotiation that Discord skips. **Recommendation: roll our own RTP
  framing + AEAD** using `discortp` (RTP) and `chacha20poly1305` /
  `aes-gcm` (depending on selected mode). Use `webrtc-rs` only for
  Phase E video where standard WebRTC tracks make sense.
- [ ] **B.7** Encode loop: `AudioInputStream` PCM frames → 20ms Opus
  frames (`audiopus::coder::Encoder`) → RTP packetize → AEAD encrypt →
  UDP send. Decode loop: UDP recv → AEAD decrypt → RTP depacketize →
  Opus decode (`audiopus::coder::Decoder`, one per remote SSRC) → push
  to `AudioOutputStream`.
- [ ] **B.8** Speaking events: send op 5 Speaking `{ speaking: bitmask, delay,
  ssrc }` when local user starts/stops transmitting. Receive op 5 from
  remote users to map SSRC → user_id (CRITICAL — without this, decoded
  audio can't be attributed to a participant in the UI).
- [ ] **B.9** Push-to-talk vs voice-activity-detection: implement both
  in a `TransmitMode` enum (`Vad { threshold_db: f32 }` /
  `PushToTalk { keybind: ... }`). VAD: simple RMS threshold on PCM
  frames before encoding. PTT: gated by an external `Signal<bool>` that
  the UI / OS-keybind drives. Default VAD with -45 dB threshold.
- [ ] **B.10** Disconnect sequence: send op 4 Voice State Update with
  `channel_id: null` on the MAIN gateway, close voice WS, drop UDP
  socket, release `AudioInputStream`/`OutputStream`.
- [ ] **B.11** **Anti-ban touch-point** (cross-ref
  `plan-discord-anti-ban.md`): a single Discord account MUST never
  have two concurrent voice WebSockets open. Enforce via a per-account
  mutex in `DiscordClient`. If a second connect is requested, fail
  with a typed error and let the held-call swap UI handle it.
- [ ] **B.12** CLI smoke test: `tools/discord-voice-smoke/` —
  authenticates, joins a known voice channel, plays a 5s sine wave,
  records 5s of incoming audio to a WAV file, disconnects. Used by the
  haiku test agent.

**Open questions**:
- Encryption mode rotation: Discord deprecated `xsalsa20_poly1305*`
  modes Nov 2024. Use only `aead_*` modes; fall back to the highest
  available from op 2 Ready's `modes` list.
- DAVE protocol (Discord's E2EE rollout for voice, opt-in 2024+) — out
  of scope for v1. Document the gap.

---

## Phase C — Discord voice UI integration (server voice channels)

Goal: clicking a Discord server voice channel actually connects via the
Phase B transport and updates `ChatData.voice_connection` to a real
`ServerChannel` connection.

- [ ] **C.1** Wire `DiscordClient::connect_voice(channel_id)` into
  `ChannelList`'s voice-channel click handler in
  `crates/core/src/ui/account/common/channel_list.rs`. Reuse the
  existing `start_voice_connection` helper if present; otherwise add
  one parallel to the temporary-call helper.
- [ ] **C.2** Implement `DiscordClient::get_voice_participants(channel_id)` —
  replace the `Ok(vec![])` stub at `clients/discord/src/lib.rs:870`.
  Source: gateway-tracked `voice_states` cache (op 0 dispatch
  `VOICE_STATE_UPDATE` for OTHER users in the same guild).
- [ ] **C.3** Emit `ClientEvent::VoiceParticipantUpdate { channel_id,
  participants }` from the Discord gateway loop on every
  `VOICE_STATE_UPDATE` for a channel the local user is in. UI consumer
  in `crates/core/src/ui/` updates `ChatData.voice_channel_participants`
  via `BatchedSignal::set_if_changed` (hang class #8 mitigation).
- [ ] **C.4** Speaking indicator: Phase B.8 op 5 Speaking events feed a
  `Signal<HashMap<UserId, bool>>` per active call. Wire into
  `VoiceParticipant.is_speaking` rendered by `voice_view.rs`.
- [ ] **C.5** Mute / deafen toggle: when the user clicks the
  banner's mute button, call `discord.set_self_mute(true/false)` which
  resends op 4 Voice State Update on the MAIN gateway with the new
  flags. Discord's voice WS does not carry the toggle.
- [ ] **C.6** Disconnect cleanly when the user clicks "leave" — same
  flow as B.10 plus clearing `ChatData.voice_connection`.
- [ ] **C.7** Stage channels (Discord channel type 13) — defer to a
  follow-up. Stage has audience/speaker roles that don't fit the
  simple participant model. Render as voice channels but disable
  push-to-talk for now.

---

## Phase D — Discord 1:1 DM calls (real transport)

Goal: replace the `TemporaryCall` pseudo-backend for Discord with real
Discord DM call signaling.

- [ ] **D.1** Discord DM call signaling reference: gateway op 0 dispatch
  `CALL_CREATE { channel_id, message_id, voice_states, ringing }` and
  `CALL_UPDATE`, `CALL_DELETE`. Outgoing call via gateway op 13 Call
  Connect (`{ channel_id, guild_id: null, self_mute, self_deaf }`).
- [ ] **D.2** Add `DiscordClient::start_direct_call(dm_id) -> ClientResult<()>`
  that sends op 13 and awaits the resulting `VOICE_SERVER_UPDATE`.
  After that, the connection flow is identical to Phase B.3+ (DMs use
  the same voice WS + UDP).
- [ ] **D.3** Incoming call handling: gateway emits
  `ClientEvent::IncomingCall { dm_id, caller_user_id, with_video: bool }`
  on `CALL_CREATE` with `ringing` containing the local user. UI
  consumer routes to a new `/:backend/:instance_id/:account_id/dms/:dm_id/incoming-call`
  route showing accept/decline.
- [ ] **D.4** Accept: same as D.2. Decline: send op 0 with op 13 to
  channel_id null, plus REST `POST /channels/{dm_id}/call/ring/stop`.
- [ ] **D.5** Replace the pseudo-backend `start_temporary_call` path in
  `crates/core/src/ui/account/common/direct_call.rs` with a backend
  dispatch: `match backend_type { Discord => discord.start_direct_call(...),
  Stoat => stoat.start_direct_call(...), Teams => teams stub, _ =>
  pseudo-backend fallback }`. Keep the pseudo-backend for backends
  without real call support.
- [ ] **D.6** Group DMs: `CALL_CREATE` works for group DMs too. Same
  flow, multiple participants. The existing add-people UI in
  `direct_call.rs` becomes a real `POST /channels/{dm_id}/recipients/{user_id}`
  followed by op 13 Call Connect (or auto-add to running call).
- [ ] **D.7** Outgoing call ring timeout: 30s (matches Discord client
  behavior). Auto-cancel via op 13 to channel null + UI toast.

**Open questions**:
- Does the Discord pending-call route
  (`.../dms/:dm_id/call`) still apply when transport is real, or do we
  collapse it now that connection latency is real (≈1-3s)? Recommend
  keep the route — it's the right place for "calling…" UI even with
  real transport.

---

## Phase E — Discord video + screen share

Goal: outgoing video and screen share over the same voice connection.

- [ ] **E.1** Discord uses standard WebRTC video tracks SDP-negotiated
  via the voice WS (op 12 Video and op 14 Client Connect carry SSRC
  assignments for video streams). This is where `webrtc-rs` is the
  right tool — H.264 / VP8 / VP9 codec negotiation, RTCP feedback, FIR
  / NACK handling.
- [ ] **E.2** Add `webrtc` crate to `clients/discord` (native only, large
  dep). Build a `VideoBackend` trait analogous to `AudioBackend`:
  - `async fn open_camera(&self, device_id: &str) -> ...`
  - `async fn open_screen_share(&self, source_id: &str) -> ...`
  - `async fn enumerate_screens(&self) -> ...`
- [ ] **E.3** Native camera capture: `nokhwa` crate (cross-platform v4l2
  / AVFoundation / MSMF wrapper). Web: `getUserMedia({video: true})`.
- [ ] **E.4** Screen capture native: `scap` (Wayland/X11/macOS/Win
  unified) — newer than `screencapturekit-rs`/`xcap`. Web:
  `getDisplayMedia()`.
- [ ] **E.5** Outgoing video: encode H.264 via `webrtc-rs` builtin or
  `openh264-rs`. Send via op 12 Video signaling + standard WebRTC RTP
  on the voice UDP socket.
- [ ] **E.6** Incoming video decode: `webrtc-rs` SDP machinery + an
  H.264 decoder (`openh264-rs` or `ffmpeg-next` — decision pending
  binary-size review).
- [ ] **E.7** UI: extend `voice_view.rs` to render a per-participant
  video tile when `is_video_on` or `is_streaming`. Use a `<canvas>` on
  web (decoded frames blitted via `CanvasRenderingContext2D.drawImage`
  on a `VideoFrame`); on native Wry/Electron, surface decoded frames
  via the same canvas path (renderer-side) — keeps one impl.
- [ ] **E.8** Screen share + camera at the same time: Discord allows
  both. Two separate SSRCs. UI shows "screen" tile distinct from
  "camera" tile.
- [ ] **E.9** Bandwidth caps: respect Discord's REMB / TWCC RTCP
  feedback. webrtc-rs handles this in its congestion controller.

**Open questions**:
- Hardware-accelerated H.264 decode (VAAPI / VideoToolbox /
  D3D11VA): worth it for laptops; defer to follow-up. Software decode
  via openh264 sufficient for v1.
- Screen-share audio (system audio capture): supported by
  `getDisplayMedia({audio: true})` on Chromium; native is OS-specific
  (PipeWire on Linux, SCKit on macOS). Defer to v2.

---

## Phase F — Stoat voice gateway

Goal: real voice transport for Stoat. Stoat (Revolt fork) uses **Vortex**
(its own custom voice service, originally Janus-based, evolving to
LiveKit-based in newer Revolt versions).

- [ ] **F.1** Investigate Stoat's actual voice protocol. Read:
  - `clients/stoat/src/api.rs:480-486` — `StoatVoiceInformation`
    (currently only `max_users` exposed; expand fields by inspecting
    Stoat server source).
  - Stoat/Revolt protocol docs at <https://developers.revolt.chat>
    (linked from clients/stoat README if present).
  - Test against the local `test-stoat` fixture
    (`servers/test-stoat/`) to see what voice fields the test server
    exposes.
- [ ] **F.2** Document Stoat's voice protocol in
  `docs/dev/stoat-voice-protocol.md` before writing transport code.
  Open question to resolve: Janus REST signaling, LiveKit SFU + JWT
  auth, or custom WS?
- [ ] **F.3** If LiveKit: add `livekit-rust-sdks` crate (`livekit` +
  `livekit-api`). Get a join token via Stoat's `POST /channels/{id}/join_call`
  REST endpoint. Connect to the LiveKit URL returned in the response.
- [ ] **F.4** If Janus: Janus REST + WS signaling (`POST /janus`,
  long-poll) — use a hand-rolled client; no good Rust crate exists.
  Audio plugin (`janus.plugin.audiobridge`) for voice channels.
- [ ] **F.5** Hook into the same `AudioBackend` trait from Phase A. The
  Phase B Opus encoder is reusable as long as Stoat's transport accepts
  Opus (LiveKit does; Janus AudioBridge does).
- [ ] **F.6** Real-time event integration: Stoat's existing event_stream
  (in `clients/stoat/src/lib.rs`) needs to emit
  `ClientEvent::VoiceParticipantUpdate` and
  `ClientEvent::IncomingCall` analogous to Discord Phase D.3.
- [ ] **F.7** Implement
  `StoatClient::get_voice_participants(channel_id)` — replace the TODO
  stub at `clients/stoat/src/lib.rs:791`.
- [ ] **F.8** Connect / disconnect lifecycle parallel to Phase B.10 +
  B.11 (single voice connection per account, anti-rate-limit hygiene).

**Open questions**:
- LiveKit JS bindings for the WASM/web shell vs the native Rust SDK on
  Wry/Electron. LiveKit publishes both; web can call the JS SDK via
  wasm-bindgen, native uses `livekit-rust-sdks`. Two impls behind one
  trait — see Phase A pattern.
- Stoat doesn't currently expose a "voice support" capability bit
  beyond the `voice` field on channels. Add a backend capability query
  in `clients/client/src/lib.rs` (`fn supports_voice() -> bool`,
  default `false`) and have Stoat return `true` when configured.

---

## Phase G — Stoat voice UI integration

Goal: same as Phase C but for Stoat. Most of the work is already done
once C lands — this is wiring + handling Stoat-specific quirks.

- [ ] **G.1** Wire `StoatClient::connect_voice(channel_id)` into the
  same channel-list click handler. Dispatch on `BackendType`.
- [ ] **G.2** Stoat voice channel discovery: ChannelType::Voice already
  set in `clients/stoat/src/api.rs:430` based on `channel_type ==
  "VoiceChannel"`. Verify the test fixture exposes this; add a voice
  channel to `test-stoat` seed data if not.
- [ ] **G.3** Speaking-indicator integration: LiveKit emits speaking
  events natively; Janus needs RTCP-based detection. Wire to the same
  `Signal<HashMap<UserId, bool>>` from Phase C.4.
- [ ] **G.4** Mute / deafen via Stoat's API
  (`PATCH /channels/{id}/voice_state` or whatever F.2 determines).
- [ ] **G.5** Update the WIT bindings and guest-side stub at
  `clients/stoat/src/guest.rs` to mirror the new methods.

---

## Phase H — Stoat 1:1 DM calls

Goal: real Stoat DM calls extending the existing `TemporaryCall` model.

- [ ] **H.1** Investigate whether Stoat supports DM voice calls
  natively. Revolt historically required a voice channel; DM calls may
  need a synthetic group voice channel created on demand.
- [ ] **H.2** If synthetic: `StoatClient::start_direct_call(dm_id)`
  creates a transient voice channel via `POST /channels/create` (or
  similar), invites the DM target, then connects via Phase F.
- [ ] **H.3** Incoming call event: emit
  `ClientEvent::IncomingCall { dm_id, caller_user_id, with_video }` from
  Stoat's event stream when a transient voice channel is created with
  the local user as a recipient.
- [ ] **H.4** Hook into the same backend dispatch from Phase D.5.
- [ ] **H.5** Cleanup: when the call ends, delete the transient voice
  channel (avoid leaking server-state).

**Open question**: if Stoat's underlying server doesn't allow synthetic
voice channels (permissions / quota), this phase may be reduced to a
"voice channels only" model with a UI hint that DM calls require a
shared server.

---

## Phase I — Teams stub

Goal: render Teams as if it has voice support so the UI doesn't degrade,
but every actual call attempt fails fast with a clear "not yet
supported" message. Full impl ships in a separate plan.

- [ ] **I.1** Add `clients/teams/src/voice.rs` with a `TeamsVoiceClient`
  struct exposing the same surface as Discord/Stoat but every method
  returns `ClientError::NotSupported("Teams calling is not yet
  implemented")`.
- [ ] **I.2** Wire `TeamsClient::get_voice_participants` to return
  `Ok(vec![])` (already the default — confirm
  `clients/teams/src/lib.rs:492` stays as-is).
- [ ] **I.3** UI: when the user clicks a Teams DM's call button, route
  to the existing pending-call overlay
  (`/:backend/:instance_id/:account_id/dms/:dm_id/call`) but show a
  friendly error after the pseudo-backend timeout: "Teams calls are
  coming soon" with a link to the follow-up plan.
- [ ] **I.4** Make sure the pseudo-backend fallback in `direct_call.rs`
  from Phase D.5 is what runs for Teams, so the UI behavior matches
  the current 1:1 DM call surface.
- [ ] **I.5** Document the Teams gap in
  `docs/plans/direct-calls-and-temporary-calls.md`'s "Known Limits"
  section, pointing to this plan and the future
  `plan-teams-calling.md`.

---

## Phase J — Cross-shell device-picker UI

Goal: a single in-call device picker (mic, speaker, camera) that works
in Wry-native, Electron-native, and the browser. Mid-call switching
must not drop the call.

- [ ] **J.1** Add `crates/core/src/ui/account/common/device_picker.rs`
  — a popover anchored to the voice banner / voice bar gear icon.
- [ ] **J.2** Lists pulled from the active `&dyn AudioBackend`
  (`list_input_devices`, `list_output_devices`) and `&dyn VideoBackend`
  (`enumerate_cameras`).
- [ ] **J.3** On-select: call `audio.switch_input(id).await` or
  `audio.switch_output(id).await` (Phase A.1 — implementations must
  swap the underlying stream WITHOUT closing the higher-level encode /
  decode pipeline).
- [ ] **J.4** Persist last-used device IDs per account (Phase A.6) and
  per call kind (`server-channel` vs `direct-call`).
- [ ] **J.5** Headset hot-swap: on web, `devicechange` event fires
  automatically; on native, polling loop from A.7 invalidates stale
  device IDs. UI shows a toast "Headset disconnected — switched to
  built-in speakers" when the active device disappears.
- [ ] **J.6** Mic test: a "test mic" button that records 2s and plays
  it back via the selected output. Verifies both ends in one click.

**Open question**: should device prefs sync across shells via
`poly_kv`? Yes for the picker's "last used" memory, but the actual
selected device per call is shell-local (different shells see
different device IDs from the OS).

---

## Phase K — Tests + acceptance bar

- [ ] **K.1** Unit tests for `AudioBackend` trait contracts using a
  mock impl (`MockAudioBackend` in `crates/audio-backend/src/test_support.rs`).
- [ ] **K.2** Discord transport CLI smoke (B.12) wired into
  `TEST_HARNESS.md` step 7 (new step). Skip-by-default; opt-in via
  `RUN_VOICE_SMOKE=1` because it requires real Discord credentials.
- [ ] **K.3** Stoat transport CLI smoke against `test-stoat` fixture —
  always-on (no external network).
- [ ] **K.4** UI integration test (Playwright via `mcp__poly-web`):
  navigate to a test-stoat voice channel, click connect, assert the
  voice banner appears, assert the participant list updates, click
  disconnect, assert the banner clears.
- [ ] **K.5** Held-call swap test: start a Discord voice channel call,
  start a Stoat DM call, assert the Discord call moves to
  `held_voice_connections`, click swap, assert it returns to active.
- [ ] **K.6** Teams stub UI test: click Teams DM call, assert pending
  overlay appears, assert "coming soon" toast fires after timeout,
  assert no real connection is attempted (no audio device opened).
- [ ] **K.7** Anti-ban regression: try to start two concurrent Discord
  voice connections programmatically, assert the second fails with the
  typed error from B.11 (no second WebSocket opened).
- [ ] **K.8** Lint gates: extend
  `tools/scripts/forbid-raw-backend-read.sh` scope (or add a sibling
  lint) so any future voice transport code that calls a backend method
  uses `read_with_timeout` (hang class #4 mitigation — not strictly
  required for native chat-mcp, but the call code runs in WASM via
  `crates/core/src/ui/`, so the rule applies).

**Acceptance bar (the "is this done?" checklist)**:
- A user on `apps/web` can join a real Discord voice channel, hear
  other participants, be heard, see speaking indicators update, mute /
  unmute, share their screen, see another user's screen, and
  disconnect cleanly.
- Same on `apps/desktop` (Wry) and `apps/desktop-electron`.
- A user can place a Discord 1:1 DM voice call AND a Discord 1:1 DM
  video call.
- Same set of operations against Stoat (modulo Stoat protocol gaps
  surfaced in Phase F).
- Teams DM call attempts gracefully show "coming soon".
- Mid-call mic/speaker/camera switching does not drop the call on any
  shell.
- No regressions in the held-call swap behavior already shipped.
- All eight WASM-hang lint gates remain green; no new
  `// poly-lint: allow` exceptions in voice code without justification
  comments.

---

## Risk register

| Risk | Phase | Mitigation |
|------|-------|------------|
| `webrtc-rs` adds 5+ MB to native binaries | E | Cfg-gate behind a `discord-video` feature; ship audio-only by default |
| Discord deprecates the voice protocol mid-development | B | Pin to v4 of voice WS; track Discord's developer changelog; have a kill-switch flag in client config |
| Stoat's voice protocol is undocumented and changes | F | Phase F.2 documentation step is gating; do not start F.3 until F.2 is reviewed |
| `audiopus` libopus FFI fails on some Linux distros | B | Add a vendored libopus build via `audiopus_sys` feature flag |
| Browser WebAudio worklet permissions break in Electron | A.7 | Test against Electron 28+ early; fall back to ScriptProcessorNode (deprecated but works) if needed |
| Concurrent voice connection from 2 worktrees triggers Discord ban | B.11 | Per-account mutex AND a persistent KV lock keyed by account_id with a 60s TTL |
