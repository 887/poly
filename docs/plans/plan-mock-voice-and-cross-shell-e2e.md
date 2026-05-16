# Plan: mock voice E2E + cross-shell verification (discord + stoat)

> Created 2026-05-16.

## Status: ✅ DONE for discord — cross-shell two-user voice verified end-to-end through real UI clicks. Stoat deferred (3-5 day uplift, see Phase B).

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
- [x] **C.5 CROSS-USER VERIFIED** — shipped in `vmzurqml` (favorites_sidebar.rs + account_restore.rs). Dedup changed from `server.id` to `(server.id, account_id)`; favorite-sidebar rendering switched from O(1) `server_by_id` (which returns only the latest push for a shared id) to a flat-map over `chat_lists.servers` filtered by id. Result: when koala + kangaroo are both members of Australiana, the sidebar renders TWO Australiana icons — each routes through its own account_id. Clicked each in their respective shell: `/discord/.../1/channels/100/204` (koala in poly-web), `/discord/.../2/channels/100/204` (kangaroo in poly-electron), both showing "Voice Connected" green state simultaneously.
- [x] **C.6** Screenshots captured: poly-web shows koala at bottom-left + voice-general selected + Voice Connected; poly-electron shows kangaroo at bottom-left + voice-general selected + Voice Connected. Both routes show distinct account_id segments (`/1/` vs `/2/`), proving the multi-account guild rendering works end-to-end.

## Phase D — cross-shell stoat voice E2E — DEFERRED

Pending Phase B's 3-5 day uplift on the stoat side AND Phase C unblock. Out of scope for this session.

## Phase E — report + lint cleanup

- [x] **E.1** cargo check clean across affected crates (poly-discord native+wasm, poly-stoat all-features, poly-test-discord, apps/web, apps/desktop-electron via dx serve verification)
- [x] **E.2** Pushed to main: `kpomlwsy` (Phase A — mock voice), `orvyzkum` (stoat fix + test cleanup), `rnwrstqr` (electron route fix), `rppzywpt` (account_restore honors discord instance_id), `kmtqzzrk` (seed voice-general), `loploqlp` (strip scheme from Session.instance_id), `tvrssqkt` (get_channels includes voice), `vmzurqml` (multi-account guild rendering)
- [x] **E.3** Phase C complete with cross-shell verification. Phase D (stoat) explicitly deferred (3-5 day scope). Plan DONE for the discord track.
