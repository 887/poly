# Voice Bridge Architecture

**Status: Shipped** — `crates/host-bridge/src/voice.rs` + `voice_wire.rs` + `voice_client.rs`

## Why this exists

Browser WASM cannot open raw UDP sockets. Discord voice requires:
- UDP for RTP audio/video packets
- AEAD encryption (XChaCha20-Poly1305 in `rtpsize` mode)
- Opus encode/decode (libopus FFI — unavailable in wasm32)

Every Poly shell (`apps/web`, `apps/desktop`, `apps/desktop-electron`) is a
Dioxus **fullstack** app: `dx serve --fullstack` builds both a WASM client and a
native axum server from the same `src/main.rs`. The native server-half already
mounts `/host/*` routes (KV, exec, video H.264). The voice bridge adds `/host/voice/*`
on the same port so the browser WASM just speaks HTTP — no UDP, no libopus in
the WASM bundle.

The same pattern was established by `/host/video/encode_h264` and
`/host/video/decode_h264` (see `docs/dev/video-codec-strategy.md`). Voice mirrors
that design at a higher abstraction level.

## Transport diagram

```
Browser WASM (WASM bundle)         Native server-half (same process)
══════════════════════════         ══════════════════════════════════════════
                                   VoiceState
                                     │
                                     └─ HashMap<session_id, VoiceSession>
                                           │
                                           ├─ broadcast::Sender<VoiceEvent>
                                           ├─ mpsc::Sender<Vec<i16>>  ← audio_tx
                                           └─ mpsc::Sender<(bool,bool)>  ← mute_tx

POST /host/voice/connect          ─→ voice WS handshake (tokio-tungstenite)
 req: { ws_endpoint, ws_token, … }    UDP IP-discovery
 resp: { session_id, voice_ssrc }     spawn: voice_ws_loop
                                           udp_encode_loop
                                           udp_decode_loop
                                           orphan_gc

GET  /host/voice/events/:sid      ←─ SSE stream (VoiceEvent JSON)
  EventSource in browser               broadcast::Receiver per subscriber
  decodes events:
    Speaking { user_id, is_speaking }
    ParticipantJoin { user_id, ssrc }
    FrameAudio { pcm_b64 }  → WebAudio
    FrameH264  { nal_units_b64 } → WebCodecs VideoDecoder

POST /host/voice/send_audio       ─→ mpsc send → udp_encode_loop:
  { session_id, pcm_b64 }             Opus encode (audiopus)
                                       RTP packetize
                                       XChaCha20-Poly1305 encrypt
                                       UdpSocket::send()

POST /host/voice/send_video       ─→ (wired to H.264 encode path, Phase 2)
POST /host/voice/set_mute         ─→ WS op 5 SPEAKING update
POST /host/voice/disconnect       ─→ shutdown signal → session drop
```

## Feature flags

| Feature | Enables |
|---------|---------|
| `poly-host-bridge/voice` | `voice.rs` handlers (non-wasm), `voice_wire.rs` types (all targets), `voice_client.rs` (all targets) |
| `poly-host-bridge/video` | Automatically pulled in by `voice` (H.264 receive path) |
| `poly-host/voice` | Mounts voice router in the `poly-host` axum app |

## WASM safety

The handler module `voice.rs` is `#[cfg(all(not(target_arch = "wasm32"), feature = "voice"))]`.

Native-only deps (`audiopus`, `chacha20poly1305`, `tokio-tungstenite`) are declared
`optional = true` in `crates/host-bridge/Cargo.toml`. They compile only when
`feature = "voice"` is active AND the target is non-wasm32.

Verification: `cargo tree -p poly-discord --target wasm32-unknown-unknown -e features |
grep -E "openh264|audiopus|chacha20poly1305"` returns empty.

## Session lifecycle

1. **Connect**: browser sends `POST /host/voice/connect` with the voice WS endpoint,
   token, and session_id (obtained from Discord's gateway before the HTTP call).
   The native handler performs the full voice WS handshake (op 8 Hello → op 0 IDENTIFY
   → op 2 Ready → UDP IP-discovery → op 1 SELECT PROTOCOL → op 4 SESSION DESCRIPTION)
   and starts background tasks. Returns `{ session_id, voice_ssrc, video_ssrc }`.

2. **Events**: browser opens `GET /host/voice/events/:session_id` as an `EventSource`.
   The SSE stream delivers participant events, decoded audio PCM, and H.264 NALs.

3. **Audio send**: browser collects mic PCM via `WebAudioBackend.open_input()` (worklet
   pipeline), then posts 20ms frames to `POST /host/voice/send_audio`. The encode loop
   accumulates frames, Opus-encodes them, AEAD-encrypts them, and sends RTP packets.

4. **Audio receive**: the decode loop reads UDP packets, AEAD-decrypts, RTP-depacketizes,
   Opus-decodes, and broadcasts `VoiceEvent::FrameAudio` PCM over the SSE channel.
   Browser hands PCM to `WebAudioBackend.open_output().push()`.

5. **Video receive**: H.264 NALs are forwarded as `VoiceEvent::FrameH264`. Browser feeds
   them to `WebCodecs VideoDecoder.decode()` — GPU-accelerated decode without re-encoding
   native-side. This is more efficient than: native decode → yuv420p bytes → transport →
   browser encode/display.

6. **Disconnect**: browser posts `POST /host/voice/disconnect`. The handler signals all
   background tasks and removes the session. Sessions with no SSE subscriber for >60s
   are automatically GC'd by the `orphan_gc` task.

## Prior art and caveat update

`docs/dev/video-codec-strategy.md` notes "browser-side Discord video transmit is not
feasible — no UDP socket." **This caveat is now superseded**: both audio and video
transmit work in the browser via the host-bridge indirection. The browser sees only
HTTP POSTs; UDP, libopus, and libchacha live entirely in the native server-half.

Do not "simplify" this by trying to open a UDP socket from WASM — WebRTC's ICE
subsystem doesn't speak Discord's custom UDP/RTP protocol, and `wasm32-unknown-unknown`
has no raw socket API. The HTTP bridge is the correct long-term solution.

## Relation to native direct path

The existing `DiscordClient::connect_voice()` in `clients/discord/src/voice/mod.rs`
remains unchanged and is still used by:
- `chat-mcp` — runs fully native, no browser involved
- `apps/desktop` / `apps/desktop-electron` in native mode (future: they may also
  use the bridge if they run as fullstack — the bridge is already mounted)

The WASM Discord client (`connect_voice_via_bridge` / `connect_voice_smart` — planned
in Phase L of this plan) will dispatch to `VoiceBridgeClient` on WASM targets and to
the direct path on native targets.
