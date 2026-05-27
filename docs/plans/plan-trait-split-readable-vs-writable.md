# Plan — Trait split: readable vs writable sibling traits

## Status: ✅ DONE — all 5 writable sub-traits shipped.
##   - Tier 1: `WritableMessagingBackend` (commit `9ad515c8`)
##   - Tier 2: `WritableSocialGraphBackend`, `WritableModerationBackend`,
##     `WritableServerAdminBackend`, `WritableDmsAndGroupsBackend`
##     (Tier 2 commits, this branch)
##   - Per-backend migration of trait method impls into the new
##     writable impl blocks is OPPORTUNISTIC: the architecture +
##     default-delegating shims are in place; concrete migrations
##     landed for the backends most likely to surface SOLID gains
##     (read-only forge/news backends drop write stubs; matrix/discord/
##     stoat/etc migrate writes into the writable trait when next touched).

## Goal

Resolve the `NotSupported`-stub-as-trait-method anti-pattern that SOLID
audits keep flagging on read-only backends (`poly-forgejo`,
`poly-lemmy`, `poly-github`, `poly-hackernews`).  Currently the parent
`IsBackend` trait declares mutating methods (`send_message`,
`create_server`, `add_friend`, `set_message_pinned`, …) with a default
`Err(NotSupported)`; read-only backends either inherit the default or
override with a more specific error message.  Either way the method
exists in their public surface.

This plan splits each kitchen-sink trait into a **read-only base** + a
**writable sibling sub-trait**, mirroring the
`VoiceTransportBackend` / `as_voice_transport()` pattern shipped in
`plan-solid-audit-core-state.md` Phase C.1.  Writable backends opt in
by implementing the writable sub-trait + overriding the parent's
`as_writable_xxx()` accessor; read-only backends do nothing and the
write method ceases to exist for them.

Cross-references (deferred items unblocked once this lands):

- `plan-solid-audit-forgejo.md` Phase C.2, C.3
- `plan-solid-audit-lemmy.md` Phase C.2
- `plan-solid-audit-github.md` Phase C.2
- `plan-solid-audit-hackernews.md` Phase C.2

## Pattern (canonical example — `send_message`)

1. New trait `WritableMessagingBackend: Send + Sync` declares
   `send_message(&self, channel_id, content) -> ClientResult<Message>`.
2. Parent `IsBackend` gains
   `fn as_writable_messaging(&self) -> Option<&dyn WritableMessagingBackend> { None }`.
3. `IsBackend::send_message` becomes a default-delegating shim:
   ```rust
   async fn send_message(&self, ch: &str, c: MessageContent) -> ClientResult<Message> {
       match self.as_writable_messaging() {
           Some(w) => w.send_message(ch, c).await,
           None => Err(ClientError::NotSupported("send_message".into())),
       }
   }
   ```
4. Writable backends (`matrix`, `discord`, `teams`, `stoat`, `demo`,
   `poly-server`, `lemmy`, `github`, `hackernews`) move their existing
   `send_message` impl into a new `impl WritableMessagingBackend` block
   and override `as_writable_messaging` to return `Some(self)`.
5. Truly read-only backends (`forgejo`) drop their `NotSupported`
   stub entirely — the trait method no longer exists for them.
6. UI / MCP call sites use capability dispatch:
   ```rust
   if let Some(wm) = guard.as_writable_messaging() {
       wm.send_message(&channel_id, content).await
   } else {
       Err(ClientError::NotSupported("read-only backend".into()))
   }
   ```
   The legacy `guard.send_message(...)` form continues to compile via
   the parent shim.

## Phase A — Audit which methods are write-coded

- [x] **A.1** `IsBackend` direct-write methods worth splitting first:
  `send_message`, `mark_typing` (already on `MessagingBackend`).
  `IsBackend::send_message` is the parent-trait method here — splits
  cleanly with the pattern above. Most other write-shaped methods on
  `IsBackend` are already on capability sub-traits.
- [x] **A.2** `SocialGraphBackend` mixes read+write: reads
  (`get_user`, `get_friends`, `get_presence`) vs writes (`add_friend`,
  `remove_friend`, `block_user`, `unblock_user`, `ignore_user`,
  `unignore_user`, `respond_to_friend_request`, `set_friend_nickname`,
  `set_user_note`, `set_presence`).  All 4 read-only backends opt-in
  for the reads but stub the writes with `NotSupported`. **Top
  candidate** for the second split.
- [x] **A.3** `MessagingBackend` is already 100% optional/read-shaped
  (`send_typing` fire-and-forget, `get_pinned_messages`,
  `get_channel_commands`, etc.).  `set_message_pinned` is the lone
  mutator. Lemmy stubs it as `NotSupported`. Candidate for Tier 2.
- [x] **A.4** `ServerAdminBackend` is mostly write (`create_server`,
  `create_channel`, `update_server_banner`, `invite_user_to_server`,
  `respond_to_server_invite`) with one read-ish `mark_channel_read`.
  Already an opt-in sub-trait — read-only backends simply don't impl
  it.  No split needed; just enforce-no-NotSupported-stubs on opt-in.
- [x] **A.5** `DmsGroupsBackend`, `ModerationBackend`,
  `ForumBackend`, `ThreadsBackend`, `ContentPolicyBackend`,
  `CodeRepoBackend` — punt to Tier 2; same pattern applies but
  surface is smaller / less frequently stubbed.

## Phase B — Define the new traits

- [x] **B.1** Add `clients/client/src/writable_messaging.rs` declaring
  `trait WritableMessagingBackend: Send + Sync` with the single
  method `send_message(&self, channel_id, content) -> ClientResult<Message>`.
  Re-export from `clients/client/src/lib.rs`.
- [x] **B.2** Add `IsBackend::as_writable_messaging(&self) -> Option<&dyn WritableMessagingBackend> { None }`.
- [~] **B.3** `WritableSocialGraphBackend` — deferred to follow-up.
  Pattern identical; see Phase B.1 + B.2 for template.

## Phase C — Migrate the parent trait's method to a delegating shim

- [x] **C.1** `IsBackend::send_message` body becomes
  `match self.as_writable_messaging() { Some(w) => w.send_message(ch, c).await, None => Err(NotSupported) }`.
  Old callers keep working; no UI changes needed in this phase.

## Phase D — Update each backend's impls

For each writable backend, move the existing `send_message` impl into
a new `impl WritableMessagingBackend for X` block, and override
`as_writable_messaging()` on the `IsBackend` impl to return `Some(self)`.

- [x] **D.1** `clients/demo/src/lib.rs` — extract `send_message` into
  `impl WritableMessagingBackend for DemoClient`.
- [x] **D.2** `clients/matrix/src/is_backend.rs` —
  `impl WritableMessagingBackend for MatrixClient` (lives in
  `clients/matrix/src/writable_messaging.rs`).
- [x] **D.3** `clients/discord/src/backend/is_backend.rs` —
  `impl WritableMessagingBackend for DiscordClient` (in
  `clients/discord/src/backend/writable_messaging.rs`).
- [x] **D.4** `clients/teams/src/...` — `impl WritableMessagingBackend
  for TeamsClient`.
- [x] **D.5** `clients/stoat/src/is_backend.rs` —
  `impl WritableMessagingBackend for StoatClient`.
- [x] **D.6** `clients/server-client/src/backend.rs` —
  `impl WritableMessagingBackend for PolyServerClient`.
- [x] **D.7** `clients/lemmy/src/is_backend.rs` —
  `impl WritableMessagingBackend for LemmyClient`.
- [x] **D.8** `clients/github/src/impl_is_backend.rs` —
  `impl WritableMessagingBackend for GithubClient`.
- [x] **D.9** `clients/hackernews/src/lib.rs` —
  `impl WritableMessagingBackend for HackernewsClient`.
- [x] **D.10** `clients/reddit/src/backend/is_backend.rs` —
  `impl WritableMessagingBackend for RedditClient`.
- [x] **D.11** `clients/forgejo/src/is_backend.rs` — DROP the
  `send_message` stub entirely.  No `impl WritableMessagingBackend`.
  Calls to `forgejo.send_message(...)` now hit the parent shim's
  `NotSupported` branch (unchanged behavior, but the method no longer
  appears on `ForgejoClient`'s capability surface).

## Phase E — Update call sites in `crates/core/` and `mcp/`

- [~] **E.1** `crates/core/src/ui/account/common/chat_view/composer.rs`
  — opportunistic.  Old `guard.send_message(...)` form still works via
  the shim, so no urgent change required.  Future refactor can switch
  to `if let Some(wm) = guard.as_writable_messaging() { ... }` for
  better error UX ("this backend is read-only" vs the generic
  `NotSupported`).
- [~] **E.2** `mcp/chat-mcp/src/main.rs`, `tools/chat.rs`,
  `tools/drafts.rs` — same deal, opportunistic.

## Tier 2 — Follow-up writable sub-traits

Phase F — `WritableSocialGraphBackend` (shipped in tier-2 commit)

- [x] **F.1** Define `clients/client/src/writable_social_graph.rs`
  carrying `add_friend`, `remove_friend`, `respond_to_friend_request`,
  `set_friend_nickname`, `set_user_note`, `block_user`, `unblock_user`,
  `ignore_user`, `unignore_user`, `set_presence`.
- [x] **F.2** `SocialGraphBackend::as_writable_social_graph` accessor +
  default-delegating shims for the writes.
- [x] **F.3** `IsBackend::as_writable_social_graph` accessor delegating
  through `as_social_graph()`.
- [x] **F.4** Read-only backends drop write stubs: `forgejo`,
  `hackernews`, `github`, `lemmy`, `reddit`.
- [x] **F.5** Writable backends opt in via `as_writable_social_graph()
  -> Some(self)`: `matrix`, `discord`, `teams`, `stoat`, `demo`,
  `server-client`.

Phase G — `WritableModerationBackend` (shipped in tier-2 commit)

- [x] **G.1** Define `clients/client/src/writable_moderation.rs`
  carrying `kick_member`, `ban_member`, `unban_member`,
  `timeout_member`, `untimeout_member`, `delete_message`,
  `update_channel`, `reorder_channels`.
- [x] **G.2** `ModerationBackend::as_writable_moderation` accessor +
  default-delegating shims for the writes.
- [x] **G.3** `IsBackend::as_writable_moderation` accessor.
- [x] **G.4** Migrated backends: `forgejo`, `github`, `matrix`.
- [~] **G.5** `stoat`, `discord`, `lemmy`, `teams` — DEFERRED.
  Existing `impl ModerationBackend` blocks satisfy the (now-default-
  bearing) trait via overrides; capability dispatch via
  `as_writable_moderation()` returns `None` until each backend is
  touched and opts in. No functional regression.

Phase H — `WritableServerAdminBackend` (shipped in tier-2 commit)

- [x] **H.1** Define `clients/client/src/writable_server_admin.rs`
  carrying `create_server`, `create_channel`, `update_server_banner`.
- [x] **H.2** `ServerAdminBackend::as_writable_server_admin` accessor +
  default-delegating shims for the writes.
- [x] **H.3** `IsBackend::as_writable_server_admin` accessor.
- [x] **H.4** Migrated backends: `lemmy`, `matrix`, `stoat`, `discord`,
  `server-client`.
- [x] **H.5** Demo drops the stubs entirely (no writable opt-in).

Phase I — `WritableDmsAndGroupsBackend` (shipped in tier-2 commit)

- [x] **I.1** Define `clients/client/src/writable_dms_and_groups.rs`
  carrying `add_group_member`, `remove_group_member`,
  `add_users_to_group_dm`, `edit_group_dm`, `close_dm_channel`.
- [x] **I.2** `DmsAndGroupsBackend::as_writable_dms_and_groups`
  accessor + default-delegating shims for the writes.
- [x] **I.3** `IsBackend::as_writable_dms_and_groups` accessor.
- [~] **I.4** Per-backend `impl WritableDmsAndGroupsBackend` migrations
  — DEFERRED. Existing `impl DmsAndGroupsBackend` blocks satisfy the
  (now-default-bearing) trait via overrides; opportunistic migration
  when each backend file is next touched. No functional regression.

Phase J — Remaining follow-ups

- [ ] `WritablePinningBackend` (or fold into `WritableMessagingBackend`) —
  `set_message_pinned`.
- [ ] `ServerAdminBackend` enforcement — lint banning
  `Err(NotSupported)` returns inside opt-in capability impls.

## Verification

For each commit:

- `cargo check --workspace` clean.
- `cargo check -p poly-core --target wasm32-unknown-unknown` clean.
- `cargo check -p poly-{forgejo,github,hackernews,lemmy,matrix,discord,teams,stoat,demo,server-client,reddit}` clean.
- Per-client unit tests where present.
