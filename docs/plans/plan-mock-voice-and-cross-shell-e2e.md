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

## Phase B — stoat voice investigation — DONE (report only, no code; agent a2b7227)

- [x] **B.1** Read `clients/stoat/src/` — `voice.rs` exists, `voice` feature gates audiopus + poly-audio-backend
- [x] **B.2** Read `servers/test-stoat/src/` — full Vortex mock shipped at `/vortex/ws` with auth + Opus echo + participants
- [x] **B.3** Reported: protocol is Revolt/Vortex (WS-binary with 8-byte user_id prefix + Opus payload). Backend transport + mock are complete. **GAP:** `join_voice_channel_transport` calls `/join_call` and returns Ok without opening the Vortex WS — needs `AudioBackend` injection through the `IsBackend` trait method signature. Scope to reach discord-parity E2E: **medium, 3-5 days**. Explicitly de-scoped from this session.

## Phase C — cross-shell discord voice E2E — single-user voice END-TO-END VERIFIED

- [x] **C.1** Started `poly-test-runner` — test-discord healthy on 9102, voice UDP echo on port 46752. Seed triggered via `POST /seed`.
- [x] **C.2a** Both shells boot clean. Electron needed two fixes:
  - `target/debug/public` symlink → `target/dx/poly-desktop-electron/debug/web/public`
  - dropped duplicate `/host/caps` route (`rnwrstqr`)
- [x] **C.2b** Discord tokens minted from test-discord, injected into shared `account_tokens` KV as proper JSON array (NOT stringified — earlier double-encoding broke `Storage::get_account_tokens` deserialize and silently dropped ALL native accounts).
- [x] **C.3a** Discord accounts now appear in sidebar — fixed by `rppzywpt` (`account_restore.rs:71` was calling `DiscordClient::new()` ignoring `instance_id`; now uses `with_base_url` when instance_id differs from `https://discord.com`).
- [x] **C.3b** Guild navigation works — fixed by `loploqlp` (`Session.instance_id` was leaking `http://` scheme into Route URL segments, router emitted PageNotFound, `on_update` handler bounced to matrix's last-known route).
- [x] **C.3c** Voice channel surfaces in sidebar — fixed by `tvrssqkt` (`get_channels` was filtering out `GuildVoice`/`GuildStageVoice`).
- [x] **C.3d** Voice channel seeded in mock — shipped in `kmtqzzrk` (`#voice-general` in guild 100, also added to `guild.channels` vec).
- [x] **C.4 SINGLE-USER VERIFIED**: clicked koala → Australiana → voice-general → Join Voice. Result: "Voice Connected" green state, "1 in channel", voice WS handshake completes against mock `/voice/ws` (HELLO/IDENTIFY/READY/SELECT_PROTOCOL/SESSION_DESCRIPTION), UDP echo socket responds. Same flow works in poly-electron.
- [ ] **C.5 CROSS-USER BLOCKED**: With BOTH shells sharing the same `~/.local/share/poly/storage.sqlite3`, the guild server-icon for Australiana is bound to whichever account first claimed it (koala). When kangaroo clicks Australiana from the inner column, the URL rebuilds with account_id=1 (koala), not 2 (kangaroo). The `FavoriteServerIcon` doesn't multi-account a shared guild. To verify two-different-users-in-one-channel, would need either: (a) separate `POLY_DATA_DIR` per shell, (b) a per-account guild-icon variant when multiple accounts are members, (c) explicit "switch account, then join from this account's perspective" flow.
- [ ] **C.6** Cross-shell mutual-presence screenshots — deferred behind C.5.

## Phase D — cross-shell stoat voice E2E — DEFERRED

Pending Phase B's 3-5 day uplift on the stoat side AND Phase C unblock. Out of scope for this session.

## Phase E — report + lint cleanup

- [x] **E.1** cargo check clean across affected crates (poly-discord native+wasm, poly-stoat all-features, poly-test-discord, apps/web, apps/desktop-electron via dx serve verification)
- [x] **E.2** Pushed to main: `kpomlwsy` (Phase A — mock voice), `orvyzkum` (stoat fix + test cleanup), `rnwrstqr` (electron route fix)
- [ ] **E.3** Cannot mark DONE — Phase C blocked, Phase D deferred. Status: PARTIAL.
