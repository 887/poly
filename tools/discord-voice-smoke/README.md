# discord-voice-smoke

CLI smoke test for the Discord voice transport (Phase B.12 of
`docs/plans/plan-voice-video-calls.md`).

## Usage

```bash
DISCORD_TOKEN=<user_token> \
DISCORD_GUILD_ID=<guild_id> \
DISCORD_VOICE_CHANNEL_ID=<channel_id> \
cargo run -p discord-voice-smoke
```

## What it tests

1. Authenticates with a real Discord user token.
2. Sends op 4 Voice State Update on the main gateway → collects
   `VOICE_STATE_UPDATE` (session_id) and `VOICE_SERVER_UPDATE` (endpoint/token).
3. Connects the voice WebSocket (`wss://{endpoint}/?v=4`).
4. Runs IP-discovery (70-byte UDP handshake) → sends op 1 SELECT PROTOCOL
   with `aead_xchacha20_poly1305_rtpsize`.
5. Receives op 4 Session Description (encryption key).
6. Starts encode loop: mic PCM → Opus → RTP → AEAD-XChaCha20 → UDP.
7. Starts decode loop: UDP → AEAD decrypt → RTP strip → Opus decode → speaker.
8. Holds the channel for 5 seconds (synthetic silence from `FakeAudioBackend`).
9. Disconnects: op 4 with `channel_id: null`, closes voice WS, drops UDP socket.
10. Prints audio stats. PASSED if both encode + decode loops started.

## Credentials

- `DISCORD_TOKEN` — Discord user auth token (NOT a bot token).
- `DISCORD_GUILD_ID` — Snowflake ID of the server.
- `DISCORD_VOICE_CHANNEL_ID` — Snowflake ID of the voice channel to join.

Optional overrides:

- `DISCORD_GATEWAY_URL` — WS gateway URL (default: `wss://gateway.discord.gg/?v=10`)
- `DISCORD_BASE_URL` — REST base (default: `https://discord.com`)
- `RUST_LOG` — log filter (default: `discord_voice_smoke=info,poly_discord=debug`)

## CI / automation

This binary is **NOT** run in automated CI. It requires:
- Real Discord credentials (a throwaway test account).
- A live voice channel with someone to talk to (or at minimum, an
  empty channel to verify the encode path).

To opt-in: set `RUN_VOICE_SMOKE=1` plus the env vars above, then run
`cargo run -p discord-voice-smoke` manually. See Phase K.2 of
`docs/plans/plan-voice-video-calls.md` for the full acceptance bar.

## Audio backend

The smoke test uses `FakeAudioBackend` (produces silence, counts samples).
For real headset testing, swap to `CpalBackend`:

```rust
use poly_audio_backend::cpal_backend::CpalBackend;
let audio = CpalBackend::new().expect("cpal");
```
