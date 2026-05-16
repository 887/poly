# Plan: Discord voice — full media plane E2E (audio + video, mutual flow)

> Created 2026-05-16. Companion to plan-mock-voice-and-cross-shell-e2e.md
> (signaling layer). This plan covers the MEDIA plane — actual audio bytes
> encoded by koala reach kangaroo and vice-versa, plus video frames.

## Status: code complete + unit-tested; live UI E2E blocked on session-bounded items

**Code shipped on main (32+ tests passing):**

| change | what |
|---|---|
| `mstpnozu` | Foundation — WS recv channel, SPEAKING listener, session shutdown senders, WsHandle.recv |
| `qxvwklkz` | Mock UDP fan-out + per-channel session registry (2 tests) |
| `tspxsqkm` | Wasm audio CAPTURE — getUserMedia → MSTP → Opus → RTP → UDP (5 helper tests) |
| `vxqmorwp` | Wasm audio PLAYBACK — UDP → AEAD → Opus → per-SSRC AudioContext (12 helper tests) |
| `wupuxmsz` | Mock op-12/21 video signaling + wasm H.264 capture/playback with FU-A fragmentation (7 tests) |
| `rtkzrxnp` | Native run_handshake via tokio-tungstenite (3 tests incl. dispatch round-trip) |
| `xyktrktok` | MCP Chromium fake-device flags (`--use-fake-ui-for-media-stream --use-fake-device-for-media-stream`) |
| `klskuozt` | account_restore derives gateway WS URL from instance_id for test mock |
| `lmvzxqzx` | Mock emits op 10 HELLO before READY (real Discord protocol order) |
| `tkrqsztq` | lint-gate compat — Params struct refactor + cfg-gated unsafe Send on wasm32 WsHandle |

**Verified at the protocol layer:** Five distinct integration test suites all green
- `voice` (5 tests) — mock voice WS handshake stages
- `voice_fanout` (2 tests) — two-client cross-fan-out via UDP
- `voice_video_signaling` (1 test) — op 12 → op 21 video SSRC negotiation
- `voice_bridge_handshake` (3 tests) — wasm bridge handshake including native dispatch
- audio_playback / audio_capture / video_capture / video_playback unit tests — RTP parse, nonce derivation, PCM conversion, RMS, FU-A fragmentation/reassembly

**Live UI smoke remaining friction (session bounds, not code blockers):**

1. **MCP binaries need Claude-Code-session restart to load new Chromium fake-device flags.** The agent's commit modifies `mcp/{web,electron}-devtools-mcp/src/main.rs`'s `chrome_args()`, but the running MCP processes were spawned from the OLD binary at the start of this session. New binaries are built and ready; they activate on next session.
2. **Boot overlay stays up longer with the new code path.** apps/web's gateway-bridge now opens TWO concurrent WS connections during account_restore (one per discord account: koala + kangaroo). Each waits for op 10 HELLO + op 0 READY + sends IDENTIFY. The added latency pushes boot past the `BOOT_HANG_TIMEOUT_MS` watchdog so the "Boot sequence complete" overlay never dismisses, even though accounts DO load behind it (14 accounts confirmed in DOM). Per CLAUDE.md hang-debug notes: *"if the friends grid renders BEHIND the overlay the page is healthy, just slow. Bump the timeout instead of treating it as a real hang."* Follow-up: bump `BOOT_HANG_TIMEOUT_MS` for the discord-voice-bridge feature path.
3. **Live mutual-audio capture+playback verification.** Even with the overlay dismissed via JS, driving `start_audio_capture()` calls `getUserMedia({audio: true})` which on a Chromium without the fake-device flags either prompts (no user to click allow in headless smoke) or returns a NotFoundError if no real device exists. Works on next session when MCP restart picks up the flags.

**To execute the live smoke after MCPs are reloaded:**

1. New Claude Code session loads new MCP binaries with the fake-device flags
2. Start `poly-test-runner` → `POST /seed` → both shells boot
3. Both shells navigate via their respective Australiana icons → voice-general → Join Voice
4. JS-call `bridge.start_audio_capture()` (or wire a mic-button click)
5. Verify in console: `voice_bridge::audio_playback` logs `RemoteSpeakingEvent { user_id: "kangaroo", ssrc: …, rms_db: > -45 }` on koala's side; symmetric in reverse
6. Same for video via `bridge.start_video_capture()` + canvas pixel data check
