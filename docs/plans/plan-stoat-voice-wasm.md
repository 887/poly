# Plan: Stoat (Vortex) voice WASM port + mutual-audio cross-shell E2E

> Created 2026-05-17. Counterpart to `plan-voice-media-plane-e2e.md` (Discord
> voice), now landed on main as commits `b610de14` → `0c946c61` → `6d326f3f` →
> `a7ea37e6` (event_stream listener for native restores + voice creds polling
> + gloo_timers boot overlay + dropped user_id filter).

## Status: IN PROGRESS — A.0/A.1/A.2/A.3 decisions landed by opus agent; Phase B ready to dispatch

## Decisions (opus agent, 2026-05-17)

1. **A.3 — sibling file `voice_wasm.rs`** (option **b**). Discord set the precedent
   with `voice_bridge.rs`; stoat's native `voice.rs` is 559 LoC of mature code
   using `tokio_tungstenite`, `audiopus`, `tokio::sync::Mutex`, and `broadcast`
   channels none of which compile on `wasm32-unknown-unknown`. Polyglot-cfg-ing
   that file would require ~30 `#[cfg(target_arch = "wasm32")]` arms inside one
   `connect_voice` body and force any future native fix to think about the
   wasm arm. A sibling file means each platform's loop body reads top-to-bottom
   without `#[cfg]` noise, and the two files can evolve at different speeds.
   Trade-off accepted: ~40 LoC of duplicated boilerplate (constants, error enum,
   `TransmitMode`). Mitigation: hoist `OPUS_FRAME_SAMPLES`, `OPUS_MAX_DECODE_SAMPLES`,
   `DEFAULT_VAD_THRESHOLD_DB`, `TransmitMode`, `VortexServerInfo`,
   `StoatVoiceError`, and `rms_db()` into a new `clients/stoat/src/voice_common.rs`
   module (cfg-free, no native or wasm deps) so both files import from one source.

2. **B.3 / B.4 — duplicate the ~100 LoC of mic/speaker glue per-client** for now.
   Discord and stoat both go through `MediaStreamTrackProcessor` → 48 kHz mono i16
   PCM frames and through `AudioContext` + `AudioBufferSourceNode` for playback,
   but the call sites are tightly coupled to each backend's frame envelope (RTP+AEAD
   for discord, 8-byte uid prefix for stoat). A new `clients/common/wasm_audio.rs`
   crate would need its own `Cargo.toml`, two new dep-graph edges, and would still
   leak the envelope shape via callbacks. Per-client duplication is honest about
   the difference and keeps the worktree-isolated sonnet agents from racing on a
   shared file. Promote to a shared crate if a third WASM voice backend appears
   (matrix has voice on the roadmap — revisit then).

3. **B.5 — per-user `HashMap<UserId, OpusDecoder>` cache** (mirror native voice.rs:285).
   Opus decoders are stateful (entropy coder state, PLC history); a shared decoder
   glitches audibly on talk-overlap because the decoder's predictive model gets
   thrashed between speakers. The HashMap costs ~5 KiB per active user and the
   8-byte ASCII uid prefix on each frame gives us a cheap key. Eviction policy:
   on `VoiceParticipantLeft` event, remove the entry — bounded by the channel's
   participant count.

4. **Phase D scope — live smoke is the only acceptance criterion.** The existing
   native integration test `clients/stoat/tests/integration.rs` already exercises
   the mock's `/join_call` + Vortex WS happy path against `voice.rs`, so the wire
   format is contractually verified on the native side. WASM-pack-test against the
   browser-only `MediaStreamTrackProcessor` and `AudioContext` paths would require
   a headless-chrome harness (~half a day of yak-shave) and would still not cover
   the host-bridge `/host/codec/opus/*` round-trip — which only the live shell
   (poly-web / poly-electron) can exercise end-to-end. Defer wasm-pack-test until
   Phase E surfaces a class of bug that only mock unit-tests can catch.

## Phase B dispatch plan (opus agent, 2026-05-17)

Worktree-isolated **sonnet** agents in parallel, gated on a serial prep step.

**Serial — must run first (this commit covers it):**
- Sibling stub `clients/stoat/src/voice_wasm.rs` (placeholder) — IN THIS COMMIT.
- `lib.rs` declares `pub(crate) mod voice_wasm;` under `#[cfg(target_arch = "wasm32")]` — IN THIS COMMIT.
- Extract `voice_common.rs` (constants + `TransmitMode` + `VortexServerInfo` + `StoatVoiceError`) — **prerequisite serial step before B.1-B.6 fan out.** Estimated 20-30 min sonnet; declare under `pub(crate) mod voice_common;` un-cfg-gated. `voice.rs` then imports from it; `voice_wasm.rs` will too.

**Parallel — all 4 can run simultaneously on disjoint files once `voice_common.rs` exists:**

| Sub-phase | Agent prompt focus | Files touched | DO NOT touch |
|---|---|---|---|
| **B.1+B.5** (signaling + per-user decoder cache) | Implement `connect_voice_wasm` in `voice_wasm.rs`: `gloo_net::websocket::futures::WebSocket` connect, JSON Authenticate, event loop calling `handle_vortex_event` analogous to native, per-user `OpusClient` decoder sessions keyed off 8-byte uid prefix. Mirror discord `voice_bridge.rs` lines 1059-1110 (wasm WS pattern). | `clients/stoat/src/voice_wasm.rs` | `clients/stoat/src/voice.rs`, `lib.rs`, audio_capture pieces. |
| **B.2** (`/host/codec/opus/*` encoder lifecycle) | Add `OpusEncoder` session bring-up via `poly_host_bridge::codec_opus_client::OpusClient::create_encoder(48000, 1, voip)` at top of encode loop; teardown on shutdown. Reuse discord pattern. | `clients/stoat/src/voice_wasm.rs` (encode-loop sub-block) | Anything signaling-related — coordinate via TODO markers if B.1 lands first. |
| **B.3** (browser mic capture) | Copy + adapt `getUserMedia` → `MediaStreamTrackProcessor` → 48 kHz mono i16 PCM stream from `clients/discord/src/voice_bridge/audio_capture.rs`. Output: an `async fn open_mic_stream() -> impl Stream<Item = Vec<i16>>` callable from B.2's encode loop. | New file `clients/stoat/src/voice_wasm_audio_capture.rs` (or sub-mod under `voice_wasm/`) | `voice.rs`. |
| **B.4** (browser speaker playback) | Copy + adapt `AudioContext` + `AudioBufferSourceNode` playback pump from `clients/discord/src/voice_bridge/audio_playback.rs`. One context per remote user, fed i16 PCM frames decoded by B.1. | New file `clients/stoat/src/voice_wasm_audio_playback.rs` | `voice.rs`. |

B.6 (UI/`IsBackend` wire-up) **must serialize after B.1-B.5** since it imports the `voice_wasm::connect_voice_wasm` symbol. A second sonnet agent picks it up once the four parallel agents have all reported committed commit-ids on `worktree-agent-<id>` branches.

Phase C and D run after B.6 ships and is integration-tested via the haiku test-harness.

## Why this exists

`clients/stoat/src/voice.rs:20` declares: *"This entire module is `#[cfg(feature = "voice")]`. The `voice` feature requires `native`. WASM builds of `poly-stoat` MUST NOT enable `voice`."*

After the discord chain landed, the WASM `poly-stoat` build still has NO voice path. apps/web and apps/desktop-electron cannot drive Stoat voice end-to-end with their respective animals (otter / beaver / etc.). This plan brings stoat to parity.

## Protocol summary — Vortex vs Discord

Stoat voice is **dramatically simpler** than Discord voice:

| Aspect | Discord (already shipped) | Stoat (this plan) |
|---|---|---|
| Signaling transport | WebSocket → gateway + voice WS | HTTP POST `/join_call` → bearer token → single WS |
| Media transport | UDP + RTP + AEAD (xchacha20_poly1305) | WS binary frames |
| Frame format | 12-byte RTP header + AEAD ciphertext | 8-byte ASCII user_id (null-padded) + raw Opus bytes |
| Key exchange | DTLS-SRTP-style via SESSION_DESCRIPTION op-4 | Bearer token in WS URL query |
| Fan-out | Server UDP socket relay | Server WS broadcast |
| User mapping | SSRC → user_id via op 5 SPEAKING events | Embedded user_id prefix per frame |

**No DTLS-SRTP. No UDP. No RTP. No xchacha20_poly1305. No SSRC bookkeeping.** Just `ws.send(user_id_8_bytes + opus_bytes)`.

## Discord chain bug-fixes that apply automatically (Phase 3 from user)

The 4 commits we shipped today touched **backend-agnostic** code:

| Commit | Path | Helps stoat? |
|---|---|---|
| `b610de14` event_stream listener for native restores | `crates/core/src/event_stream.rs` + `account_restore.rs` | ✅ YES — stoat is a native account; its `event_stream()` will be polled on restore |
| `0c946c61` voice_server_creds polling | `clients/discord/src/lib.rs` | ❌ no — discord-specific |
| `6d326f3f` boot overlay gloo_timers | `crates/core/src/ui.rs` | ✅ YES — overlay is global; stoat account on the boot path benefits |
| `a7ea37e6` drop user_id filter | `clients/discord/src/gateway_bridge.rs` | ❌ no — discord-specific |

**Verification work for Phase A.0:** confirm a stoat account on the cold-boot path no longer wedges behind the boot overlay AND that stoat-side ClientEvents flow through the now-spawned event_stream listener. Likely a single bash + browser-eval smoke test, ~5 min.

## Phases — checkbox + jj change-id discipline per CLAUDE.md

### Phase A — verification + protocol gap analysis

- [x] **A.0 ✅ shipped in change `uxqvulmv` — verified.** Live poly-web (CDP /devtools/page/B6081A99…) probed: title `"Poly — PolyGlot Messenger"`, no boot-overlay element in DOM (`overlayDisplay: "no-overlay"`, `overlayVisible: false`). SQLite `~/.local/share/poly/storage.sqlite3` `account_tokens` row contains 2 stoat accounts (`Stoat` + `Raccoon` on `http://localhost:9101`). `crates/core/src/account_restore.rs:311` calls `spawn_event_stream_listener` unconditionally for every restored backend (slug-agnostic), so the discord chain commit `b610de14` automatically helps stoat — no stoat-specific change needed. `6d326f3f`'s `gloo_timers`-based overlay dismissal is also global. No stoat-tied console warnings.
- [x] **A.1 ✅ shipped in change `uxqvulmv` — native-only inventory complete.** See "A.1 native-only call inventory" sub-section below.
- [x] **A.2 ✅ shipped in change `uxqvulmv` — mock wire format documented.** See "A.2 Vortex mock wire format" sub-section below.
- [x] **A.3 ✅ shipped in change `uxqvulmv` — decision (b) sibling file `voice_wasm.rs`.** See "Decisions" section above for rationale. Stub at `clients/stoat/src/voice_wasm.rs` (placeholder), declared in `lib.rs` under `#[cfg(target_arch = "wasm32")]`.

### A.1 native-only call inventory

`clients/stoat/src/voice.rs` (559 LoC). Native-only call sites and their proposed WASM substitutes:

| Native dependency / call | Where (line) | WASM substitute |
|---|---|---|
| `use audiopus::coder::{Decoder, Encoder}` (Opus FFI) | 47-52 | `poly_host_bridge::codec_opus_client::OpusClient` — discord's `voice_bridge` already uses this; sessions live in the native fullstack process, host-bridge HTTP. |
| `audiopus::packet::Packet::try_from(opus_data)` | 319 | Not needed — host-bridge takes raw opus bytes; `OpusClient::decoder_decode(session_id, bytes)` validates internally. |
| `audiopus::MutSignals::try_from(&mut pcm[..])` | 323 | Not needed (host-bridge returns owned `Vec<i16>`). |
| `use tokio::sync::{broadcast, mpsc, Mutex as TokioMutex}` | 54-56 | `tokio::sync::mpsc`/`broadcast`/`Mutex` all work on wasm32 (channel-only, no `Instant`). **No substitute required.** |
| `use tokio_tungstenite::{connect_async, tungstenite::Message}` | 57 | `gloo_net::websocket::futures::WebSocket` + `gloo_net::websocket::Message` — same pattern as `clients/discord/src/voice_bridge.rs` lines 1059-1077 (wasm path). |
| `tokio::spawn(async move {…})` (3 sites: ws-write, ws-event, encode) | 258, 284, 361 | `wasm_bindgen_futures::spawn_local` — discord uses the same on wasm32. Tasks become `!Send`. |
| `audio.open_input("", AudioFormat::STOAT_VOICE)` (`poly_audio_backend`) | 238-241 | `MediaStreamTrackProcessor` pipeline lifted from `clients/discord/src/voice_bridge/audio_capture.rs` (decision B.3). |
| `audio.open_output("", AudioFormat::STOAT_VOICE)` | 242-245 | `AudioContext` + `AudioBufferSourceNode` from `clients/discord/src/voice_bridge/audio_playback.rs` (decision B.4). |
| `tokio::time::sleep` / `Instant::now()` — **NOT USED** in this file | n/a | n/a (no migration needed; hang-class #4 avoided naturally). |
| `OpusEncoder::new(...).encode(pcm_slice, &mut opus_out)` | 362-372, 389 | `OpusClient::create_encoder(48000, 1, Voip)` → `encoder_encode(session_id, pcm)`. |
| `OpusDecoder::new(...).decode(packet, mut_signals, false)` | 313-316, 327 | `OpusClient::create_decoder(48000, 1)` → `decoder_decode(session_id, opus_bytes)`. Per-user `HashMap<String, session_id>` keyed off 8-byte uid prefix (decision B.5). |
| `connect_async(&ws_url).await` returning `(WebSocketStream, _)` | 223-225 | `WebSocket::open(&ws_url)` (gloo_net) — `.split()` for sink/stream. |
| `TMsg::Text(...)` / `TMsg::Binary(...)` | 232-233, 350, 395 | `gloo_net::websocket::Message::Text(String)` / `::Bytes(Vec<u8>)`. |

Pure data items that already compile on wasm32 and should be hoisted to `voice_common.rs` (serial prep step):
`OPUS_FRAME_SAMPLES`, `OPUS_APP`, `DEFAULT_VAD_THRESHOLD_DB`, `OPUS_MAX_DECODE_SAMPLES`, `StoatVoiceError` enum, `TransmitMode` enum + `should_transmit`, `rms_db`, `VortexServerInfo`.

### A.2 Vortex mock wire format

`servers/test-stoat/src/routes.rs:1117-1360` ("Phase F — Voice (Vortex mock)").

**HTTP — `POST /channels/{channel_id}/join_call`** (lines 1121-1182):
- Auth: requires session cookie/header (`session_user(&state, &headers)`).
- Channel `channel_type` must be `"VoiceChannel"`, `"DirectMessage"`, or `"Group"`; otherwise `400 NotAVoiceChannel`. Non-existent channel → `404 NotFound`.
- Side effects: upserts caller into `state.voice_sessions[channel_id].participants`; broadcasts `StoatEvent::VoiceUserJoined { channel_id, user_id, display_name, avatar_url, is_muted: false }` on the Bonfire bus.
- Response JSON: `{ "token": "vortex-token-<n>", "url": "ws://<host>/vortex/ws?token=<token>&channel_id=<id>&user_id=<id>" }`. (`<host>` from the `Host` header, fallback `localhost:9101`.)

**WebSocket — `GET /vortex/ws?token=<t>&channel_id=<id>&user_id=<id>`** (lines 1210-1360):
- Token validation: rejects if `!token.starts_with("vortex-token-")` by sending `{"type":"InvalidToken"}` and closing.
- Step 1 (immediate, server→client text JSON): `{"type":"Authenticated","user_id":"<id>"}`.
- Step 2 (server→client text JSON, 100 ms later, ONLY if `voice_sessions[channel_id].participants.len() <= 1`): `{"type":"VoiceParticipantJoined","user_id":"RACCOON01","display_name":"Raccoon","avatar_url":"raccoon","is_muted":false}` — injected so smoke tests have a remote peer.
- Server→client text JSON, ongoing, forwarded from Bonfire bus filtered to this `channel_id`:
  - `{"type":"VoiceParticipantJoined","user_id":<str>,"display_name":<str>,"avatar_url":<str?>,"is_muted":<bool>}`
  - `{"type":"VoiceParticipantLeft","user_id":<str>}`
  - `{"type":"SpeakingUpdate","user_id":<str>,"speaking":<bool>}`
  - (native voice.rs also handles `VoiceStateUpdated` and `IncomingCall` over the same envelope; the mock doesn't emit them spontaneously yet.)
- Client→server text JSON: `{"type":"Leave"}` closes the WS cleanly. Any other text is ignored.
- Client→server BINARY frame (no length prefix; raw bytes): the mock **echoes the entire frame back as a BINARY message** (loopback) — useful as self-test even with a single participant.
- BINARY frame layout (both directions, per `voice.rs:303-340` and `voice.rs:385-403`):
  - **Bytes 0..8**: ASCII user_id, NUL-padded (`0x00`) to exactly 8 bytes. Locally-encoded frames currently use 8 NULs (`voice.rs:393`).
  - **Bytes 8..N**: raw Opus payload, no length prefix.
- Cleanup on disconnect: removes user from `voice_sessions[channel_id].participants`; broadcasts `StoatEvent::VoiceUserLeft { channel_id, user_id }`.

**Note on echo-vs-uid:** because the mock echoes verbatim, the user_id prefix on echoed frames is whatever the sender wrote (currently always 8 NULs). The WASM client's per-user decoder cache will key off the empty string for self-echoed audio — fine for smoke testing, but a fan-out test with two real shells will surface this since each shell will send `0x00 * 8` and the receiver can't tell them apart. **Mark for Phase D follow-up if it bites E.3.**

**HTTP — `PATCH /channels/{channel_id}/voice_state`** (lines 1184-1208):
- Body: `{"muted": <bool>, "deafened": <bool>}`.
- Side effect: publishes `StoatEvent::VoiceSpeakingUpdate { channel_id, user_id, speaking: !is_muted }` on Bonfire.
- Response: `204 No Content`.

**Integration-test coverage:** `clients/stoat/tests/integration.rs` is wired up per `Cargo.toml`. No dedicated voice-named integration test file; the native `voice.rs` is contractually verified on the native side. Per decision 4, no wasm-pack-test added.

### Phase B — WASM Vortex client implementation — shipped in change `oxuznzwv` (B.1+B.2+B.5), B.6 in `pwuvwxtp`

- [x] **B.1** Replace native WS (`tokio_tungstenite`) with `gloo_net::websocket::futures::WebSocket` on wasm32. Mirror discord's `voice_bridge.rs:run_handshake_wasm` pattern. shipped in change `oxuznzwv`
- [x] **B.2** Replace native Opus (`audiopus` FFI) with the `/host/codec/opus/*` host-bridge pattern already used by `clients/discord/src/voice_bridge/audio_capture.rs` and `audio_playback.rs`. Reuse the same encoder/decoder session-ID lifecycle. shipped in change `oxuznzwv`
- [ ] **B.3** Replace native mic input (presumably `cpal`) with `MediaStreamTrackProcessor` — exact same browser-side capture path discord uses. Lift the helper from discord into a shared place (`clients/common/wasm_audio.rs`?) OR duplicate ~50 LoC. **B.3 agent owns `voice_wasm_audio_capture.rs`.**
- [ ] **B.4** Replace native speaker output with the WebAudio `AudioContext` + `AudioBufferSourceNode` pattern from discord's `audio_playback.rs`. Same reuse-vs-duplicate question as B.3. **B.4 agent owns `voice_wasm_audio_playback.rs`.**
- [x] **B.5** Implement the Vortex frame format on the wire: `[8 bytes user_id padded with 0x00][opus bytes]`. No RTP, no encryption. Per-user `OpusDecoder` keyed off the 8-byte prefix. shipped in change `oxuznzwv`
- [x] **B.6** Wire the WASM `StoatVoiceConnection` into `clients/stoat/src/lib.rs` so the `IsBackend` trait surface includes a `join_voice_channel(channel_id)` method that does the HTTP `/join_call` POST then opens the Vortex WS. shipped in change `pwuvwxtp`

### Phase C — UI integration

- [ ] **C.1** Add stoat to the voice-view UI's backend match arm — discord already shows Join Voice; mirror that for stoat. Check `crates/core/src/ui/voice_view.rs` (or wherever the discord Join Voice button is rendered) and add the stoat case.
- [ ] **C.2** Confirm the stoat voice channels are seeded in the test-stoat mock and navigable via URL like `/stoat/<instance_id>/<account_num>/channels/<channel_id>`.

### Phase D — Mock fixes (if surfaced during smoke)

- [ ] **D.1** Catch-all for whatever the live smoke turns up. Likely candidates: WS upgrade rejection on a query-param the WASM client sends, frame format off-by-one, opus-session reuse across clients.

### Phase E — Live mutual-audio cross-shell smoke

- [ ] **E.1** Launch poly-web as stoat account (otter), poly-electron as stoat account (beaver), both join the same voice channel via mock.
- [ ] **E.2** Both shells reach the in-voice UI with 🎤🔊📵 buttons; no console warnings; participant list shows both users.
- [ ] **E.3** (Optional, stretch) Add a `RemoteSpeakingEvent` tracing log analogous to discord, drive mic capture on one shell, observe the log on the other. This is the load-bearing mutual-audio byte-flow verification.

## Open architectural questions for the OPUS agent

1. **A.3** — cfg-gate in place vs. sibling file. Discord went with a sibling (`voice_bridge.rs`) because the native code was already mature and rewriting it as one polyglot file would have been higher-risk. Stoat's native code is similar size — same choice or different?
2. **B.3 / B.4** — shared `clients/common/wasm_audio.rs` for mic capture + speaker playback, or per-client duplication? Sharing requires a new crate and dep-graph work; duplication is ~100 LoC × 2 backends.
3. **B.5** — Vortex's per-user OpusDecoder cache: keep state-of-the-art `HashMap<UserId, OpusDecoder>` like native voice.rs:285, or simpler shared decoder? Single decoder might glitch on talk-overlap.
4. **Phase D scope** — should we proactively run the mock's integration tests (if any exist) on the WASM path via wasm-pack-test, or rely on the live smoke as the only acceptance criterion?

## Estimated scope

| Phase | LoC delta | Time |
|---|---|---|
| A (research + decisions) | ~0 (notes only) | 30-60 min wall, opus agent |
| B (WASM client impl) | ~400-700 | 2-4 sonnet agents in parallel, 30-90 min each |
| C (UI wire-up) | ~30 | 1 sonnet agent, 10-20 min |
| D (mock fixes) | unknown | 0-1 agents, surfaces during E |
| E (smoke) | ~0 | orchestrator drives in this session, 15-30 min |

**Total realistic ship target: 1 session if no mock surprises. 2-3 sessions if mock has structural issues.**
