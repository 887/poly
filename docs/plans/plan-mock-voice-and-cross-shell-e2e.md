# Plan: mock voice E2E + cross-shell verification (discord + stoat)

> Created 2026-05-16.

## Goal

Make discord and stoat voice testable end-to-end against test-server
mocks (no real third-party accounts), then prove the pipeline works by
having two real shells (poly-web + poly-electron) join the same voice
channel as two different test users and verify they connect.

## Phase A — test-discord mock voice (server side) — shipped in git commit dd2df96a on branch worktree-agent-a3d76fe361a7ad85c

- [x] **A.1** Add op-4 handler in `servers/test-discord/src/routes.rs`'s `handle_gateway_socket`: client sends `{op:4, d:{guild_id, channel_id, self_mute, self_deaf}}`, mock responds with `VOICE_STATE_UPDATE` dispatch (carrying mock `session_id`) AND `VOICE_SERVER_UPDATE` dispatch (carrying `endpoint`/`token`/`guild_id`).
- [x] **A.2** Add mock voice gateway WS endpoint at `/voice/ws` on the test-discord server. Implements: send op 8 HELLO on connect, accept op 0 IDENTIFY, send op 2 READY (with mock ssrc + a UDP port), accept op 1 SELECT_PROTOCOL, send op 4 SESSION_DESCRIPTION (with fake AEAD key bytes — all zeros is fine for mock, or random).
- [x] **A.3** Add mock UDP echo socket on the announced port — receives bound packets, replies with the same bytes (so the bridge's IP-discovery + audio round-trip both work). Opus and AEAD are passthrough — mock doesn't decrypt/decode, just echoes.
- [x] **A.4** Wire the `endpoint` in VOICE_SERVER_UPDATE to point at the mock's own `/voice/ws` (e.g. `ws://127.0.0.1:9102/voice/ws` — strip the `wss://` scheme since this is local + plaintext). Fixed in `clients/discord/src/voice_bridge.rs`: loopback endpoints now use `ws://` and append `/voice/ws`.
- [x] **A.5** Add an integration test in `servers/test-discord/tests/voice.rs` that drives the full handshake from a tokio client and asserts each step lands. 5 tests all pass.

## Phase B — stoat voice investigation

- [ ] **B.1** Read `clients/stoat/src/` to find any existing voice support (look for `voice`, `livekit`, `webrtc`, `audio`, `rtp` references).
- [ ] **B.2** Read `servers/test-stoat/src/` for mock voice scaffolding.
- [ ] **B.3** Report: what voice protocol stoat uses (matrix-style livekit? custom WebRTC? bespoke?), what's wired, what's missing, what a mock would need to be useful for E2E.

## Phase C — cross-shell discord voice E2E

- [ ] **C.1** Start `poly-test-runner` (or just test-discord on port 9102 if simpler) — fresh state.
- [ ] **C.2** Drive `poly-web` (Chromium, port 3000) — login as test user A (one of the test animals — pick one from the seed users).
- [ ] **C.3** Drive `poly-electron` (port 3001) — login as test user B (different animal).
- [ ] **C.4** Both join the same Discord voice channel in the mock.
- [ ] **C.5** Verify in browser console logs (`mcp__poly-web__list_console_messages` + `mcp__poly-electron__list_console_messages`) that both clients reached `VOICE_SERVER_UPDATE` → handshake → SESSION_DESCRIPTION → IP discovery → audio round-trip stages.
- [ ] **C.6** Screenshot the voice-channel-roster UI in both shells showing the other user as present.

## Phase D — cross-shell stoat voice E2E

Same as Phase C but with stoat backend. Scope TBD pending Phase B findings.

## Phase E — report + lint cleanup

- [ ] **E.1** Run lint-gate + cargo check
- [ ] **E.2** Commit + push everything to main
- [ ] **E.3** Mark plan DONE
