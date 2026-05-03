# SOLID Survey — Slice C: Interface Segregation + Dependency Inversion

**Author:** orchestrator  •  **Pass:** investigation only — no refactors  •  **Date:** 2026-05-03

Scope: `clients/client/src/lib.rs` (the kitchen-sink `ClientBackend` trait), the
11 backend impls under `clients/<name>/src/lib.rs`, the UI surface types in
`clients/client/src/ui_surface.rs`, and concrete-type signal usage under
`crates/core/src/ui/`.

Sources of truth this survey leans on:
- `/home/laragana/workspcacemsg/clients/client/src/lib.rs` (1163 lines, ~70 trait methods)
- `/home/laragana/workspcacemsg/clients/client/src/ui_surface.rs` (714 lines, all the plugin UI surface types)
- `/home/laragana/workspcacemsg/wit/messenger-plugin.wit` (1656 lines, the canonical WIT contract)
- `/home/laragana/workspcacemsg/crates/core/src/state.rs` (`AppState`, ~100 fields)
- `/home/laragana/workspcacemsg/crates/core/src/state/chat_data.rs` (`ChatData`, ~30 fields)
- `/home/laragana/workspcacemsg/crates/core/src/client_manager.rs` (`ClientManager`)

Backends in scope (11): `demo`, `discord`, `forgejo`, `github`, `hackernews`,
`lemmy`, `matrix`, `reddit`, `server-client`, `stoat`, `teams`.

Legend for the matrix below:
- **Y** — backend has a real (non-NotSupported, non-trivial) implementation.
- **S** — backend explicitly overrides the method but the body is `Err(NotSupported)` or `Ok(vec![])` — i.e. it overrode just to silence a warning or for consistency.
- **.** — backend takes the trait default (which for ~50 of the 70 methods is `Err(NotSupported)`).
- **-** — backend has no `impl ClientBackend` block (only `server-client`, which is a tiny re-export shim).

For the count column, **only `Y` counts** — `S` and `.` are both functionally NotSupported.

---

## C.1 — `ClientBackend` audit table

### C.1.1 — Per-method × per-backend matrix (real impls only)

| method | demo | discord | forgejo | github | hn | lemmy | matrix | reddit | server-client | stoat | teams | nonstub | grouping |
|---|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|---|
| `get_messages`                | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **11** | core-read |
| `get_context_menu_items`      | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **11** | UI-surface |
| `invoke_context_action`       | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **11** | UI-surface |
| `poll_action`                 | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **11** | UI-surface |
| `get_settings_sections`       | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **11** | settings |
| `get_setting_value`           | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **11** | settings |
| `get_composer_buttons`        | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **11** | UI-surface |
| `get_message_actions`         | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **11** | UI-surface |
| `backend_capabilities`        | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **11** | metadata |
| `set_setting_value`           | Y | Y | Y | Y | Y | Y | Y | Y | S | S | S | **8** | settings |
| `invoke_composer_action`      | Y | Y | Y | Y | Y | S | Y | Y | Y | Y | Y | **10** | UI-surface |
| `invoke_message_action`       | Y | Y | Y | Y | Y | S | Y | Y | Y | Y | Y | **10** | UI-surface |
| `get_signup_method`           | . | Y | Y | Y | Y | Y | Y | Y | Y | Y | Y | **10** | metadata |
| `client_version`              | Y | Y | Y | Y | Y | Y | Y | . | Y | Y | Y | **10** | client-cfg |
| `set_client_version_override` | Y | Y | Y | Y | Y | Y | Y | . | . | Y | Y | **9** | client-cfg |
| `plugin_manifest`             | . | Y | Y | Y | Y | Y | Y | Y | . | . | Y | **8** | metadata |
| `get_user`                    | Y | Y | S | S | S | Y | Y | Y | Y | Y | Y | **8** | core-users |
| `get_friends`                 | Y | Y | S | S | S | Y | Y | Y | Y | Y | Y | **8** | social-graph |
| `get_channel_members`         | Y | Y | S | S | S | Y | Y | S | Y | Y | Y | **7** | core-read |
| `get_groups`                  | Y | Y | S | S | S | Y | Y | S | Y | Y | Y | **7** | dm-groups |
| `get_dm_channels`             | Y | Y | S | S | S | Y | Y | S | Y | Y | Y | **7** | dm-groups |
| `get_voice_participants`      | Y | Y | S | S | S | Y | Y | S | Y | Y | Y | **7** | voice |
| `send_message`                | Y | Y | S | S | S | S | Y | S | Y | Y | Y | **6** | messaging |
| `get_notifications`           | Y | Y | S | S | S | S | Y | S | Y | Y | Y | **6** | notifications |
| `get_my_permissions`          | . | Y | S | Y | . | . | Y | . | Y | Y | Y | **6** | moderation |
| `get_presence`                | Y | Y | S | S | S | S | Y | S | Y | Y | Y | **6** | presence |
| `set_presence`                | Y | Y | S | S | S | S | Y | S | Y | Y | Y | **6** | presence |
| `get_view_rows`               | Y | S | Y | Y | Y | Y | S | Y | S | S | S | **6** | UI-surface |
| `get_sidebar_declaration`     | S | S | Y | Y | Y | Y | Y | Y | S | S | S | **6** | UI-surface |
| `delete_message`              | . | Y | S | S | . | S | Y | . | Y | Y | Y | **5** | moderation |
| `get_bans`                    | . | Y | S | . | . | Y | Y | . | Y | Y | S | **5** | moderation |
| `send_typing`                 | Y | Y | . | . | . | . | Y | . | Y | Y | . | **5** | messaging |
| `invoke_sidebar_action`       | S | S | Y | Y | Y | Y | S | Y | S | S | S | **5** | UI-surface |
| `get_channel_view`            | S | S | Y | Y | Y | Y | S | Y | S | S | S | **5** | UI-surface |
| `ban_member`                  | . | Y | S | . | . | Y | S | . | Y | Y | S | **4** | moderation |
| `unban_member`                | . | Y | S | . | . | Y | S | . | Y | Y | S | **4** | moderation |
| `timeout_member`              | . | Y | S | . | . | Y | S | . | Y | Y | S | **4** | moderation |
| `untimeout_member`            | . | Y | S | . | . | Y | . | . | Y | Y | S | **4** | moderation |
| `get_moderation_log`          | . | Y | S | . | . | Y | Y | . | Y | . | S | **4** | moderation |
| `block_user`                  | . | Y | . | . | . | . | Y | . | Y | Y | . | **4** | social-graph |
| `ignore_user`                 | . | Y | . | . | . | . | Y | . | Y | Y | . | **4** | social-graph |
| `leave_group_dm`              | . | Y | . | . | . | . | Y | . | Y | Y | . | **4** | dm-groups |
| `edit_group_dm`               | . | Y | . | . | . | . | Y | . | Y | Y | . | **4** | dm-groups |
| `send_reply_message`          | Y | . | . | . | . | S | Y | S | Y | Y | . | **4** | messaging |
| `get_account_overview_view`   | S | S | Y | Y | Y | Y | S | . | S | S | S | **4** | UI-surface |
| `get_view_detail`             | S | S | Y | S | Y | Y | S | Y | S | S | S | **4** | UI-surface |
| `kick_member`                 | . | Y | S | . | . | S | S | . | Y | Y | S | **3** | moderation |
| `update_channel`              | . | Y | S | . | . | S | S | . | Y | Y | S | **3** | moderation |
| `unblock_user`                | . | Y | . | . | . | . | S | . | Y | Y | . | **3** | social-graph |
| `add_group_member`            | Y | . | . | . | . | . | . | . | Y | Y | . | **3** | dm-groups |
| `close_dm_channel`            | . | S | . | . | . | . | Y | . | Y | Y | . | **3** | dm-groups |
| `update_server_banner`        | . | Y | . | . | . | Y | . | . | Y | . | . | **3** | server-mgmt |
| `search_communities`          | Y | . | . | . | . | Y | . | Y | . | . | . | **3** | discover |
| `remove_group_member`         | Y | . | . | . | . | . | . | . | . | Y | . | **2** | dm-groups |
| `open_direct_message_channel` | Y | . | . | . | . | . | . | . | . | Y | . | **2** | dm-groups |
| `open_saved_messages_channel` | Y | . | . | . | . | . | . | . | . | Y | . | **2** | dm-groups |
| `respond_to_friend_request`   | Y | . | . | . | . | . | . | . | . | Y | . | **2** | social-graph |
| `unignore_user`               | . | S | . | . | . | . | S | . | Y | Y | . | **2** | social-graph |
| `add_friend`                  | . | S | . | . | . | . | S | . | Y | Y | . | **2** | social-graph |
| `remove_friend`               | . | S | . | . | . | . | S | . | Y | Y | . | **2** | social-graph |
| `mute_conversation`           | . | S | . | . | . | . | Y | . | Y | . | . | **2** | dm-groups |
| `unmute_conversation`         | . | S | . | . | . | . | Y | . | Y | . | . | **2** | dm-groups |
| `add_users_to_group_dm`       | . | S | . | . | . | . | S | . | Y | Y | . | **2** | dm-groups |
| `client_mechanisms`           | . | . | . | . | . | Y | . | Y | . | . | . | **2** | client-cfg |
| `set_client_mechanism`        | . | . | . | . | . | Y | . | Y | . | . | . | **2** | client-cfg |
| `list_files`                  | . | . | Y | Y | . | . | . | . | . | . | . | **2** | code-repo |
| `read_file`                   | . | . | Y | Y | . | . | . | . | . | . | . | **2** | code-repo |
| `mark_channel_read`           | Y | . | . | . | . | . | . | . | . | . | . | **1** | messaging |
| `search_messages`             | Y | . | . | . | . | . | . | . | . | . | . | **1** | messaging |
| `get_pinned_messages`         | Y | . | . | . | . | . | . | . | . | . | . | **1** | messaging |
| `get_channel_commands`        | Y | . | . | . | . | . | . | . | . | . | . | **1** | messaging |
| `get_available_emojis`        | Y | . | . | . | . | . | . | . | . | . | . | **1** | messaging |
| `get_available_stickers`      | Y | . | . | . | . | . | . | . | . | . | . | **1** | messaging |
| `set_friend_nickname`         | . | S | . | . | . | . | S | . | Y | . | . | **1** | social-graph |
| `set_user_note`               | . | S | . | . | . | . | S | . | Y | . | . | **1** | social-graph |
| `invite_user_to_server`       | . | S | . | . | . | . | S | . | Y | . | . | **1** | server-mgmt |
| `get_server_roles`            | . | Y | . | . | . | . | . | . | . | . | . | **1** | moderation |
| `create_server`               | . | . | . | . | . | . | . | . | Y | . | . | **1** | server-mgmt |
| `create_channel`              | . | . | . | . | . | . | . | . | Y | . | . | **1** | server-mgmt |
| `get_forum_posts`             | . | Y | . | . | . | . | . | . | . | . | . | **1** | forum |
| `get_active_threads`          | . | Y | . | . | . | . | . | . | . | . | . | **1** | forum |
| `get_archived_threads`        | . | Y | . | . | . | . | . | . | . | . | . | **1** | forum |
| `create_forum_post`           | . | . | . | . | . | Y | . | . | . | . | . | **1** | forum |
| `get_recent_comments`         | . | . | . | . | . | Y | . | . | . | . | . | **1** | forum |
| `set_message_pinned`          | . | . | . | . | . | . | . | . | . | . | . | **0** | messaging |
| `respond_to_server_invite`    | . | . | . | . | . | . | . | . | . | . | . | **0** | dm-groups |
| `get_content_policy`          | . | . | . | . | . | . | . | . | . | . | . | **0** | content-policy |
| `set_content_policy`          | . | . | . | . | . | . | . | . | . | . | . | **0** | content-policy |
| `get_blocked_users`           | . | . | . | . | . | . | . | . | . | . | . | **0** | content-policy |

(Generated by walking each `clients/<b>/src/**` for the `impl ClientBackend for` block and regex-matching method bodies for `NotSupported(`.)

### C.1.2 — Suggested sub-trait grouping (sizes & shape)

| sub-trait | methods (count) | who implements (≥1 Y) |
|---|---|---|
| **`Backend`** (required, marker + identity) | `backend_type`, `backend_name`, `backend_capabilities`, `client_version`, `get_signup_method`, `plugin_manifest`, `event_stream`, `is_authenticated`, `authenticate`, `logout` (10) | all 11 |
| **`ServerBrowsing`** (required) | `get_servers`, `get_server`, `get_channels`, `get_channel` (4) | all 11 |
| **`UiSurface`** (required by D9) | `get_context_menu_items`, `invoke_context_action`, `poll_action`, `get_settings_sections`, `get_setting_value`, `set_setting_value`, `get_sidebar_declaration`, `invoke_sidebar_action`, `get_channel_view`, `get_view_rows`, `get_view_detail`, `get_composer_buttons`, `get_message_actions`, `invoke_composer_action`, `invoke_message_action`, `get_account_overview_view` (16) | all 11 (variable real-impl rate; see C.4) |
| **`Messaging`** (capability) | `send_message`, `send_reply_message`, `send_typing`, `get_messages`, `search_messages`, `get_pinned_messages`, `set_message_pinned`, `get_channel_commands`, `get_available_emojis`, `get_available_stickers`, `mark_channel_read` (11) | demo, discord, matrix, stoat, teams, server-client (6 backends — and lemmy/reddit only for `get_messages`) |
| **`SocialGraph`** (capability) | `get_friends`, `get_user`, `add_friend`, `remove_friend`, `respond_to_friend_request`, `set_friend_nickname`, `set_user_note`, `block_user`, `unblock_user`, `ignore_user`, `unignore_user`, `get_blocked_users`, `get_presence`, `set_presence` (14) | demo, discord, matrix, server-client, stoat (5) |
| **`DmsAndGroups`** (capability) | `get_dm_channels`, `get_groups`, `open_direct_message_channel`, `open_saved_messages_channel`, `add_group_member`, `remove_group_member`, `add_users_to_group_dm`, `close_dm_channel`, `mute_conversation`, `unmute_conversation`, `leave_group_dm`, `edit_group_dm` (12) | demo, matrix, server-client, stoat (4) — discord partial, others none |
| **`Voice`** (capability) | `get_voice_participants` (1) | discord, matrix, server-client, stoat (Y); rest stub. Real voice ops live elsewhere; this trait is a 1-method surface today. |
| **`Moderation`** (capability) | `get_my_permissions`, `kick_member`, `ban_member`, `unban_member`, `timeout_member`, `untimeout_member`, `get_bans`, `delete_message`, `update_channel`, `reorder_channels`, `get_moderation_log`, `get_server_roles` (12) | discord, lemmy, matrix, stoat, server-client (5) — explicit S-stubs in forgejo/teams suggest aspirational impls |
| **`ServerManagement`** (capability) | `create_server`, `create_channel`, `update_server_banner`, `invite_user_to_server`, `respond_to_server_invite` (5) | server-client (4), discord (1), lemmy (1) — extremely sparse |
| **`Discover`** (capability) | `search_communities` (1) | demo, lemmy, reddit (3) |
| **`Notifications`** (capability) | `get_notifications` (1) | demo, discord, matrix, stoat, teams, server-client (6) |
| **`Forum`** (capability) | `get_forum_posts`, `get_active_threads`, `get_archived_threads`, `create_forum_post`, `get_recent_comments` (5) | discord (3), lemmy (2) |
| **`CodeRepo`** (capability) | `list_files`, `read_file` (2) | forgejo, github (2) |
| **`ContentPolicy`** (capability) | `get_content_policy`, `set_content_policy`, `get_blocked_users` (3) | **0 backends** — pure dead surface, just defaults |
| **`ClientConfig`** (capability) | `set_client_version_override`, `client_mechanisms`, `set_client_mechanism` (3) | most backends override version (10), 2 backends use mechanisms (lemmy, reddit) |

### C.1.3 — Recommendation: SPLIT, with caveats

**Verdict — split into ~10 capability sub-traits + 3 required core traits.** The
data is unambiguous: the bottom of the table (count ≤ 4) is **52 of ~88 trait
methods (59%)**. For 5 of those 52 the real-impl count is literally **zero** —
the methods exist exclusively to be NotSupported.

**But constrained by:**

1. **WIT contract is the source of truth, not the Rust trait.**
   `wit/messenger-plugin.wit` has 1656 lines and already shapes the surface as
   one big `messenger-plugin` world with several `interface` blocks
   (`client-events`, `client-menus`, `client-sidebar`, …). The Rust split must
   either mirror the WIT split (preferred — keep the boundary stable) or
   stay one big trait on the host side and expose capability-detection via
   `BackendCapabilities` flags (status quo). **A split that ignores WIT will
   be reverted by the next plugin author.**
2. **Dynamic dispatch.** `Box<dyn ClientBackend>` is the storage type
   (`clients/client/src/lib.rs:1102`, `BackendHandle = Arc<RwLock<Box<dyn
   ClientBackend>>>` in core). If we split, callers will need
   `Box<dyn Backend + UiSurface + ServerBrowsing>` or per-capability downcasts —
   pick one. Recommendation: keep one **`ClientBackend: Backend + ServerBrowsing
   + UiSurface`** super-trait for storage; split the capability traits with
   blanket `dyn ClientBackend -> Option<&dyn Messaging>` accessors driven by
   the existing `BackendCapabilities` bitflags.
3. **The `S` (explicit-stub) cells are a tell.** When discord overrides
   `add_friend` to return `Err(NotSupported)` instead of taking the default
   that does the same thing, it's signaling "this method should exist on a
   trait we don't have but probably should have, so I'm holding the slot."
   Splitting lets those backends *not implement* the trait at all, which is
   semantically clearer.

---

## C.2 — Top 3 ISP wins ranked by ROI

### C.2.1 — Win #1 — Carve out `ContentPolicy` (zero real impls)

**Where the kitchen-sink hurts:** `clients/client/src/lib.rs:283-299` ships
three methods (`get_content_policy`, `set_content_policy`, `get_blocked_users`)
that **no backend implements** — the matrix shows 0/11 for all three. They
exist as defaults that always return `NotSupported` or `Ok(vec![])`. Every
backend pays the cost of being a candidate consumer; every plugin author has
to read the documentation and decide "I don't have this either."

**What the split looks like:**

```rust
// clients/client/src/content_policy.rs (new)
#[async_trait(?Send)]
pub trait ContentPolicyExt: ClientBackend {
    async fn get_content_policy(&self) -> ClientResult<ContentPolicy>;
    async fn set_content_policy(&self, policy: ContentPolicy) -> ClientResult<()>;
    async fn get_blocked_users(&self) -> ClientResult<Vec<BlockedUser>>;
}
```

Remove the three methods from `ClientBackend`. Callers use:
```rust
if let Some(cp) = backend.as_content_policy() { cp.get_content_policy().await }
```

**Who benefits:** every plugin author (3 fewer methods to skim past in the
trait); the UI-side fallback in `account/settings/content_social.rs` (which
already has local-storage fallback) becomes the *only* path until a backend
opts in; the WIT bridge sheds 3 export entries. Cost: trivial — the impls
don't exist, so no migration burden.

**ROI:** highest of the three because deletion is free. Surface area shrinks
without disturbing any working code.

### C.2.2 — Win #2 — Split `Forum` + `CodeRepo` into capability traits

**Where the kitchen-sink hurts:** `clients/client/src/lib.rs:790-868` defines 5
forum methods + 2 code-repo methods. The matrix shows:
- forum: discord 3/5, lemmy 2/5, **rest 0/5**.
- code-repo: forgejo 2/2, github 2/2, **rest 0/2**.

So 9 of 11 backends carry 7 dead trait methods, while the 2 forge backends are
the only consumers of `list_files`/`read_file`. The CodeRepo capability is
*completely orthogonal* to messaging — it's a different domain entirely.

The forum case is more nuanced: discord's `get_forum_posts` and lemmy's
`create_forum_post` are real impls but they exist on a chat-shaped backend
(discord) and a forum-shaped backend (lemmy). The seven methods don't all
align — they're cross-cutting between "backends that have *threads*" and
"backends that are *forums*."

**What the split looks like:**

```rust
// clients/client/src/code_repo.rs
#[async_trait(?Send)]
pub trait CodeRepoBackend: ClientBackend {
    async fn list_files(&self, channel_id: &str, path: &str) -> ClientResult<Vec<FileEntry>>;
    async fn read_file(&self, channel_id: &str, path: &str) -> ClientResult<FileContent>;
}

// clients/client/src/forum.rs
#[async_trait(?Send)]
pub trait ForumBackend: ClientBackend {
    async fn get_forum_posts(&self, channel: &str, sort: ForumSortOrder, limit: Option<u32>) -> ClientResult<Vec<ForumPost>>;
    async fn create_forum_post(&self, channel: &str, title: &str, body: &str, tags: Vec<String>) -> ClientResult<ForumPost>;
    async fn get_recent_comments(&self, channel: &str, query: MessageQuery) -> ClientResult<Vec<Message>>;
}

// clients/client/src/threads.rs
#[async_trait(?Send)]
pub trait ThreadsBackend: ClientBackend {
    async fn get_active_threads(&self, server_id: &str) -> ClientResult<Vec<ThreadInfo>>;
    async fn get_archived_threads(&self, parent: &str, limit: Option<u32>) -> ClientResult<Vec<ThreadInfo>>;
}
```

Discord implements `ThreadsBackend`. Lemmy implements `ForumBackend`. Forgejo
+ GitHub implement `CodeRepoBackend`. The 2 issue-tracker forge backends grow
into `ForumBackend` later (issues-as-threads).

**Who benefits:** UI sites that today guard with `if let Ok(posts) =
backend.get_forum_posts(...)` would either become `if let Some(forum) =
backend.as_forum() { forum.get_forum_posts(...) }` or — better — be statically
gated on the capability. The four code-explorer routes in
`crates/core/src/ui/code_explorer.rs` would take `&dyn CodeRepoBackend`
instead of `&dyn ClientBackend` (DIP win).

**ROI:** medium-high. 7 methods migrate, 4 backends affected, 2 routes simplify.

### C.2.3 — Win #3 — Split `Moderation`, `SocialGraph`, `DmsAndGroups`

**Where the kitchen-sink hurts:** the bottom of the matrix is dominated by
these three groupings:
- moderation (12 methods) — only 5 backends have real impls; forgejo and
  teams have explicit-stub Y rate of 0 with 6+ S-stubs each (matrix
  cells: forgejo `kick`/`ban`/`unban`/`timeout`/`untimeout`/`get_bans`/
  `delete_message`/`update_channel`/`reorder_channels`/`get_moderation_log`
  are all `S` — they overrode just to silence the trait-default warning).
- social-graph (14 methods) — 5 real implementers, lots of S noise.
- dm-groups (12 methods) — 4 real implementers; reddit/lemmy/hackernews/forge
  backends are noise.

These 38 methods together = **43% of the trait** that fewer than half the
backends use.

**What the split looks like:** capability-gated traits exactly mirroring the
groupings. Three traits, ~12 methods each. UI sites that drive these features
(channel context menu, friends panel, DM picker, moderation dialogs) would
take the narrower trait.

**Concrete shrinkage:**
- `forgejo/src/lib.rs` (1045 lines) loses 12 `S`-stub moderation methods +
  3 social-graph + 4 dm-group stubs = ~80-line reduction.
- `teams/src/lib.rs` (1246 lines) loses 6 moderation S-stubs + 2 social
  S-stubs = ~50-line reduction.
- The trait surface drops from ~88 to ~30 in core, with 3 capability
  add-ons each at ~12 methods.

**Who benefits:** plugin authors writing a read-only feed (HN, GitHub) skip
the entire moderation domain. UI components like `ChannelContextMenu` and
`FriendsPanel` get statically smaller dependency surfaces (DIP win).

**ROI:** medium. 38 methods, 11 backends affected, but real code-deletion is
modest because most cells are already `.` (defaults). The win is *cognitive*
— the trait stops looking like "everything a Discord client does."

---

## C.3 — Top 3 DIP wins ranked by ROI

### C.3.1 — Win #1 — `BatchedSignal<ChatData>` props (46 files, 24 component sigs)

**Concrete-type leakage today:** `ChatData` (`crates/core/src/state/chat_data.rs:69`)
has ~30 fields covering servers, channels, messages, members, notifications,
DM channels, groups, friends, voice, presence, drag state, account ordering,
typing users, and more. **46 files** under `crates/core/src/ui/` import
`BatchedSignal<ChatData>`; `chat_view.rs` alone has 24 references.

Examples of components that take the whole signal but read only 1-2 fields:
- `crates/core/src/ui/account/common/voice_view.rs:627` — `VoiceChatBar(mut chat_data: BatchedSignal<ChatData>)` — reads `voice_connection`, `held_voice_connections`, `voice_media_settings`. That's 3 of 30 fields.
- `crates/core/src/ui/account/settings/content_social.rs:178` — `SpamFilterSection(mut chat_data: BatchedSignal<ChatData>)` — reads only `current_server` (for context) and writes settings via the backend, not `chat_data`. Pure passthrough.
- `crates/core/src/ui/account/common/chat_view.rs:632` — `mark_channel_as_read(chat_data, channel_id)` — touches a single `notifications` Vec.

Every one of these subscribes to writes of *all 30 fields*. The reactive
graph treats `voice_connection` mutations and `notifications` mutations
identically — both rerun every component that holds the signal.

**Abstraction that fixes it:** introduce **slice signals** — small `Memo`-like
read views over `ChatData` that only re-fire when the chosen field changes.
Status quo Dioxus pattern:

```rust
// In a context provider:
let voice_slice: Memo<VoiceMediaSettings> = use_memo(move || chat_data.read().voice_media_settings.clone());

// Component takes:
fn VoiceChatBar(voice: BatchedSignal<VoiceState>, mute: Callback<()>) -> Element { ... }
```

For the read-only consumers, a small `pub trait ChatRead` with only the
field accessors they need is the DIP-correct shape:

```rust
pub trait ChatNotifications {
    fn unread_count_for(&self, channel_id: &str) -> u32;
    fn mark_read(&self, channel_id: &str);
}
```

Pass `&dyn ChatNotifications` to `mark_channel_as_read` — the function no
longer knows or cares that the source is a Signal.

**Sites that simplify:** every test of these helpers becomes trivial — no
need to construct a 30-field `ChatData` to test `mark_channel_as_read`.
The reactive subscription footprint per render shrinks dramatically. Class #7
WASM-hang risk (render-time `.read()` cascading subscriptions) drops because
each component only subscribes to the slice it actually needs.

**ROI:** highest. Performance + correctness + testability gains in one move.
Migration is one component at a time — no big-bang rewrite.

### C.3.2 — Win #2 — `BatchedSignal<AppState>` props (57 files)

**Concrete-type leakage today:** `AppState` (`crates/core/src/state.rs:542`) is
even worse than `ChatData` — ~100 fields including 8 distinct
`*_context_menu` scalars, navigation, layout flags, member-list preferences,
moderation dialogs, forum scope, etc. **57 files** hold
`BatchedSignal<AppState>` or `Signal<AppState>`; key examples:

- `crates/core/src/ui/routes.rs:524` — `sync_route_to_app_state(route, app_state)`
  writes `nav.*` only.
- `crates/core/src/ui/account/common/user_profile_modal.rs:68` —
  `open_user_profile(app_state, user)` writes one of the modal fields.
- `crates/core/src/ui/account/common/chat_view.rs:1770` —
  `use_member_list_preferences_effect(app_state)` reads/writes
  `member_list_grouping`, `member_list_sort_order`, `member_list_show_offline`.
  3 of 100 fields.

**Abstraction that fixes it:** the same slice-signal / capability-trait
pattern as C.3.1, but the win is bigger because `AppState` is the central
"god struct." Suggested decomposition:

- `BatchedSignal<NavigationState>` — `routes.rs`, breadcrumbs, deep-linking
- `BatchedSignal<MemberListPrefs>` — chat_view's member sidebar sub-component
- `BatchedSignal<ContextMenuStack>` — every context-menu host (the 8 scalar
  fields are leftovers being phased out anyway, per the comment at
  `state.rs:586-590`).
- `BatchedSignal<ModerationDialogState>` — only the moderation dialog tree.

The codebase already has the *aspiration* — see the comment at
`state.rs:584-590` saying the old per-menu scalars "will be retired once
every menu-opening site migrates" to `context_menu_stack`. The DIP fix is
to lock that in by exposing `context_menu_stack` as its own signal and
deleting the scalar fields.

**Sites that simplify:** the moderation dialog tree becomes self-contained —
its components only see `BatchedSignal<ModerationDialogState>`, can be unit
tested in isolation, can have `app_state` passed as a `Provider` rather than
a prop.

**ROI:** very high but bigger surgery (57 files affected). Plan-fit:
align with the existing component-lints plan that's targeting oversize
components.

### C.3.3 — Win #3 — Routes that own concrete behaviour (`routes.rs`, 2515 lines)

**Concrete-type leakage today:** `crates/core/src/ui/routes.rs` is 2515 lines
and contains inline behaviour for every route — channel-load logic,
scroll-to-message, server-switch flow, account-switch flow, etc. Most of it
takes `BatchedSignal<ChatData>` and `BatchedSignal<AppState>` directly and
reaches into `ClientManager` via `BatchedSignal<ClientManager>` to call
backend methods. That's three god-signals plus a god-struct.

Specific patterns visible from the structure:
- 56 files reference `BatchedSignal<ClientManager>` — every loader, every
  refresh, every action handler.
- The `ClientManager` itself (`crates/core/src/client_manager.rs:304`) holds
  `HashMap<String, BackendHandle>` and dispatches by ID. Every consumer
  needs the *whole* manager just to call one backend.

**Abstraction that fixes it:** split routes by domain (social-graph routes,
moderation routes, voice routes, code-repo routes) and have each one depend
only on the capability trait it needs. Specifically:

- `code_explorer.rs` should take `&dyn CodeRepoBackend` (from C.2.2), not
  `BatchedSignal<ChatData>` + `BatchedSignal<ClientManager>`.
- The friends-panel handlers in `chat_view.rs` should take `&dyn SocialGraphBackend`
  resolved once at handler creation, not look up the backend ID on every click.
- `routes.rs` itself becomes a thin dispatcher; each Strategy lives in its
  own module with a tightly-scoped dependency.

**Sites that simplify:** mocks for unit tests collapse — instead of building a
fake `ClientManager` with one fake `BackendHandle`, you build a stub `&dyn
SocialGraphBackend` that returns 3 fake friends. The agent layer
(`mcp/chat-mcp/src/persona/context.rs`) does this already — `BackendPoolProvider`
takes per-backend trait objects — and the UI layer should follow.

**ROI:** medium-high but largest blast radius. Best done in lockstep with
C.2.3 (the moderation/social-graph trait splits) since the routes can't
depend on a narrower trait that doesn't exist yet.

---

## C.4 — False positives — where the kitchen-sink is warranted

### C.4.1 — `UiSurface` block (16 methods, 11/11 implementers)

The block of methods labelled "Client-provided UI surface (WP 1 /
plan-client-ui-surface)" at `clients/client/src/lib.rs:870-1000` deliberately
has **no default implementations** — D9 explicitly says "every backend is
required to implement them (explicit empty list for backends that have
nothing to contribute)." This is the right design: the host needs to be able
to call these on any backend without an `Option<&dyn UiSurface>` dance, and
returning empty `Vec<MenuItem>` / `Vec<SettingsSection>` is cheap.

These should stay in the core required surface (or move into a single
`UiSurface` super-trait that `ClientBackend` requires). **Do not split into
per-method capability traits.**

### C.4.2 — `MenuItem` / `SidebarItem` flat-with-`parent_id` shape

`clients/client/src/ui_surface.rs:111` `MenuItem` and `:391` `SidebarItem`
both use `parent_id: Option<String>` instead of nested `Vec<MenuItem>`. The
inline doc at `:107-109` is explicit:

> Submenus are expressed as a flat list with `parent_id` pointers (WIT
> forbids recursive records). The host reconstructs the tree: items with
> `parent_id == None` are top-level; children reference their parent by id.

**This is a WIT constraint, not a Rust-side ISP problem.** Component-Model
record types cannot be recursive. The host could store the reconstructed
tree internally for ergonomics (e.g. expose `MenuTree { item: MenuItem,
children: Vec<MenuTree> }` to UI consumers after parsing the wire format),
but the *trait surface* and the wire shape have to stay flat. This is a
genuine "kitchen-sink trait method shape forced by the runtime constraint";
keep it.

### C.4.3 — `event_stream` returning `Pin<Box<dyn Stream<Item = ClientEvent>>>`

`clients/client/src/lib.rs:689` returns one stream per backend covering all
event kinds (typing, presence, message, sidebar-invalidated, …). This *looks*
like it should split per capability, but the host's event consumer
(`crates/core/src/event_consumer.rs` and friends) is a single dispatch loop
keyed on `ClientEvent` enum variants, and per-capability streams would
require per-capability consumer threads — much more complex for no win. Keep
it.

### C.4.4 — `BackendCapabilities` bitflags vs. capability traits

The trait already has `fn backend_capabilities(&self) -> BackendCapabilities`
returning a bitflags value. This is the "soft" capability mechanism today —
the UI checks `caps.supports_voice()` to decide whether to render the mic
button. Splitting into capability traits competes with this; the migration
needs to pick one. The flags are *runtime introspection* and should stay
(plugins can advertise capabilities without the host calling them); the
traits would be *compile-time witness*. **Both can coexist** — a backend
implementing `Voice` should also set `BackendCapabilities::VOICE`. Plan
must specify the rule.

### C.4.5 — `set_client_version_override` (10/11 real impls)

This looks like a candidate for splitting (the matrix shows 10 of 11 real
impls), but per `crates/host-bridge/src/client_config.rs` and
`docs/client-settings.md`, every backend needs version overrides for
A/B testing fingerprint-evasion. Keep it on the core trait.

### C.4.6 — Demo backend's high `Y` count is not signal

The `demo` backend implements many methods other backends skip (e.g.
`get_channel_commands`, `get_available_emojis`, `get_available_stickers`,
`get_pinned_messages`). Demo is the **reference / fixture / UI testbed
backend** — it implements everything to give the UI something realistic to
exercise. Don't read demo's `Y` cells as evidence that the methods are
genuinely cross-backend; weight them at ~0.3.

### C.4.7 — `server-client` is a re-export shim

`clients/server-client/src/lib.rs` is 90 lines — it's just an internal
re-export. Its high `Y` count comes from `impl ClientBackend for
ServerClient` delegating to the inner poly-server-client logic. Don't read
it as an independent data point either.

### C.4.8 — `BatchedSignal<T>` itself is fine

`BatchedSignal<T>` is a single-purpose newtype around `Signal<T>` (six core
methods: `batch`, `with`, `set_if_changed`, `batch_if_changed`,
`pending_update`, `peek`). It is intentionally narrow and is documented as
the canonical reactive API. Do **not** split it further — the coherence is
the value (cf. the lint at `tools/scripts/forbid-signal-write.sh`).

### C.4.9 — `ActionCx<'a>` (`crates/core/src/ui/actions.rs:9`)

Single-struct (not a trait). Doesn't trigger ISP at all; it's a context
parameter. No action.

---

## Appendix A — Method count summary

- **88 trait methods** in `ClientBackend` (10 required + ~78 with defaults).
- **5 methods** with **0** real implementations (`set_message_pinned`,
  `respond_to_server_invite`, `get_content_policy`, `set_content_policy`,
  `get_blocked_users`).
- **52 methods** (59%) implemented by ≤4 backends.
- **9 methods** implemented by all 11 backends — these are the genuine "core."
- Heaviest stubbers: `forgejo` (11 explicit `S`), `teams` (10), `matrix` (10),
  `lemmy` (6).
- Real-impl champions: `stoat` (~50 Y), `discord` (~40 Y), `matrix` (~38 Y).

## Appendix B — Files audited

- `clients/client/src/lib.rs` (trait, 1163 lines)
- `clients/client/src/ui_surface.rs` (UI types, 714 lines)
- `clients/client/src/types.rs` (data types, 1891 lines — not deeply
  inspected; no traits)
- `clients/{demo,discord,forgejo,github,hackernews,lemmy,matrix,reddit,
  server-client,stoat,teams}/src/lib.rs` (all 11 impls)
- `wit/messenger-plugin.wit` (1656 lines — capability boundary)
- `crates/core/src/state.rs:542` (AppState)
- `crates/core/src/state/chat_data.rs:69` (ChatData)
- `crates/core/src/client_manager.rs:304` (ClientManager)
- `crates/core/src/ui/routes.rs` (2515 lines)
- `crates/core/src/ui/account/common/chat_view.rs` (6809 lines)
- `crates/core/src/ui/account/common/{voice_view.rs,direct_call.rs,user_profile_modal.rs,channel_list.rs,...}`
- `crates/core/src/ui/account/settings/content_social.rs`

No backend imports leak into `crates/core/src/` (`grep -c "use poly_lemmy\|
use poly_discord\|use poly_matrix\|..."` = 0 across the core crate). DIP at
the *crate* boundary is clean; the leakage is at the *signal-of-god-struct*
level inside the UI tree.
