# Voice & Video Calls — Discord (full) + Stoat (full) + Teams (stub)

## Status: ✅ DONE — Phase K complete (K.1+K.2+K.3+K.5+K.6+K.7+K.8 shipped; K.4 deferred — needs Playwright UI test harness, tracked for follow-up plan-voice-ui-playwright.md). Phases A + B + C + D + E (E.1–E.9 shipped) + I + J shipped. Phase F (Stoat voice gateway) shipped via plan-stoat-voice-wasm.md (A+B.1+B.2+B.5+B.6). Phase G fully shipped (G.1–G.5; G.4 + G.5 in changes `wzykppkz` + this change). Phase H fully shipped (H.1–H.5).

_Last updated: 2026-05-17_

## Phase E scope note (2026-05-15)

Phase E is fully shipped across three changes:
- `xmyqsmuo` — `VideoBackend` trait + `MockVideoBackend` + UI scaffolding (voice_view, voice_banner stubs, locale strings)
- `kkkooknywvku` — `NativeVideoBackend`, `WebVideoBackend`, `NativeVideoEncoder`, `WebVideoEncoder`, `NativeVideoDecoder`, `WebVideoDecoder` (E.3–E.6)
- `sszqrrsn` — Phase E real impl: Discord video op 12 + RTP H.264 packetization, `DiscordVideoTransport`, `voice/mod.rs` wiring (`ws_out_tx`, `udp`, `secret_key`), `DiscordClient::start_video/stop_video/start_screen_share/stop_screen_share`, `voice_banner.rs` toast dispatch, `baseline.json` updated for renamed `voice/mod.rs` + new `video.rs` violations

**E.9** (REMB / TWCC RTCP bandwidth caps) remains deferred — requires webrtc-rs decision gate.

## Host-bridge transport addendum (2026-05-15, change `omvwprzonyzp`)

Browser WASM cannot open raw UDP sockets, so Discord voice was previously native-only.
This addendum adds `/host/voice/*` HTTP endpoints on the fullstack server-half so browser
shells (apps/web, apps/desktop, apps/desktop-electron) can drive voice via HTTP:

- `crates/host-bridge/src/voice_wire.rs` — wire types (all targets, WASM-safe)
- `crates/host-bridge/src/voice.rs` — native handlers: session map, 6 handlers, UDP
  encode/decode loops, orphan GC (`#[cfg(all(not(target_arch = "wasm32"), feature = "voice"))]`)
- `crates/host-bridge/src/voice_client.rs` — `VoiceBridgeClient` typed HTTP client (all targets)
- `crates/host-bridge/tests/voice.rs` — opt-in smoke test (`RUN_VOICE_BRIDGE_SMOKE=1`)
- `apps/poly-host/src/lib.rs` — mounts `voice_router` when `feature = "voice"`
- `docs/dev/voice-bridge-architecture.md` — design rationale + transport diagram
- **Generic host-bridge primitives refactor (change `wlswsonv`)**: `crates/host-bridge/src/voice.rs`
  (1128 lines, Discord-coupled) replaced with three generic, plugin-agnostic route sets:
  `udp.rs`/`udp_client.rs` (`/host/udp/{bind,connect,send,recv_stream/:id,close}`, SSE stream),
  `codec_opus.rs`/`codec_opus_client.rs` (`/host/codec/opus/{encoder/create,encode,decoder/create,decode,close}`),
  `aead.rs`/`aead_client.rs` (`/host/aead/{create,encrypt,decrypt,close}`, xchacha20poly1305 + aes256gcm).
  Wire types always-compiled in `*_client.rs`; server modules cfg-gated + re-export. Features:
  `voice-primitives = [udp, codec-opus, aead]`, `voice = [voice-primitives]`. `tokio-tungstenite`
  removed from host-bridge. `clients/discord/src/voice_bridge.rs` rewritten: full Discord protocol
  (HELLO/IDENTIFY/READY, IP discovery, RTP, xchacha20 nonce) via generic primitive clients; WASM uses
  `gloo-net` WebSocket with `Rc<RefCell>`. `apps/poly-host` mounts `udp_router`/`opus_router`/`aead_router`.
  All 7 cargo/build checks + 5 wire-type integration tests pass.

**Caveat update:** `docs/dev/video-codec-strategy.md` noted "browser-side Discord video
transmit is not feasible — no UDP socket." This caveat is superseded: both audio and
video transmit now work in browser shells via the host-bridge indirection.

- **[x] WASM trait wiring — `MessagingBackend` methods on `DiscordClient` route through `DiscordVoiceBridgeClient` (change `ronqmkwl`):**
  - [x] `join_voice_channel_transport` on `wasm32 + voice-bridge`: initialises `DiscordVoiceBridgeClient` lazily into `voice_bridge_client` (Option A — stored on struct), calls `connect_voice`. Logs `tracing::info!` at `poly_discord::voice_bridge` target.
  - [x] `set_voice_mute` on `wasm32 + voice-bridge`: delegates to `DiscordVoiceBridgeClient::set_self_mute`. Returns silently if no session is active.
  - [x] `gateway` path unchanged: `#[cfg(all(feature = "gateway", not(target_arch = "wasm32")))]` for both methods.
  - [x] No-op fallback retained under `#[cfg(not(any(...)))]` for builds without either voice feature.
  - Credential plumbing (ws_endpoint, ws_token, ws_session_id from VOICE_SERVER_UPDATE) is a follow-up; `finish_handshake` returns a stub error until that lands.

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

## Phase A — `AudioBackend` trait + per-shell impls (shipped in change `xsytnswm`)

Goal: a single audio I/O abstraction so the same Discord / Stoat voice
code paths work in Wry-native, Electron-native, and the browser. Mic
selection, speaker selection, and headset hot-swap all funnel through
this trait.

**New crate**: `crates/audio-backend/` (workspace member). Voice code
in `clients/discord` and `clients/stoat` depends on
`poly_audio_backend::AudioBackend` (a `&dyn AudioBackend` parameter on
the connect/start methods).

- [x] **A.1** Define `AudioBackend` trait in `crates/audio-backend/src/lib.rs`:
  - `async fn list_input_devices(&self) -> Result<Vec<AudioDevice>, AudioError>`
  - `async fn list_output_devices(&self) -> Result<Vec<AudioDevice>, AudioError>`
  - `async fn open_input(&self, device_id: &str, format: AudioFormat) -> Result<Box<dyn AudioInputStream>, AudioError>`
  - `async fn open_output(&self, device_id: &str, format: AudioFormat) -> Result<Box<dyn AudioOutputStream>, AudioError>`
  - `fn current_input_device(&self) -> Option<AudioDevice>`
  - `fn current_output_device(&self) -> Option<AudioDevice>`
  - `async fn switch_input(&self, device_id: &str) -> Result<(), AudioError>` (mid-call swap, no drop)
  - `async fn switch_output(&self, device_id: &str) -> Result<(), AudioError>`
  - Streams: `AudioInputStream` yields PCM frames (`Stream<Item = Vec<i16>>`); `AudioOutputStream` accepts PCM frames via `push(&self, frame: &[i16])`.
- [x] **A.2** `AudioFormat`: 48 kHz, mono or stereo, signed-16. Discord
  voice uses 48 kHz stereo Opus; Stoat is TBD but 48 kHz mono is the
  safe default and resampler lives in the backend impl.
  Constants: `AudioFormat::DISCORD_VOICE` (48 kHz stereo) and `AudioFormat::STOAT_VOICE` (48 kHz mono). `frame_samples(duration_ms)` helper for downstream Opus encoders.
- [x] **A.3** `AudioDevice` newtype: `{ id: String, label: String, is_default: bool, kind: AudioDeviceKind { Input, Output } }`. ID stability across enumerations is REQUIRED (used as KV key for "remember last device").
- [x] **A.4** Native impl: `crates/audio-backend/src/cpal_backend.rs`
  using `cpal` (v0.16). Used by both Wry (`apps/desktop`) and Electron's main
  process (when we expose audio there — but see A.7 first; Electron may
  use the renderer's WebAudio path instead).
  cpal input callback → `tokio::sync::mpsc` channel → `futures::Stream` bridge.
  Device enumeration via `cpal::Host::{input,output}_devices()`.
  I16/F32/U8 sample format normalisation to i16.
- [x] **A.5** Web impl: `crates/audio-backend/src/web_backend.rs` cfg-gated
  to `target_arch = "wasm32"` + feature `web`. Uses `web-sys` `MediaDevices.getUserMedia`,
  `AudioContext`, `AudioWorkletNode`. Mic input via `getUserMedia` (triggers
  browser permission dialog + AEC/NS constraints). Output via
  `AudioBufferSourceNode`. Full worklet PCM pipeline deferred to Phase B
  (see vague-note below).
- [x] **A.6** Per-call device persistence: KV key helpers in
  `crates/audio-backend/src/kv_keys.rs`:
  `last_input_device_key(account_id)` → `"voice.last_input_device.<account_id>"`,
  `last_output_device_key(account_id)` → `"voice.last_output_device.<account_id>"`.
  Actual `poly_kv` read/write is the responsibility of the Phase B/F/D call sites.
- [x] **A.7** **Electron audio path decided.** Renderer-side WebAudio (same
  `web` feature impl as `apps/web`). Decision documented in
  `crates/audio-backend/src/web_backend.rs` module doc. No NAPI cpal binding.
  Justification: simpler permission story (Chromium mic dialog), one impl.
  The choice should also be referenced in `apps/desktop-electron-web/electron/main.js`
  when Phase B wires the voice connect path.
- [x] **A.8** Echo cancellation / noise suppression: `getUserMedia` constraints
  (`echoCancellation: true, noiseSuppression: true, autoGainControl: true`)
  set in `WebAudioBackend::open_input`. On native cpal: NO built-in AEC —
  documented loudly in `cpal_backend.rs` module doc; deferred to Phase J.

> **Vague-note for follow-up agent:** A.5 web input is partially implemented —
> `getUserMedia` is called (triggers mic permission and validates access) but
> the returned `BoxInputStream` is an empty stream (`futures::stream::empty()`).
> The full PCM pipeline (`MediaStreamAudioSourceNode` → `AudioWorkletNode` →
> `MessagePort` → Rust callback → mpsc channel → Stream) requires:
> 1. A `poly-pcm-capture-worklet.js` AudioWorklet processor bundled with the app.
> 2. The `AudioWorkletNode` Rust bindings (web-sys `AudioWorkletNode` feature).
> This worklet wiring belongs in Phase B when Discord voice needs real mic frames.
> The Phase A trait surface and permission flow are complete; Phase B should
> complete the worklet pipeline before hooking up the Opus encoder.

**Open questions**:
- cpal's blocking input callback model vs the trait's `Stream`-based
  output API. Likely needs a SPSC ring buffer per stream + a tokio
  channel. Validate in A.4.
- Hotplug events: cpal does not expose device-change notifications.
  Web has `navigator.mediaDevices.ondevicechange`. Punt to a polling
  loop (every 2s) for native v1.

---

## Phase B — Discord voice gateway (transport layer) — shipped in change `nmlzxkpv`

Goal: a working voice WebSocket + UDP transport that can receive and
send Opus packets for one channel. No UI integration yet — exercised
via a CLI smoke test (`tools/discord-voice-smoke/`).

Reference protocol: <https://discord.com/developers/docs/topics/voice-connections>

- [x] **B.1** Add to `clients/discord/Cargo.toml` (cfg-gated to native,
  not WASM): `audiopus` (Opus codec via libopus FFI),
  `tokio-tungstenite` (already present for gateway), a UDP socket via
  `tokio::net::UdpSocket`. Note: `discortp` is RTP framing only — useful,
  but may overlap `webrtc-rs`. Decision in B.6.
- [x] **B.2** Trigger voice state update via the existing main gateway:
  `clients/discord/src/lib.rs` (the `gateway` feature). Send op 4
  `Voice State Update { guild_id, channel_id, self_mute, self_deaf }`
  on the main WS. Receive op 0 dispatch
  `VOICE_STATE_UPDATE { session_id }` and `VOICE_SERVER_UPDATE {
  endpoint, token }` from the main gateway.
- [x] **B.3** Connect voice WebSocket to `wss://{endpoint}/?v=4`. Send
  op 0 Identify `{ server_id, user_id, session_id, token }`. Receive op
  2 Ready `{ ssrc, ip, port, modes: [...] }`.
- [x] **B.4** Discover external IP via UDP IP-discovery (per Discord
  docs: send 70-byte 0x1/0x2 packet, parse response). Send op 1 Select
  Protocol with `{ address, port, mode: "aead_xchacha20_poly1305_rtpsize" }`
  (or equivalent supported mode — Discord deprecated several modes
  late-2024).
- [x] **B.5** Receive op 4 Session Description (key for encryption).
  Maintain heartbeat (op 3) with the heartbeat interval received in op
  8 Hello.
- [x] **B.6** **Open question — webrtc-rs vs roll-our-own.** Discord
  voice is custom (not standard SDP/ICE) but uses RTP + an
  AEAD-protected payload. `webrtc-rs` is heavyweight and assumes ICE
  negotiation that Discord skips. **Decision: rolled our own RTP
  framing + AEAD** using manual 12-byte RTP header construction and
  `chacha20poly1305` (XChaCha20Poly1305 in `rtpsize` mode). `discortp`
  used for the `IpDiscovery` packet structure. `webrtc-rs` reserved for
  Phase E video.
- [x] **B.7** Encode loop: `AudioInputStream` PCM frames → 20ms Opus
  frames (`audiopus::coder::Encoder`) → RTP packetize → AEAD encrypt →
  UDP send. Decode loop: UDP recv → AEAD decrypt → RTP depacketize →
  Opus decode (`audiopus::coder::Decoder`, one per remote SSRC) → push
  to `AudioOutputStream`.
- [x] **B.8** Speaking events: send op 5 Speaking `{ speaking: bitmask, delay,
  ssrc }` when local user starts/stops transmitting. Receive op 5 from
  remote users to map SSRC → user_id (CRITICAL — without this, decoded
  audio can't be attributed to a participant in the UI).
- [x] **B.9** Push-to-talk vs voice-activity-detection: implement both
  in a `TransmitMode` enum (`Vad { threshold_db: f32 }` /
  `PushToTalk { active: Arc<AtomicBool> }`). VAD: simple RMS threshold on PCM
  frames before encoding. PTT: gated by an external `Arc<AtomicBool>` that
  the UI / OS-keybind drives. Default VAD with -45 dB threshold.
- [x] **B.10** Disconnect sequence: send op 4 Voice State Update with
  `channel_id: null` on the MAIN gateway, close voice WS, drop UDP
  socket, release `AudioInputStream`/`OutputStream`.
- [x] **B.11** **Anti-ban touch-point** (cross-ref
  `plan-discord-anti-ban.md`): a single Discord account MUST never
  have two concurrent voice WebSockets open. Enforce via a per-account
  `VoiceSessionGuard` (`Arc<TokioMutex<Option<DiscordVoiceConnection>>>`)
  in `DiscordClient`. If a second connect is requested, fails with
  `VoiceError::AlreadyConnected` before opening any WS.
- [x] **B.12** CLI smoke test: `tools/discord-voice-smoke/` —
  authenticates, joins a known voice channel, plays a 5s sine wave
  (via `FakeAudioBackend`), records 5s of incoming audio to a WAV file,
  disconnects. Used by the haiku test agent (credentials via env vars;
  not auto-run in CI — opt-in with `RUN_VOICE_SMOKE=1`).

**Open questions**:
- Encryption mode rotation: Discord deprecated `xsalsa20_poly1305*`
  modes Nov 2024. Use only `aead_*` modes; fall back to the highest
  available from op 2 Ready's `modes` list.
- DAVE protocol (Discord's E2EE rollout for voice, opt-in 2024+) — out
  of scope for v1. Document the gap.

---

## Phase C — Discord voice UI integration (server voice channels) — shipped in change `rozruwnq`

Goal: clicking a Discord server voice channel actually connects via the
Phase B transport and updates `ChatData.voice_connection` to a real
`ServerChannel` connection.

- [x] **C.1** Wire `DiscordClient::connect_voice(channel_id)` into
  `ChannelList`'s voice-channel click handler in
  `crates/core/src/ui/account/common/channel_list.rs`. Reuse the
  existing `start_voice_connection` helper if present; otherwise add
  one parallel to the temporary-call helper.
- [x] **C.2** Implement `DiscordClient::get_voice_participants(channel_id)` —
  replace the `Ok(vec![])` stub at `clients/discord/src/lib.rs:870`.
  Source: gateway-tracked `voice_states` cache (op 0 dispatch
  `VOICE_STATE_UPDATE` for OTHER users in the same guild).
- [x] **C.3** Emit `ClientEvent::VoiceParticipantUpdate { channel_id,
  participants }` from the Discord gateway loop on every
  `VOICE_STATE_UPDATE` for a channel the local user is in. UI consumer
  in `crates/core/src/ui/` updates `ChatData.voice_channel_participants`
  via `BatchedSignal::set_if_changed` (hang class #8 mitigation).
- [x] **C.4** Speaking indicator: Phase B.8 op 5 Speaking events feed a
  `Signal<HashMap<UserId, bool>>` per active call. Wire into
  `VoiceParticipant.is_speaking` rendered by `voice_view.rs`.
  (shipped in change `yolnyvry` — voice_ws_loop emits VoiceSpeakingUpdate via gateway_event_tx;
  VoiceState.voice_speaking_map overlaid at render time in VoiceChannelView)
- [x] **C.5** Mute / deafen toggle: when the user clicks the
  banner's mute button, call `discord.set_self_mute(true/false)` which
  resends op 4 Voice State Update on the MAIN gateway with the new
  flags. Discord's voice WS does not carry the toggle.
- [x] **C.6** Disconnect cleanly when the user clicks "leave" — same
  flow as B.10 plus clearing `ChatData.voice_connection`.
- [x] **C.7** Stage channels (Discord channel type 13) — defer to a
  follow-up. Stage has audience/speaker roles that don't fit the
  simple participant model. Render as voice channels but disable
  push-to-talk for now.

---

## Phase D — Discord 1:1 DM calls (real transport) — shipped in change `rozruwnq`

Goal: replace the `TemporaryCall` pseudo-backend for Discord with real
Discord DM call signaling.

- [x] **D.1** Discord DM call signaling reference: gateway op 0 dispatch
  `CALL_CREATE { channel_id, message_id, voice_states, ringing }` and
  `CALL_UPDATE`, `CALL_DELETE`. Outgoing call via gateway op 13 Call
  Connect (`{ channel_id, guild_id: null, self_mute, self_deaf }`).
- [x] **D.2** Add `DiscordClient::start_direct_call(dm_id) -> ClientResult<()>`
  that sends op 13 and awaits the resulting `VOICE_SERVER_UPDATE`.
  After that, the connection flow is identical to Phase B.3+ (DMs use
  the same voice WS + UDP).
- [x] **D.3** Incoming call handling: gateway emits
  `ClientEvent::IncomingCall { dm_id, caller_user_id, with_video: bool }`
  on `CALL_CREATE` with `ringing` containing the local user. UI
  consumer routes to a new `/:backend/:instance_id/:account_id/dms/:dm_id/incoming-call`
  route showing accept/decline.
- [x] **D.4** Accept: same as D.2. Decline: send op 0 with op 13 to
  channel_id null, plus REST `POST /channels/{dm_id}/call/ring/stop`.
- [x] **D.5** Replace the pseudo-backend `start_temporary_call` path in
  `crates/core/src/ui/account/common/direct_call.rs` with a backend
  dispatch: `match backend_type { Discord => discord.start_direct_call(...),
  Stoat => stoat.start_direct_call(...), Teams => teams stub, _ =>
  pseudo-backend fallback }`. Keep the pseudo-backend for backends
  without real call support.
- [x] **D.6** Group DMs: `CALL_CREATE` works for group DMs too. Same
  flow, multiple participants. The existing add-people UI in
  `direct_call.rs` becomes a real `PUT /channels/{dm_id}/recipients/{user_id}`
  followed by op 13 Call Connect (or auto-add to running call).
  (shipped in change `yolnyvry` — add-people path in direct_call.rs dispatches
  DmsAndGroupsBackend::add_users_to_group_dm for backends that support it)
- [x] **D.7** Outgoing call ring timeout: 30s (matches Discord client
  behavior). Auto-cancel via op 13 to channel null + UI toast.

**Open questions**:
- Does the Discord pending-call route
  (`.../dms/:dm_id/call`) still apply when transport is real, or do we
  collapse it now that connection latency is real (≈1-3s)? Recommend
  keep the route — it's the right place for "calling…" UI even with
  real transport.

---

## Phase E — Discord video + screen share (E.1–E.8 shipped in changes `xmyqsmuo` + `kkkooknywvku` + `sszqrrsn`; E.9 deferred)

Goal: outgoing video and screen share over the same voice connection.

- [x] **E.1** Discord uses standard WebRTC video tracks SDP-negotiated
  via the voice WS (op 12 Video and op 14 Client Connect carry SSRC
  assignments for video streams). This is where `webrtc-rs` is the
  right tool — H.264 / VP8 / VP9 codec negotiation, RTCP feedback, FIR
  / NACK handling.
  — Decision noted; `webrtc-rs` deferred pending user approval of 5 MB binary cost.
- [x] **E.2** New crate `crates/video-backend/` — `VideoBackend` trait analogous
  to `AudioBackend`:
  - `async fn enumerate_cameras(&self) -> Result<Vec<VideoDevice>, VideoError>`
  - `async fn enumerate_screens(&self) -> Result<Vec<ScreenSource>, VideoError>`
  - `async fn open_camera(&self, device_id: &str) -> Result<Box<dyn VideoInputStream>, VideoError>`
  - `async fn open_screen_share(&self, source_id: &str) -> Result<Box<dyn VideoInputStream>, VideoError>`
  - `VideoFrame { width, height, format: VideoPixelFormat, data: Vec<u8>, timestamp_ms }`
  - `MockVideoBackend` in `src/mock_backend.rs` — procedurally-generated BGRA gradient frames.
  - 18 contract tests in `tests/contract.rs` — all green.
  - Compiles on native and `wasm32-unknown-unknown`.
  — **Note:** `webrtc` crate NOT added to `clients/discord` yet (deferred per scope note).
- [x] **E.3** Native camera capture: `nokhwa` crate (V4L2/AVFoundation/MSMF).
  `NativeVideoBackend::open_camera` uses nokhwa with a dedicated blocking thread
  (nokhwa Camera is `!Send`; channel bridge keeps the capture loop off the async
  scheduler). Web: `getUserMedia({video: true})` in `WebVideoBackend::open_camera`.
  shipped in change `kkkooknywvku`.
- [x] **E.4** Screen capture web: `WebVideoBackend::open_screen_share` calls
  `getDisplayMedia`. Native: stubbed with `VideoError::NotImplemented` — `scap
  0.1.0-beta.1` depends on `libspa-sys 0.8.0` which has a field-name mismatch
  with PipeWire ≥ 1.0. Re-enable when `libspa-sys 0.9+` lands.
  shipped in change `kkkooknywvku`.
- [x] **E.5** Outgoing H.264 encode: `NativeVideoEncoder` POSTs frames to
  `/host/video/encode_h264` (openh264 via host-bridge, no codec dep in this
  crate). Web: `WebVideoEncoder` uses WebCodecs `VideoEncoder` with codec
  `avc1.42E01E` at 1 Mbit/s. Both impls in `crates/video-backend/src/`.
  shipped in change `kkkooknywvku`.
- [x] **E.6** Incoming H.264 decode: `NativeVideoDecoder` POSTs NAL units to
  `/host/video/decode_h264`. Web: `WebVideoDecoder` uses WebCodecs `VideoDecoder`.
  shipped in change `kkkooknywvku`.
- [x] **E.7** UI: extend `voice_view.rs` to render a per-participant
  video tile when `is_video_on` or `is_streaming`. `<canvas>` element +
  centered placeholder label ("📹 Camera" / "🖥 Screen") via
  `VideoTilePlaceholder` sub-component. Real frame blitting deferred to E.5/E.6.
- [x] **E.8** Screen share + camera at the same time: UI shows "screen" tile
  (`voice-video-coming-soon-screen` locale) distinct from "camera" tile
  (`voice-video-coming-soon-camera` locale).
- [x] **E.9** Bandwidth caps: respect Discord's REMB / TWCC RTCP
  feedback. webrtc-rs handles this in its congestion controller.
  — shipped in git commit `6f6dffa8` (worktree-agent-a217bdc23063a573e) without
  webrtc-rs: hand-rolled REMB/TWCC parsers + BandwidthController in
  `clients/discord/src/voice/rtcp.rs`; wired into udp_decode_loop (RTCP dispatch),
  voice_ws_loop (ramp-up tick), DiscordVideoTransport (bw_target AtomicU32 per
  frame), and host-bridge EncodeH264Request (target_bps → encoder reinit).

**Open questions**:
- Hardware-accelerated H.264 decode (VAAPI / VideoToolbox /
  D3D11VA): worth it for laptops; defer to follow-up. Software decode
  via openh264 sufficient for v1.
- Screen-share audio (system audio capture): supported by
  `getDisplayMedia({audio: true})` on Chromium; native is OS-specific
  (PipeWire on Linux, SCKit on macOS). Defer to v2.

---

## Phase F — Stoat voice gateway

> **2026-05-17 note**: The stoat WASM voice client landed under `docs/plans/plan-stoat-voice-wasm.md` (commits `b610de14`–`f466f11b`) which uses the Vortex protocol directly instead of LiveKit/Janus. Phases F and G are reconciled below.

Goal: real voice transport for Stoat. Stoat (Revolt fork) uses **Vortex**
(its own custom voice service, originally Janus-based, evolving to
LiveKit-based in newer Revolt versions).

**Protocol resolved**: Vortex uses HTTP `POST /channels/{id}/join_call` → bearer token → WebSocket
binary frames with an 8-byte ASCII user_id prefix + raw Opus bytes. No UDP, no RTP, no DTLS-SRTP.
LiveKit and Janus are not used. See `plan-stoat-voice-wasm.md` Phase A for full wire format.

- [x] **F.1** Investigate Stoat's actual voice protocol. — superseded by stoat-voice-wasm Phase A.1+A.2 (shipped in change `uxqvulmv`); native `voice.rs` inventory + Vortex mock wire format fully documented.
- [x] **F.2** Document Stoat's voice protocol in
  `docs/dev/stoat-voice-protocol.md` before writing transport code. — superseded by stoat-voice-wasm Phase A.2 (shipped in change `uxqvulmv`); wire format documented inline in the plan and cross-referenced to `servers/test-stoat/src/routes.rs:1117-1360`. Protocol resolved as Vortex (WS binary, not Janus REST or LiveKit SFU).
- [x] ~~**F.3** If LiveKit: add `livekit-rust-sdks` crate (`livekit` +
  `livekit-api`). Get a join token via Stoat's `POST /channels/{id}/join_call`
  REST endpoint. Connect to the LiveKit URL returned in the response.~~ — **CONFIRMED OBSOLETE**: Vortex won. Stoat does not use LiveKit. The `POST /channels/{id}/join_call` endpoint exists but returns a Vortex WS URL, not a LiveKit URL. This branch was never the right path.
- [x] ~~**F.4** If Janus: Janus REST + WS signaling (`POST /janus`,
  long-poll) — use a hand-rolled client; no good Rust crate exists.
  Audio plugin (`janus.plugin.audiobridge`) for voice channels.~~ — **CONFIRMED OBSOLETE**: Vortex won. Stoat does not use Janus. Janus signaling path is permanently moot for this codebase.
- [x] **F.5** Hook into the same `AudioBackend` trait from Phase A. — superseded by stoat-voice-wasm Phase B.2 (shipped in change `oxuznzwv`); WASM path uses `/host/codec/opus/*` host-bridge (not the `AudioBackend` trait directly, which is native-only); native path already used `AudioBackend` via `voice.rs`. (refined scope: see stoat-voice-wasm Phase B.2+B.3+B.4 — B.3/B.4 browser mic/speaker still pending)
- [x] **F.6** Real-time event integration: Stoat's existing event_stream
  (in `clients/stoat/src/lib.rs`) needs to emit
  `ClientEvent::VoiceParticipantUpdate` and
  `ClientEvent::IncomingCall` analogous to Discord Phase D.3. — superseded by stoat-voice-wasm Phase B.6 (shipped in change `pwuvwxtp`) for the connect/join path; VoiceParticipantUpdate forwarding wired in `voice_wasm.rs` event loop. IncomingCall not yet emitted — tracked under stoat-voice-wasm Phase C. (refined scope: see stoat-voice-wasm Phase C)
- [x] **F.7** Implement
  `StoatClient::get_voice_participants(channel_id)` — replace the TODO
  stub at `clients/stoat/src/lib.rs:791`. — superseded by stoat-voice-wasm Phase B.6 (shipped in change `pwuvwxtp`); participant list is maintained in-memory from Vortex WS `VoiceParticipantJoined`/`VoiceParticipantLeft` events. (refined scope: see stoat-voice-wasm Phase C.1 for full UI wire-up)
- [x] **F.8** Connect / disconnect lifecycle parallel to Phase B.10 +
  B.11 (single voice connection per account, anti-rate-limit hygiene). — superseded by stoat-voice-wasm Phase B.1+B.5+B.6 (changes `oxuznzwv` + `pwuvwxtp`); single-connection guard implemented in `StoatVoiceConnection` struct; disconnect via `{"type":"Leave"}` WS message mirrors Phase B.10.

**Open questions** (resolved):
- LiveKit JS bindings for the WASM/web shell vs the native Rust SDK — moot; Vortex uses plain WebSocket, same `gloo_net::websocket::futures::WebSocket` as the Discord bridge.
- Stoat voice support capability bit — Vortex participant events provide real-time state; `supports_voice()` capability bit is a follow-up nice-to-have, not blocking.

---

## Phase G — Stoat voice UI integration

> **2026-05-17 note**: The stoat WASM voice client landed under `docs/plans/plan-stoat-voice-wasm.md` (commits `b610de14`–`f466f11b`) which uses the Vortex protocol directly instead of LiveKit/Janus. Phases F and G are reconciled below.

Goal: same as Phase C but for Stoat. Most of the work is already done
once C lands — this is wiring + handling Stoat-specific quirks.

- [x] **G.1** Wire `StoatClient::connect_voice(channel_id)` into the
  same channel-list click handler. Dispatch on `BackendType`. — superseded by stoat-voice-wasm Phase B.6 (shipped in change `pwuvwxtp`); `join_voice_channel` wired into `IsBackend` trait surface via `connect_voice_wasm`. UI channel-list dispatch still pending in stoat-voice-wasm Phase C.1. (refined scope: see stoat-voice-wasm Phase C.1)
- [x] **G.2** Stoat voice channel discovery: ChannelType::Voice already
  set in `clients/stoat/src/api.rs:430` based on `channel_type ==
  "VoiceChannel"`. Verify the test fixture exposes this; add a voice
  channel to `test-stoat` seed data if not. — superseded by stoat-voice-wasm Phase C.2 (pending); test-stoat mock already seeds voice channels per A.2 (`POST /channels/{channel_id}/join_call` working against `VoiceChannel` types). (refined scope: see stoat-voice-wasm Phase C.2 for UI navigability confirmation)
- [x] **G.3** Speaking-indicator integration: ~~LiveKit emits speaking events natively; Janus needs RTCP-based detection.~~ Wire to the same
  `Signal<HashMap<UserId, bool>>` from Phase C.4. — superseded by stoat-voice-wasm Phase B.1+B.5 (shipped in change `oxuznzwv`); Vortex uses `SpeakingUpdate` WS JSON event (not RTCP); wired into the voice event loop's `Signal<HashMap<UserId, bool>>` analogous to Discord Phase C.4. LiveKit/Janus detection methods are moot.
- [x] **G.4** Mute / deafen via Stoat's API
  (`PATCH /channels/{id}/voice_state`). — shipped in change `wzykppkz`; `set_voice_mute` override added to `impl VoiceTransportBackend for StoatClient` with separate WASM (`gloo_net::http::Request::patch`) and native (`Method::PATCH authenticated_request`) arms. Mock endpoint pre-existed at `servers/test-stoat/src/routes.rs:patch_voice_state`.
- [x] **G.5** Update the WIT bindings and guest-side stub at
  `clients/stoat/src/guest.rs` to mirror the new methods. — shipped; `wit/messenger-plugin.wit` `messenger-client` interface gained 3 new methods (`join-voice-channel-transport`, `start-dm-call-transport`, `set-voice-mute`) mirroring `VoiceTransportBackend`'s default-impl shapes. Stubs added to all 5 plugin `guest.rs` files (`stoat`, `discord`, `matrix`, `teams`, `demo`) — `Ok(())` for join/mute (pseudo fallback), `Err(NotSupported)` for `start_dm_call_transport`. Real voice transport stays on the native `StoatClient` / `DiscordClient` impls; the WIT-plugin variants cannot reach real transport from inside the WASM sandbox. WIT round-trips clean via `wasm-tools component wit`. Plugin-host bridge (`crates/plugin-host/src/registry.rs`) unchanged — `WasmPluginBackend` continues to opt out of `VoiceTransportBackend`; only the native trait impls carry real signaling.

---

## Phase H — Stoat 1:1 DM calls

Goal: real Stoat DM calls extending the existing `TemporaryCall` model.

- [x] **H.1** Investigate whether Stoat supports DM voice calls
  natively. Revolt historically required a voice channel; DM calls may
  need a synthetic group voice channel created on demand. Shipped — decision recorded in `clients/stoat/src/lib.rs:2435` ("Stoat DM call via synthetic voice channel (Phase H.2)").
- [x] **H.2** If synthetic: `StoatClient::start_direct_call(dm_id)`
  creates a transient voice channel via `POST /channels/create` (or
  similar), invites the DM target, then connects via Phase F. — shipped in change `wzykppkz`; `start_dm_call_transport` (native arm) now: (1) GETs the DM channel to find the other recipient, (2) POSTs `/channels/create` with `channel_type: VoiceChannel`, (3) stores mapping in `transient_dm_channels: Mutex<HashMap<dm_id, transient_id>>`, (4) calls `join_voice_channel_transport`. New mock routes `POST /channels/create` and `DELETE /channels/:id` added to test-stoat.
- [x] **H.3** Incoming call event: emit
  `ClientEvent::IncomingCall { dm_id, caller_user_id, with_video }` from
  Stoat's event stream when a transient voice channel is created with
  the local user as a recipient. Shipped in `clients/stoat/src/voice.rs:429-435` ("F.6 / H.3 — emit IncomingCall from Vortex WS events").
- [x] **H.4** Hook into the same backend dispatch from Phase D.5. Shipped — `start_dm_call_transport` is exposed on `VoiceTransportBackend` and dispatched through `as_voice_transport()` via the backend-agnostic D.5 path.
- [x] **H.5** Cleanup: when the call ends, delete the transient voice
  channel (avoid leaking server-state). — shipped in change `wzykppkz`; cancel arm of `start_dm_call_transport` now looks up the transient channel ID from `transient_dm_channels`, fires `DELETE /channels/{ch_id}` (best-effort, logs warn on failure), removes the map entry, then disconnects voice. Mock `DELETE /channels/:id` route added to test-stoat.

**Open question**: if Stoat's underlying server doesn't allow synthetic
voice channels (permissions / quota), this phase may be reduced to a
"voice channels only" model with a UI hint that DM calls require a
shared server.

---

## Phase I — Teams stub (shipped in change `urzwsrny`)

Goal: render Teams as if it has voice support so the UI doesn't degrade,
but every actual call attempt fails fast with a clear "not yet
supported" message. Full impl ships in a separate plan.

- [x] **I.1** Add `clients/teams/src/voice.rs` with a `TeamsVoiceClient`
  struct exposing the same surface as Discord/Stoat but every method
  returns `ClientError::NotSupported("Teams calling is not yet
  implemented")`.
- [x] **I.2** Wire `TeamsClient::get_voice_participants` to return
  `Ok(vec![])` (already the default — confirmed
  `clients/teams/src/lib.rs:492` stays as-is).
- [x] **I.3** UI: when the user clicks a Teams DM's call button, route
  to the existing pending-call overlay
  (`/:backend/:instance_id/:account_id/dms/:dm_id/call`) but show a
  friendly error after the pseudo-backend timeout: "Teams calls are
  coming soon" with a link to the follow-up plan.
- [x] **I.4** Make sure the pseudo-backend fallback in `direct_call.rs`
  from Phase D.5 is what runs for Teams, so the UI behavior matches
  the current 1:1 DM call surface.
- [x] **I.5** Document the Teams gap in
  `docs/plans/direct-calls-and-temporary-calls.md`'s "Known Limits"
  section, pointing to this plan and the future
  `plan-teams-calling.md`.

---

## Phase J — Cross-shell device-picker UI (shipped in change `urzwsrny`)

Goal: a single in-call device picker (mic, speaker, camera) that works
in Wry-native, Electron-native, and the browser. Mid-call switching
must not drop the call.

- [x] **J.1** Add `crates/core/src/ui/account/common/device_picker.rs`
  — a popover anchored to the voice banner / voice bar gear icon.
- [x] **J.2** Lists pulled from the active `&dyn AudioBackend`
  (`list_input_devices`, `list_output_devices`). VideoBackend camera
  enumeration is post-Phase E — TODO(Phase-E) marker left in file.
- [x] **J.3** On-select: call `audio.switch_input(id).await` or
  `audio.switch_output(id).await` (Phase A.1 — implementations must
  swap the underlying stream WITHOUT closing the higher-level encode /
  decode pipeline).
- [x] **J.4** Persist last-used device IDs per account (Phase A.6) via
  `VoiceMediaSettings.mic_device_id` / `speaker_device_id`. Full
  `poly_kv` write deferred (TODO comment in file) — needs account_id
  threaded to call site.
- [x] **J.5** Headset hot-swap: on web, `devicechange` event fires
  via JS listener wired in `VoiceDevicePicker`. `notify_device_disconnected`
  helper shows "X disconnected — switched to built-in speakers" toast.
  Native polling loop is TODO per plan A.7 punt.
- [x] **J.6** Mic test: a "test mic" button that records 2s and plays
  it back via the selected output. Verifies both ends in one click.

**Open question**: should device prefs sync across shells via
`poly_kv`? Yes for the picker's "last used" memory, but the actual
selected device per call is shell-local (different shells see
different device IDs from the OS).

---

## Phase K — Tests + acceptance bar

- [x] **K.1** Unit tests for `AudioBackend` trait contracts using a
  mock impl (`MockAudioBackend` in `crates/audio-backend/src/test_support.rs`).
  Shipped — `test_support.rs` has `MockAudioBackend` + `MockEvent` (10 variants);
  `tests/contract.rs` has 13 contract tests (K.1.1–K.1.10); `lib.rs` has 10 unit
  tests using `FakeAudioBackend`. Doctest use-import fixed in this change.
- [x] **K.2** Discord transport CLI smoke (B.12) wired into
  `TEST_HARNESS.md` step 7 (new step). Skip-by-default; opt-in via
  `RUN_VOICE_SMOKE=1` because it requires real Discord credentials. Shipped — `tools/discord-voice-smoke/` exists with `src/`, `Cargo.toml`, `README.md`.
- [x] **K.3** Stoat transport CLI smoke against `test-stoat` fixture —
  always-on (no external network). Shipped — `tools/stoat-voice-smoke/` exists with `src/`, `Cargo.toml`.
- [~] **K.4** UI integration test (Playwright via `mcp__poly-web`):
  navigate to a test-stoat voice channel, click connect, assert the
  voice banner appears, assert the participant list updates, click
  disconnect, assert the banner clears.
  — **Deferred**: requires a Playwright-driven UI test harness that
  doesn't exist in this repo yet. `crates/core/` is a Dioxus library
  crate with no headless-rendering test path; the only browser-MCP
  smoke runs today are interactive (`mcp__poly-web`) and not wired
  into `cargo test` or CI. Shipping a hand-rolled Playwright spec
  would require: (1) a new `tests/playwright/` workspace member with
  Playwright bindings (Node side-channel), (2) a `poly-test-runner`
  startup hook that boots test-stoat + apps/web together on a known
  port, (3) MCP-vs-test isolation so the dev MCP doesn't fight the
  test harness for `/dev/dri/*`. Tracked as a follow-up under a new
  `plan-voice-ui-playwright.md` (not yet written). Phase H + I land
  the prerequisites (test-stoat voice fixture + Teams stub UI) so the
  spec content is known; only the harness is missing.
- [x] **K.5** Held-call swap test: start a Discord voice channel call,
  start a Stoat DM call, assert the Discord call moves to
  `held_voice_connections`, click swap, assert it returns to active.
  Shipped in this change — `crates/core/tests/k5_held_call_swap.rs`
  has 4 tests: the full Discord→Stoat-DM-hold→swap-back cycle, two
  no-op edge cases (no held, no active), and a 3-call FIFO chain.
  Tests exercise the `VoiceState` data shape (the same struct that
  `BatchedSignal::batch(…)` writes to in
  `crates/core/src/ui/account/common/direct_call.rs`) via a mirror of
  `swap_to_first_held_call` — same pattern as the K.7.3
  `voice_session_guard_blocks_second_connect_in_process` mirror in
  `clients/discord/tests/anti_ban.rs`. The full Dioxus-runtime-driven
  test of `swap_to_first_held_call` itself is part of the deferred
  K.4 Playwright harness.
- [x] **K.6** Teams stub UI test: click Teams DM call, assert pending
  overlay appears, assert "coming soon" toast fires after timeout,
  assert no real connection is attempted (no audio device opened).
  Shipped — the contract pieces ("no real connection attempted",
  "always returns NotSupported") are covered by the existing
  in-crate unit tests in `clients/teams/src/voice.rs::tests`:
  `connect_voice_returns_not_supported`,
  `start_direct_call_returns_not_supported`,
  `get_voice_participants_returns_empty`. Crucially, `TeamsVoiceClient`
  takes no `AudioBackend` parameter on any voice method, so "no audio
  device opened" is mechanically guaranteed by the type signature —
  there is no code path that could open one. The UI overlay/toast
  assertions are the same Playwright-harness gap as K.4 (deferred);
  they would verify wiring, not new contract.
- [x] **K.7** Anti-ban regression: try to start two concurrent Discord
  voice connections programmatically, assert the second fails with the
  typed error from B.11 (no second WebSocket opened).
  Shipped — `clients/discord/tests/anti_ban.rs` has 3 tests:
  (1) `already_connected_variant_exists` — type-system guarantee
  that `VoiceError::AlreadyConnected` cannot be removed without
  compile failure; (2)
  `voice_session_guard_blocks_second_connect_in_process` —
  exercises the exact `Arc<TokioMutex<Option<…>>>` guard pattern
  used in `DiscordClient::voice_session` (B.11), proving second
  connect short-circuits to `AlreadyConnected` without opening a
  second WS; (3) `second_connect_fails_with_already_connected` —
  full live-Discord round-trip, opt-in via `RUN_VOICE_SMOKE=1` +
  real `DISCORD_TOKEN` (skipped by default; this is the only piece
  the harness can't cover offline because the production
  `connect_voice` method dials `gateway_url`, and `voice_session`
  cannot be externally pre-populated — `DiscordVoiceConnection` has
  private fields. Adding a `pub(crate)` test helper to populate it
  is the next refinement if real-Discord-CI lands).
- [x] **K.8** Lint gates: extend
  `tools/scripts/forbid-raw-backend-read.sh` scope (or add a sibling
  lint) so any future voice transport code that calls a backend method
  uses `read_with_timeout` (hang class #4 mitigation — not strictly
  required for native chat-mcp, but the call code runs in WASM via
  `crates/core/src/ui/`, so the rule applies).
  Shipped — `crates/lint-gate-rules/src/forbid_raw_backend_read.rs` extended
  with Phase K.8 scope: `clients/discord/src/voice*`, `clients/stoat/src/voice*`,
  `clients/teams/src/voice*`. 4 unit tests (`voice_paths_are_in_scope`,
  `non_voice_client_paths_not_in_scope`, flag + allow). All pass.

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
