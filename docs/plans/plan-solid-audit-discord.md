# SOLID + missing-impl audit — `clients/discord/`

## Status: ✅ DONE — Phase A shipped in `qkmuqpks`/`nprtmlvu`; Phase B file splits B.1/B.2/B.3 in `okyokprs` / `svqknqps` / `wlqmorlz`; B.4 (WIT-guest gateway parser — partial, heavy events deferred) + B.5 (DiscordClientBuilder) + C.3 (stub triage + `get_friends` impl) shipped in close-out worktree change. C.1 (voice transport unification) and C.2 (view-rows ISP split) DEFERRED — cross-crate, see rationale on phase headers.

Audit performed in change `nprtmlvu` against parent `rorooxlm` (worktree
`agent-a052fb2a89a993497`). Scope: `clients/discord/src/**` only. The largest
plugin in the repo (~13 071 LoC across 17 files); 38 hits for stub markers
(`TODO`, `NotSupported`, `not implemented`, `Ok(vec![])`).

Out of scope per dispatch contract:
- `clients/discord/src/guardrails.rs` (just landed)
- `clients/discord/src/nitro.rs` (just landed)
- everything outside `clients/discord/`

---

## Phase A — Ship-now wins (shipped in change `qkmuqpks`)

- [x] **A.1** DRY the three `DiscordClient` constructors (`new`, `with_base_url`,
      `with_base_url_and_gateway`) into a single private `build` helper.
      lib.rs:273-355 was ~83 lines of duplicated field init — collapses to one
      helper + three thin wrappers. SRP+DRY win. Shipped in `qkmuqpks`.
- [x] **A.2** Extract duplicated Discord permission-bit constants into a private
      `permission_bits` module. The same eight `const _: i64 = 1 << N` lines
      appeared in both `get_my_permissions` (lib.rs:2722-2729) and
      `get_server_roles` (lib.rs:2997-3007). Pull-up removes the silent
      drift risk (two copies must stay in sync; one is `i32` shifts, the other
      `i64`). SRP+OCP win — adding a new perm bit is now a one-line edit.
      Shipped in `qkmuqpks`.
- [x] **A.3** Extract CDN guild image URL formatting into a private
      `Self::guild_image_urls` helper. `get_servers` (lib.rs:1496-1500) and
      `get_server` (lib.rs:1527-1530) both had identical `format!` chains for
      `(icon_url, banner_url)`. SRP/DRY win. Shipped in `qkmuqpks`.

---

## Phase B — Medium refactors (defer to dedicated work)

Each item is 50–300 LoC and should land as its own change.

- [x] **B.1** Split `lib.rs` (3 542 LoC) into per-trait files. — shipped in change `okyokprs`. Each `impl Trait for DiscordClient` block now lives under `clients/discord/src/backend/{is_backend,forum,threads,moderation,social_graph,dms_groups,messaging,server_admin,voice_transport,settings,view_descriptor,context_action}.rs`. `lib.rs` keeps struct + constructors + mappers + native gateway loop; trimmed to ~1 580 LoC.
      `DiscordClient` implements **9** distinct traits in one file:
      `IsBackend`, `ForumBackend`, `ThreadsBackend`, `ModerationBackend`,
      `SocialGraphBackend`, `DmsAndGroupsBackend`, `MessagingBackend`,
      `ServerAdminBackend`, plus inherent voice/gateway impl blocks.
      SRP violation: one file with 9 reasons to change. Mirror the trait
      structure as a module tree (`lib.rs` → `backend/`, `forum.rs`,
      `threads.rs`, `moderation.rs`, etc.). The trait splitting is already
      done in `poly_client` — this is purely physical reorganisation.
      Cite: lib.rs:1482, 2586, 2686, 2707, 3036, 3119, 3229, 3288.

- [x] **B.2** Split `voice_bridge.rs` (1 976 LoC). — shipped in change `svqknqps`. Existing `voice_bridge/{audio_capture,audio_playback}.rs` pattern extended with `voice_bridge/{rtp,voice_protocol,video_capture,video_playback}.rs`. `voice_bridge.rs` → `voice_bridge/mod.rs` (~999 LoC: error type, transmit mode, session state, struct + main impl, WS event listener, tests).
      `DiscordVoiceBridgeClient` is one type with seven responsibilities:
      WS handshake, IP discovery, AEAD session, RTP packing, video NAL
      fragmentation/reassembly (lib.rs:1625-1862 — `find_nal_unit_starts`,
      `fragment_nal_units_to_fua`, `reassemble_fua`), playback wiring, and
      event subscription. The H.264 NAL helpers belong in
      `voice_bridge/h264.rs`; the post-handshake event listener already lives
      out-of-file in `voice_bridge/audio_playback.rs` — extend that pattern.
      Cite: voice_bridge.rs:215 (struct), 1128 (`parse_session_description`),
      1625 (`find_nal_unit_starts`).

- [x] **B.3** Split `voice/mod.rs` (1 241 LoC). — shipped in change `wlqmorlz`. New sibling files `voice/{handshake,ip_discovery,ws_loop,encode,decode,rtp,aead}.rs` extract the protocol helpers, IP discovery, encode/decode loops, RTP framing, and AEAD encrypt/decrypt. `voice/mod.rs` (~630 LoC) keeps constants, error type, transmit mode, `DiscordVoiceConnection` struct/impl, per-account voice mutex, `VoiceServerInfo`, and `connect_voice` entrypoint.
      Same SRP smell as B.2 on the native path. `connect_voice` at line 276
      is one function that runs WS handshake, IP discovery, key derivation,
      and spawns the encode + decode loops. Already has `rtcp.rs` and
      `video.rs` siblings — pull `connect_voice`'s body into
      `voice/handshake.rs` and `voice/encode.rs`.

- [~] **B.4** Implement `parse_gateway_event` for the WASM plugin guest. — PARTIAL, shipped in close-out worktree change. Guest now parses the Discord Gateway envelope (`{op, t, d}`) and emits the lightweight events: `MESSAGE_DELETE` → `MessageDeleted`, `TYPING_START` → `TypingStarted`, `PRESENCE_UPDATE` → `PresenceChanged`, `THREAD_DELETE` → `ChannelUpdated` (tombstone). DEFERRED: `MESSAGE_CREATE`/`MESSAGE_UPDATE` and `THREAD_CREATE`/`THREAD_UPDATE`/`THREAD_LIST_SYNC` are logged but not emitted — they require porting `discord_message_to_poly` / `discord_channel_to_poly` (~500 LoC of attachment/reaction/forum-tag/role marshalling) into the guest. Host can fall back to polling `get_messages` / `get_channels` for these until the conversion helpers land in a future change. Cite: guest.rs:323+.

- [x] **B.5** Replace the two `with_base_url*` constructors with a builder. — shipped in close-out worktree change. New `DiscordClientBuilder` with chainable `.base_url(_).gateway_url(_).build()`. Existing public constructors (`new`, `with_base_url`, `with_base_url_and_gateway`) kept as thin wrappers around the builder for backwards compatibility — OCP win: future config knobs add `.with_X(_)` methods instead of forking the constructor.

---

## Phase C — Architectural rewrites (document only)

Each item is >300 LoC and should be planned as its own dedicated effort.

- [~] **C.1** DEFERRED — cross-cfg-gate consolidation explicitly out of scope per audit rationale. `voice_bridge.rs` ↔ `voice/mod.rs` are parallel transport
      implementations of the same Discord voice protocol — one over
      tokio-tungstenite (native), one over `gloo_net::websocket` (WASM).
      Both re-implement op 8/0/2/1/4 handshake, IP discovery, AEAD key
      negotiation, RTP packing. DIP violation: callers should depend on a
      `trait DiscordVoiceTransport` and the cfg-gate should select the impl,
      not maintain two parallel call surfaces. Estimated 800-1 200 LoC
      consolidation + a new trait crate. Out-of-scope for now because the
      two paths diverged for cfg-pragmatic reasons (no `tokio::net` on wasm32)
      and the divergence is documented; the win is maintenance, not
      correctness.

- [~] **C.2** DEFERRED — cross-crate (touches `poly_client` trait + every other client). Schedule alongside the next `poly_client` trait-split effort. The "view-rows + view-detail" surface (`get_account_overview_view`
      / `get_channel_view` / `get_view_rows` / `get_view_detail`) is a
      kitchen-sink trait the Discord backend only half-implements
      (lib.rs:2410, 2434, 2494, 2569). The legitimate Discord overview is one
      method (the guild card grid at lib.rs:2427-2492); the other three are
      `NotSupported` stubs. ISP violation in `poly_client::IsBackend`: split
      the view methods into a `ViewBackend` trait that Discord opts into for
      only the overview surface it actually supports. Cross-crate refactor —
      touches every other client. Schedule alongside the next
      `poly_client` trait-split effort.

- [x] **C.3** Implement the four `IsBackend` methods that currently return
      `Ok(vec![])` as documented stubs: — shipped in close-out worktree change.
      - `get_channel_members` (`backend/is_backend.rs:238`) — TAGGED with deferred-impl rationale (rate-limit gated; awaiting `MemberFetchGuard`).
      - `get_notifications` (`backend/is_backend.rs:248`) — TAGGED as BY DESIGN empty (UI synthesises from MESSAGE_CREATE + per-channel unread counts).
      - `get_friends` (`backend/social_graph.rs:18`) — IMPLEMENTED via new `DiscordHttpClient::get_relationships()` + `DiscordRelationship` struct, filtered by `type == 1`.
      - `get_composer_buttons` (`backend/context_action.rs:336`) — already had rationale comment (stickers/GIF in MediaPickerPopup); no change.

---

## Audit notes — uncategorised findings (informational)

- **`Mutex<DiscordMenuState>` is in-memory only** (lib.rs:199, 192-194)
  with a documented `TODO` to migrate to `host-api.kv_set`. F10 contract
  says menu state is persisted across restarts; current impl loses it on
  process exit. Tracked in lib.rs:192.
- **LSP smell — `ChannelType::Unknown(_) | _` catch-all** in
  `map_channel_type` (lib.rs:407-424) is correct only because
  `twilight_model::channel::ChannelType` is `#[non_exhaustive]`. The arm
  silently down-maps any future Discord channel type to plain Text, which
  may surprise callers. Add an `unreachable_patterns` `#[allow]` or a
  `tracing::warn!` for the catch-all so new types get noticed.
- **DIP smell — `voice_bridge` directly constructs concrete clients**
  (`UdpClient::from_origin()`, `OpusClient::from_origin()`,
  `AeadClient::from_origin()` at voice_bridge.rs:261-271) instead of taking
  trait objects. Hard-blocks unit-testing the handshake flow. Not a
  ship-now win because it requires extending the host-bridge crate API.
- **Two `TODO(discord)` ignore-vs-block mapping comments** at lib.rs:3094
  and 3100 are aliasing comments, not stubs — leave as-is.

---

## Verification — Phase A only

- `cargo check -p poly-discord --all-features` ✓ (run by orchestrator at
  end of change).
- `cd apps/web && dx build …` ✓ (run by orchestrator at end of change).
- No public API surface change — all helpers are private; the three public
  constructors keep their existing signatures and semantics.
