# Plan: SOLID + missing-impl audit — core/state, host-bridge, poly-client

> Owner: alexander.stuermer@aareon.com
> Created: 2026-05-17
> Status: 🟡 IN PROGRESS — Phase A (ship-now wins) shipped in change `swmztsumvtpl`
>
> Scope (non-UI core layer only — UI lives under `crates/core/src/ui/` and is
> owned by parallel agents):
> - `crates/core/src/state/` (12 modules, 1903 LoC)
> - `crates/core/src/state.rs` (root types, 718 LoC)
> - `crates/core/src/client_manager.rs` (1067 LoC) + `client_manager_timeout.rs` (237 LoC)
> - `crates/core/src/storage.rs` (1056 LoC) + `crates/core/src/storage/` (4 backends, 756 LoC)
> - `crates/core/src/theme.rs` (431 LoC)
> - `crates/core/src/wasm_crash_handler.rs` (631 LoC)
> - `crates/host-bridge/` (3568 LoC)
> - `clients/client/` — the actual `poly-client` crate (the task brief said
>   `crates/poly-client/`; that path does not exist — workspace member is
>   `clients/client/` with package `name = "poly-client"`)
>
> Out of scope (touched only as observation):
> - `crates/core/src/ui/`, `crates/core/src/ui.rs`, `crates/core/src/event_stream.rs`,
>   `crates/core/src/account_restore.rs` — parallel opus agents.
> - `clients/*/` per-backend impls — parallel opus agents.

## Executive summary

Most of the *call-site* SOLID work landed in `plan-solid-refactor-survey.md`
(Phases G/H/I/J). The reactive-state primitives (`BatchedSignal`,
`use_spawn_once`, `use_reactive_effect`) are clean, well-segregated, and
enforced by CI lints. The four sub-signal slices (`NavState`, `UiLayout`,
`UiOverlays`, `UserPrefs`, plus `ChatLists`, `ChatViewState`, `AccountSessions`,
`VoiceState`, `DragState`) demonstrate good SRP — each ≤ 50 lines, one
reason to change.

**What still bites:**

- **`ClientManager` (`client_manager.rs:305`) is a 12-field god struct** —
  active backends, demo flag, server→account map, sessions, connection
  statuses, presence statuses, plugin settings registry, disabled list,
  signup entries, test accounts, expected ids, capability registry. Each
  field has a distinct change driver. (Medium refactor.)
- **`IsBackend` (`clients/client/src/lib.rs:112`) is still a 60+-method
  kitchen-sink trait** even after the `as_content_policy` / `as_moderation`
  / `as_forum` / `as_threads` / `as_social_graph` / `as_dms_and_groups` /
  `as_messaging` / `as_server_admin` / `as_code_repo` / `as_discover`
  capability-trait extractions. The remaining mandatory methods
  (`backend_type`, `backend_name`, `authenticate`, `logout`, `event_stream`,
  `plugin_manifest`) are load-bearing; the rest are `Err(NotSupported)`
  defaults that backends override. ISP suggests pulling the optional voice /
  view-descriptor / settings-storage / context-menu surfaces out into their
  own trait objects exposed via further `as_*` accessors. (Architectural —
  out-of-scope file, recorded as a follow-up.)
- **Stale TODO markers on shipped features** in `storage.rs` and
  `client_manager.rs` — fixed in Phase A.
- **Magic KV-key strings** in `storage.rs` — 11 unique keys, each repeated
  2–5 times. Adding a new persisted namespace required grepping the file.
  Fixed in Phase A.

## Phase A — Ship-now wins (≤50 LoC each)

- [x] **A.1** Remove stale `TODO(phase-2.5.1): Client Manager Module`
  comment at `crates/core/src/client_manager.rs:8`. The module is fully
  implemented (33 public methods, ~1067 LoC). Shipped in change `swmztsumvtpl`.
- [x] **A.2** Remove stale `TODO(phase-2.4.3.8): Favorites storage
  implementation` at `storage.rs:565` and `storage.rs:772`. `get_favorites`,
  `upsert_favorite`, `remove_favorite` are fully implemented. Shipped in
  change `swmztsumvtpl`.
- [x] **A.3** Remove stale `TODO(phase-2.4.3.9): Theme preferences storage`
  at `storage.rs:848`. `get_theme_config` / `set_theme_config` are
  implemented. Shipped in change `swmztsumvtpl`.
- [x] **A.4** Remove stale `TODO(phase-2.4.3.10): Migration system` at
  `storage.rs:1034`. `run_migrations` is implemented with a v0→v1 stamp and
  schema-version persistence. Shipped in change `swmztsumvtpl`.
- [x] **A.5** Extract 11 magic KV-namespace strings into a new
  `pub mod keys` at top of `crates/core/src/storage.rs`, and rewrite
  `reset_user_data` to use them. OCP win — adding a new persisted namespace
  is now a single-line `const` add. Shipped in change `swmztsumvtpl`.

Verification: `cargo check -p poly-core --all-features` — green, 2m43s.

## Phase B — Medium refactors (50-300 LoC each)

- [ ] **B.1** **SRP-split `ClientManager`** (`crates/core/src/client_manager.rs`,
  1067 LoC, 12 fields). Decompose into three sub-stores held inside
  `ClientManager` so callers continue to take one `Signal<ClientManager>`,
  but each store has one reason to change:
  - `BackendRegistry` — `backends`, `server_account_map`,
    `expected_account_ids`, `backend_capabilities`
  - `AccountIdentity` — `sessions`, `connection_statuses`,
    `presence_statuses`, `disabled_native_backends`
  - `PluginRegistry` — `plugin_settings`, `signup_entries`,
    `test_account_entries`, `demo_active`
- [ ] **B.2** **Replace remaining magic KV-key string sites** — call sites
  on lines 658, 666, 741, 752, 764, 775, 786, 794, 805, 824, 838, 851, 862,
  871, 885, 893, 906, 919, 930, 943, 953, 973, 980, 995, 1014, 1019, 1037,
  1050 still embed string literals. Migrate each to `keys::*` constants
  added in A.5.
- [ ] **B.3** **DIP — `ClientManager` consumer-side trait extraction.** UI
  components that only need to look up a backend by account ID currently
  take `&ClientManager` or `Signal<ClientManager>`. Define
  `trait BackendLookup { fn get_backend(&self, account_id: &str) -> Option<BackendHandle>; }`
  and migrate read-only consumers to `impl BackendLookup`. Lets future
  test fakes / persona-MCP shims swap in without dragging the full struct.
- [ ] **B.4** **`Storage` trait extraction.** `Storage(StorageInner)` is
  hard-coded — fine for the app, painful for `chat-mcp` and persona-MCP
  callers that want an in-memory fake. Define
  `trait KvStore { async fn get/set/delete/clear }` and add a
  `pub struct MemoryKvStore` test impl in a `#[cfg(test)]` module.
- [ ] **B.5** **`run_migrations` extensibility.** The current `if version
  < 1` ladder is a textbook OCP violation in waiting — every new schema
  bump edits the same `match`. Move each step into
  `async fn migrate_v{N}_to_v{N+1}(&self) -> Result<...>` and have
  `run_migrations` iterate a `const STEPS: &[(u64, MigrationFn)]` table.
- [ ] **B.6** **`wasm_crash_handler.rs` is 631 LoC of mixed-concern code**
  — boot-hang watchdog, panic-hook installation, overlay rendering, fetch
  bridge, debug-key handler. Split into `watchdog.rs`, `panic_hook.rs`,
  `overlay.rs`, `debug_keys.rs` (cfg-gated wasm32-only inner module).
  Each sub-file ≤ 200 LoC, single reason to change.
- [ ] **B.7** **`AccountSessions::content_policy` is a singleton field on
  a per-account-collection struct** (`state/account_sessions.rs:47`). When
  switching accounts the policy is replaced wholesale — but the struct
  already keys other fields by account ID (`account_sessions`,
  `blocked_users`). Promote `content_policy: ContentPolicy` to
  `content_policies: HashMap<String, ContentPolicy>`. LSP win — the field
  shape matches every other per-account map.
- [ ] **B.8** **Voice-noise integration is a real missing impl, not a
  stale comment** (`state/chat_data.rs:27`,
  `TODO(phase-voice-3)`). The toggle is wired in the UI but the
  RNNoise → WebRTC send-track plumbing isn't. Track in
  `plan-voice-phase-3-noise.md` (separate plan) — leave the comment.

## Phase C — Architectural rewrites (>300 LoC each)

- [ ] **C.1** **`IsBackend` ISP split** (`clients/client/src/lib.rs`, 773
  LoC, 60+ methods). Move every default-`NotSupported` method onto its own
  capability trait exposed via `as_*`. Targets: `start_dm_call_transport`,
  `join_voice_channel_transport`, `set_voice_mute`, `get_voice_participants`
  → `VoiceTransportBackend`. `get_settings_sections`, `settings_storage`,
  `get_setting_value`, `set_setting_value` → `SettingsBackend`.
  `get_sidebar_declaration`, `invoke_sidebar_action`,
  `get_account_overview_view`, `get_channel_view`, `get_view_rows`,
  `get_view_detail` → `ViewDescriptorBackend`. `get_context_menu_items`,
  `invoke_context_action`, `get_message_actions`, `invoke_message_action`,
  `get_composer_buttons`, `invoke_composer_action`, `poll_action` →
  `ContextActionBackend`. Net: `IsBackend` shrinks to ~12 mandatory
  methods (auth, plugin metadata, server/channel/message read,
  event-stream). Backends drop ~40 stub `Err(NotSupported)` overrides.
  **Out of scope for this plan** (file is owned by the per-client agents);
  recorded so the next backend audit can pick it up.
- [ ] **C.2** **Unify the four voice/codec/aead/udp host-bridge clients
  behind a common transport trait.** Each of `voice_client.rs`,
  `codec_opus_client.rs`, `aead_client.rs`, `udp_client.rs` re-implements
  the same `POST → JSON → typed-error` pipeline against different
  endpoints, with subtly-different retry/timeout policies. Define
  `trait HostRoute { fn endpoint() -> &str; type Req; type Resp; type Err; }`
  and one generic `fn call<R: HostRoute>(req: R::Req) -> Result<R::Resp, R::Err>`.
  Touches ~600 LoC across the four `*_client.rs` files. DIP + DRY win.
- [ ] **C.3** **`AppState` is now nearly empty (2 fields, post-G.5)** —
  consider deleting the struct entirely. `is_setup_complete` belongs on
  `AccountSessions` (it's an identity-init flag), `sidebar_invalidated_tick`
  belongs on `ChatLists` (it gates `get_sidebar_declaration` refresh).
  Removes the last "god-struct" name from the codebase and one whole
  `BatchedSignal` context. ~300 LoC of call-site updates across
  `crates/core/src/ui/` (so requires coordination with parallel UI agent).

## Hang-class spot-checks (in scope)

Scanned for raw `Signal::write()` / `Signal::read()` outside the migrated
`BatchedSignal` surface across all scope files:

- `crates/core/src/state/use_reactive_effect.rs:92` — `deps_sig.read()` on a
  dedicated mirror Signal owned by the hook. Safe (one-shot, scoped).
- `crates/core/src/state/use_spawn_once.rs:151` — `spawned_for.read()`,
  cloned immediately. Safe (no live guard across `.set()`).
- `crates/core/src/client_manager.rs:699,711,872,884,966` — all are
  `BackendHandle` (`Arc<RwLock<Box<dyn IsBackend>>>`) reads/writes, NOT
  Dioxus `Signal`s. These are hang class #4 territory; the file uses the
  `BackendHandleExt::read_with_timeout` extension for the hot paths and
  documents the discipline at `client_manager.rs:653-660`. No new
  exposures found.

No new hang-class HIGH findings in scope.

## Verification

```
cargo check -p poly-core --all-features            # green, Phase A
```

Per-crate `dx build` and `poly-host-bridge` checks left to follow-up
runs since A.1–A.5 only touched comments and added a `mod keys` block
behind `pub` — no behavioural change.
