# Stoat Voice Protocol — Phase F.2

> Last updated: 2026-05-14

## Status

Phase F.2 investigation complete. Protocol documentation written. The test-stoat
mock implements the minimal subset described below.

---

## Background: Vortex and its evolution

Stoat is a fork of **Revolt**, an open-source chat platform. Revolt's voice service
is called **Vortex**. Historically:

| Era | Vortex backend | Notes |
|-----|---------------|-------|
| Pre-2023 | Janus WebRTC SFU | The original `vortex` service at `github.com/revoltchat/vortex` used Janus's `janus.plugin.audiobridge` for voice and `janus.plugin.videoroom` for video. |
| 2023+ | LiveKit migration | Many newer Revolt forks (and the official `revoltchat/vortex` `livekit` branch) migrate to LiveKit as the SFU. The signaling protocol changes significantly: clients get a LiveKit JWT from the REST API and connect to a LiveKit server. |
| Stoat-specific | Unknown | Stoat's internal deployment details are not public. The `StoatVoiceInformation.max_users` field in the Poly client API (`clients/stoat/src/api.rs:480`) suggests a custom or Janus-era surface. |

**Decision for Phase F:** Since Stoat's real Vortex deployment is undocumented and
potentially still in flux, Phase F implements a **test-mock Vortex protocol** that
exercises the wiring without protocol fidelity. The implementation is structured
so the WS event parsing (`voice.rs::handle_vortex_event`) is factored out and can
be retargeted to either Janus REST or LiveKit JWT once Stoat's actual protocol is
confirmed.

---

## What the test-mock simulates

The test-stoat mock (`servers/test-stoat/`) implements a minimal Vortex-alike:

### REST signaling

```
POST /channels/{channel_id}/join_call
  → { "token": "<opaque-token>", "url": "ws://<host>/vortex/ws?token=...&channel_id=...&user_id=..." }
```

- Any channel with `channel_type == "VoiceChannel"` (or DM) can be joined.
- The token is a simple counter-based string (`vortex-token-NNNNNNNN`).
- The WS URL encodes the token + channel_id + user_id for the handler to parse.

```
PATCH /channels/{channel_id}/voice_state
  Body: { "muted": bool, "deafened": bool }
  → 204 No Content
  Side effect: broadcasts VoiceSpeakingUpdate via Bonfire
```

### Vortex WebSocket (`GET /vortex/ws?token=...`)

1. **On connect:** Server sends `{"type":"Authenticated","user_id":"<id>"}`.
2. **After 100ms:** Server sends `VoiceParticipantJoined` for a synthetic "raccoon"
   participant (simulates a user already in the channel — gives smoke tests a
   participant to assert on).
3. **Binary frames:** Opus audio received from the client is echoed back verbatim
   (loopback). Frame format:
   ```
   [8 bytes] ASCII user_id, null-padded
   [N bytes] Opus payload
   ```
4. **`{"type":"Leave"}`:** Client sends this when disconnecting; server closes.

### Bonfire WebSocket events (F.6)

The following events are forwarded to existing Bonfire WS subscribers when voice
state changes:

| Event type | Payload fields | When emitted |
|------------|---------------|--------------|
| `VoiceUserJoined` | `channel_id`, `user_id`, `display_name`, `avatar_url`, `is_muted` | User calls `join_call` |
| `VoiceUserLeft` | `channel_id`, `user_id` | User disconnects from Vortex WS |
| `VoiceSpeakingUpdate` | `channel_id`, `user_id`, `speaking` | `PATCH /voice_state` called |

These are forwarded to `ClientEvent::VoiceUserJoined`, `VoiceUserLeft`, and
`VoiceSpeakingUpdate` respectively in `clients/stoat/src/lib.rs::parse_bonfire_event`.

---

## Client protocol (Poly Stoat voice.rs)

The Poly Stoat voice client (`clients/stoat/src/voice.rs`) speaks to the mock
(and would speak to a real Vortex/LiveKit server with minimal changes):

### Opus format

- Sample rate: 48 kHz
- Channels: Mono (1 channel) — `AudioFormat::STOAT_VOICE`
- Frame size: 20 ms → 960 i16 samples
- Application: `OpusApplication::Voip`

### Encode loop

```
mic PCM frames (BoxInputStream)
  → accumulate until OPUS_FRAME_SAMPLES (960) samples
  → if transmit_mode.should_transmit(pcm):
      OpusEncoder::encode(pcm) → opus_bytes
      frame = [8-byte zero user_id][opus_bytes]
      WS binary send
```

### Decode loop

```
WS binary receive
  → extract user_id from first 8 bytes
  → opus_data = bytes[8..]
  → OpusDecoder (per user_id) → pcm[0..decoded_samples]
  → AudioOutputStream::push(&pcm)
```

### WS events consumed

| Vortex event | Action |
|-------------|--------|
| `VoiceParticipantJoined` | Insert into `participants` map; emit `ClientEvent::VoiceUserJoined` |
| `VoiceParticipantLeft` | Remove from `participants` map; emit `ClientEvent::VoiceUserLeft` |
| `SpeakingUpdate` | Update `is_speaking` in map; emit `ClientEvent::VoiceSpeakingUpdate` |
| `VoiceStateUpdated` | Update `is_muted`/`is_deafened` in map; emit `ClientEvent::VoiceStateUpdated` |
| `IncomingCall` | Emit `ClientEvent::IncomingCall` (Phase H.3) |

---

## Open questions (for real Stoat integration)

1. **Janus vs LiveKit:** Which SFU does the production Stoat server use? If LiveKit,
   the client needs a `livekit-rust-sdks` dep and the token is a LiveKit JWT.
   If Janus, the REST signaling changes to `POST /janus` long-poll.

2. **Audio bridge vs room:** Janus AudioBridge (group mono mix) vs VideoRoom
   (individual streams per participant). Revolt historically used AudioBridge.

3. **Binary frame format:** The real Vortex format may differ from the mock's
   8-byte user_id prefix. The `handle_vortex_event` function in `voice.rs` needs
   to be updated when the real format is confirmed.

4. **Encryption:** Does Stoat's Vortex encrypt the WebSocket binary payload?
   Discord uses AEAD; Vortex historically relied on TLS only. If end-to-end
   encryption is required, add an encryption step before the WS send.

5. **DM calls (Phase H):** Revolt doesn't natively support DM voice calls — they
   require a shared server voice channel. The Phase H synthetic-channel approach
   (`POST /channels/create` + invite + join) may need admin permissions or may
   not be available on the production Stoat server.

---

## Files

| File | Purpose |
|------|---------|
| `clients/stoat/src/voice.rs` | Vortex WS + Opus encode/decode transport |
| `clients/stoat/src/lib.rs` | Integration with IsBackend trait + voice session guard |
| `servers/test-stoat/src/routes.rs` | `join_call`, `patch_voice_state`, `vortex_ws` handlers |
| `servers/test-stoat/src/state.rs` | `VoiceSession`, `StoatEvent` voice variants |
| `tools/stoat-voice-smoke/src/main.rs` | CLI smoke test (K.3) |
