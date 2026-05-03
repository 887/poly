# Shard D — State + Reactive Architecture

> Survey-only. **No refactor.** Source of truth for the master SOLID plan.
> Cross-references CLAUDE.md hang classes #1–#8.

Counts below come from `grep -rEn` against
`/home/laragana/workspcacemsg/crates/core/src/ui/` (and selected
neighbours) on 2026-05-03. They are point-in-time but stable to ±5%.

Reference numbers used throughout:

- `client_manager.read()` call sites — **115**
- `client_manager.read().get_backend(&account_id)` — **48**
- `client_manager.read().get_backend_for_server(&...)` — **8**
- `read_with_timeout(...)` call sites — **71**
- `chat_data.batch(|cd| ...)` call sites — **142**
- `app_state.batch(|s| ...)` call sites — **96**
- `use_resource(move || ...)` — **28**
- `use_spawn_once(...)` — **24**
- `use_reactive_effect(...)` — **25**
- `use_effect(...)` — ~104 (includes the above two; raw plain `use_effect` is the residual)
- `Signal<bool>` declarations — **78**
- `RouteSynced<…>` field reads/writes — **119**
- `BatchedSignal` references — **624**

---

## D.1 — Top 5 state-architecture wins (ranked by ROI)

### D.1.1 — `BackendHandle::for_account(&signal, &id)` — *highest ROI, shipping-cost-of-cleanup low*

**Pattern repeated 48 times:**

```rust
let Some(backend) = client_manager.read().get_backend(&account_id) else { return; };
let guard = backend.read_with_timeout(Duration::from_secs(5)).await?;
guard.get_messages(...).await
```

Sample sites (each is the same shape):

- `crates/core/src/ui/account/common/saved_items_view.rs:147` (`get_backend` + `read_with_timeout` + multiple guard calls)
- `crates/core/src/ui/account/common/notifications.rs:661`
- `crates/core/src/ui/account/common/dm_user_sidebar.rs:166`
- `crates/core/src/ui/create_channel.rs:174` (`get_backend_for_server` variant)
- `crates/core/src/ui/create_server.rs:161`
- `crates/core/src/ui/account/common/direct_call.rs:84`
- `crates/core/src/ui/account/server/settings.rs:296,584`
- `crates/core/src/ui/account/settings.rs:259`
- `crates/core/src/ui/routes.rs:1109`
- `crates/core/src/ui/account/common/forum_view.rs:404`
- `crates/core/src/ui/favorites_sidebar.rs:1180,1409`
- `crates/core/src/ui/account/common/dm_context_menu.rs:187,229,254,280,314`
  (5 variants in one file — strong duplicate)
- `crates/core/src/ui/account/common/group_dm_context_menu.rs:121,150`

**Today's shape costs three things at every call site:**

1. Render-time `client_manager.read()` if not in async context — risks
   subscription bloat (CLAUDE.md hang class #7 — `peek-vs-read`).
2. Manual `Option` plumbing for "no backend for this account" — easy to
   forget the `else { return }` branch.
3. Manual 5-second timeout repeated by hand; 27 of 71 sites use 5s,
   the rest 10s/30s/no-timeout — inconsistency.

**Proposed typed abstraction:**

```rust
// in crates/core/src/client_manager.rs
impl ClientManager {
    pub async fn with_backend<F, R>(&self, account_id: &str, f: F) -> ClientResult<R>
    where F: AsyncFnOnce(&dyn ClientBackend) -> ClientResult<R>;

    pub async fn with_backend_for_server<F, R>(&self, server_id: &str, f: F) -> ClientResult<R>
    where F: AsyncFnOnce(&str /* account_id */, &dyn ClientBackend) -> ClientResult<R>;
}
```

The closure receives an already-locked-and-timeout-bounded guard.
Default 5s timeout, `BACKEND_TIMEOUT` const overridable. Returns
`ClientError::NotFound` for missing account so callers can `?` instead
of `else return`. Eliminates the `client_manager.read()` peek by
taking `&BatchedSignal<ClientManager>` and using `.peek()` internally.

**Replaces / augments:** CLAUDE.md hang class #4 countermeasure
(`BackendHandleExt::read_with_timeout`). The current lint
(`tools/scripts/forbid-raw-backend-read.sh`) bans raw `backend.read().await`
but doesn't cover the *peek-vs-read on client_manager* dimension or
the missing-backend `else` branch. A `with_backend(...)` helper makes
both correct-by-construction and removes ~48 boilerplate stanzas.

**ROI:** ~48 sites × ~6 lines collapsed each = **~290 lines
removed**. Two CLAUDE.md hang classes (#4 + #7) get tighter coverage.

---

### D.1.2 — `Signal<HashMap<Id, T>>` shadow on `ChatData.servers` / `channels` / `dm_channels` / `groups`

**Pattern:** all of `ChatData.servers: Vec<Server>`,
`channels: Vec<Channel>`, `dm_channels: Vec<DmChannel>`,
`groups: Vec<Group>`, `messages: Vec<Message>` are read-by-id N times
per render. Sample iteration sites:

- `crates/core/src/ui/favorites_sidebar.rs:1231,1442,1458` (3 × `channels.iter().find(|c| c.id == ...)`)
- `crates/core/src/ui/account/common/media_viewer.rs:38` (`messages.iter().find(|m| m.id == ...)`)
- `crates/core/src/ui/account/common/forum_view.rs:203,471,484`
- `crates/core/src/ui/signup.rs:283,393,446` (linear find on `servers`)
- `crates/core/src/ui/demo.rs:503` (`servers.iter().any`)
- `crates/core/src/ui/account/common/account_server_bar.rs:219`
- `crates/core/src/ui/routes.rs:1553` (`channels.iter().any`)
- `crates/core/src/ui/account/common/overview_subpages.rs:194-196`
  (server/dm/group filter by `account_id` in 3 consecutive lines)

**Counts:**

- `\.iter()\.find\(...\.id ==` — 4 explicit sites visible
- `\.iter()\.any\(` on these Vecs — 6+ sites
- `\.iter()\.filter\(\|...\|\..account_id ==` — 3+ sites
- Total iterator hops across all five Vecs in `crates/core/src/ui/` ≈ 14 raw, plus probably ~30 indirect through helper closures.

Worse: every reader subscribes to the *whole* `chat_data` signal, so
appending one message re-renders every component that ever read
`servers`. Hang class #1 / #7 — write-cascade and over-subscribe.

**Proposed typed abstraction — denormalized index + helper API:**

```rust
// inside ChatData
pub struct ChatData {
    // … current Vec fields stay (canonical, ordered)
    // …  but augment with:
    pub servers_by_id: HashMap<String, usize>,    // index into servers
    pub channels_by_id: HashMap<String, usize>,
    pub dm_channels_by_id: HashMap<String, usize>,
    pub groups_by_id: HashMap<String, usize>,
    pub messages_by_id: HashMap<String, usize>,
}

impl ChatData {
    pub fn server(&self, id: &str) -> Option<&Server> { … }
    pub fn channel(&self, id: &str) -> Option<&Channel> { … }
    pub fn channels_for_server(&self, server_id: &str) -> impl Iterator<Item=&Channel>;
    pub fn servers_for_account(&self, account_id: &str) -> impl Iterator<Item=&Server>;
}
```

Better — but doesn't fix the over-subscribe problem.

**Better still — split `ChatData` into focused signals (see D.3).**
Once split, each list lives in its own `BatchedSignal<List>` and
readers only subscribe to what they actually need. Per-id hash lookups
are then a within-signal optimisation, not a separate concern.

**ROI:** ~14 hot-path linear scans become O(1) lookups; cuts
re-render cascades on `chat_data` writes that don't touch the field
the reader cares about. Augments hang class #1 (`BatchedSignal`
write-cascade) — even the batched path is overkill when the reader
only wanted `current_channel`.

---

### D.1.3 — `chat_data.batch(|cd| { /* clear current view */ })` action enum

**Pattern repeated 23 times** — `cd.channels.clear(); cd.messages.clear(); cd.members.clear()`:

- `crates/core/src/ui/demo.rs:123-125` (account swap)
- `crates/core/src/ui/favorites_sidebar.rs:665-667,1295-1296`
- `crates/core/src/ui/account/common/account_server_bar.rs:570-572,613-615,658-…`

Plus the broader 142 `chat_data.batch(...)` sites — a few dozen of
them are clearly the *same* action ("user picked a different
server/channel/account"). Each call site re-implements the field-list
manually; missing one (e.g. forgetting to clear `members`) leaves
stale data on screen.

**Proposed typed abstraction:**

```rust
// in state/chat_data.rs
pub enum ChatAction {
    SwitchAccount { account_id: String },
    SwitchServer  { server_id: String },
    SwitchChannel { channel_id: String, channel: Channel },
    ClearView,
    Logout { account_id: String },
}

impl ChatData {
    pub fn apply(&mut self, action: ChatAction);
}

// Call site:
chat_data.batch(|cd| cd.apply(ChatAction::ClearView));
```

The `apply` impl owns the "what fields belong to a channel view"
knowledge — readers stop guessing.

**Replaces / augments:** CLAUDE.md hang class #1 (`BatchedSignal`)
*already* makes the 5-write cascade safe. This is one level up: it
makes the *intent* of the write self-documenting. Pairs nicely with
D.1.2 — the `apply` impl is also where the by-id index gets refreshed.

**ROI:** Lower than D.1.1/D.1.2 in line-count but high in
correctness: the 23 sites with manual field lists are bug-bait.
Worth ~20 LOC saved + a class of "forgot to clear field X" bugs
eliminated.

---

### D.1.4 — `use_view_resource<Q: ViewQuery>(query)` hook

**Pattern repeated 28 times** — every `use_resource(move || async { ... })`
is keyed on some implicit-but-conventional tuple of
`(account_id, server_id, channel_id, scope, sort, filter)` cloned
into the closure:

- `crates/core/src/ui/account/common/saved_items_view.rs:142` — `(account_id, dm_channels, groups)`
- `crates/core/src/ui/account/common/thread_view.rs:168,274,442` — `(server_id, channel_id, account_id)` × 3 in one file
- `crates/core/src/ui/account/common/account_server_bar.rs:178` — `(account_id, backend_slug)`
- `crates/core/src/ui/account/server/settings/roles.rs:24` — `(server_id, account_id, client_manager)`
- `crates/core/src/ui/account/server/settings/bans.rs:24` — same shape
- `crates/core/src/ui/account/server/settings.rs:289,577` — `(account_id, ...)`
- `crates/core/src/ui/account/settings.rs:252` — `(account_id, ...)`
- `crates/core/src/ui/account/channel/settings.rs:59` — `(account_id, ...)`
- `crates/core/src/ui/search.rs:389` — `(server_id, client_manager)`
- `crates/core/src/ui/account/common/forum_view.rs:404` — keyed inside use_effect
- `crates/core/src/ui/account/common/discord_forum_view.rs:153`

Each body re-clones the same handful of strings, then resolves a
backend, then calls one method. Three duplicated layers per call.

**Proposed typed abstraction:**

```rust
pub trait ViewQuery: Clone + PartialEq + 'static {
    type Output: Clone + 'static;
    async fn fetch(&self, backend: &dyn ClientBackend) -> ClientResult<Self::Output>;
    fn account_id(&self) -> &str;
}

pub fn use_view_resource<Q: ViewQuery>(query: Q) -> Resource<ClientResult<Q::Output>>;
```

Implementation: the hook resolves `client_manager.peek().get_backend(query.account_id())`,
`read_with_timeout(BACKEND_TIMEOUT).await`, then calls `query.fetch(&*guard)`.
PartialEq-keyed via `use_reactive_effect` so it re-fires correctly when the
query changes (CLAUDE.md hang class #6 already proven here).

**Replaces / augments:** Hang classes #4 (timeout) and #6
(`use_reactive_effect`) both apply per-site today; this hook bakes
both in. Plus the lint `forbid-raw-backend-read.sh` no longer needs
to chase async-resource bodies.

**ROI:** ~28 sites, each ~8 lines of preamble (clone, get_backend,
read_with_timeout, error branch) → ~3 lines. **~140 lines saved.**

---

### D.1.5 — `LoadState<T>` enum replacing `Signal<bool>` pairs in modal/edit components

**Sites with 3+ `Signal<bool>` declarations in one file**
(detected by grep — true state-machine smell, not just "scattered
per-component flags"):

| File | Bool signals | Likely state machine |
|---|---|---|
| `crates/core/src/ui/agent/persona/edit_modal.rs` | 13 | `loading`/`saving`/`show_*_confirm`/`open_*` accordion sections |
| `crates/core/src/ui/account/common/chat_view.rs` | 13 | `show_input_emoji`/`markdown_enabled`/`drag_over`/`notifications_muted`/`show_search_filters`/`show_command_popup`/`unread_marker_on_screen`/`pinned_filter_open`/`threads_filter_open` etc. |
| `crates/core/src/ui/account/settings/notifications.rs` | 7 | one per toggle row — fine, NOT a state machine, leave alone |
| `crates/core/src/ui/account/server/settings/notifications.rs` | 5 | same — toggle list, leave alone |
| `crates/core/src/ui/account/server/context_menu.rs` | 4 | mute submenu nesting — minor candidate |
| `crates/core/src/ui/settings/identity.rs` | 3 | reveal/copy/regenerate |
| `crates/core/src/ui/dialogs/ban_member.rs` | 3 | step machine |

Most are distinct toggles (correctly per-flag). Two are real:

- **`edit_modal.rs:384,386`** — `loading` + `saving` + `show_forget_confirm`
  + `show_delete_confirm` is a 4-state machine: `Loading → Idle → Saving → ConfirmDelete → ConfirmForget`.
- **`chat_view.rs`** — accordion of mutually-exclusive panels
  (`utility_panel`, `pinned_filter_open`, `threads_filter_open`,
  `show_search_filters`) reads as ONE `enum ChatRightPanel { None, Pinned, Threads, Search, Utility(_) }`.
- **`dialogs/ban_member.rs`** — wizard steps, classic case for
  `enum BanMemberStep { Reason, Duration, Confirm }`.

**Proposed typed abstraction:**

```rust
pub enum LoadState<T, E = ClientError> {
    Idle,
    Loading,
    Loaded(T),
    Failed(E),
}
// + helper hook
pub fn use_load_state<T, F>(deps: D, fetch: F) -> Signal<LoadState<T>>;
```

For wizards / accordions, lift to a per-component `enum FooStep`.

**Replaces / augments:** No CLAUDE.md hang class directly, but
removes the "two booleans both true" impossible state that's a
classic source of reactivity bugs. Touches at most 5 files.

**ROI:** Lowest of the five. Listed for completeness because the
investigation request asked. The bigger win is **D.4 territory** (do
NOT lift simple per-row toggles into enums).

---

## D.2 — `AppState.nav` audit

**File:** `crates/core/src/state.rs:169-237`. Defined as
`pub struct NavigationState { … }`.

**Field inventory:**

| Field | Type | Read sites | Write sites | Category |
|---|---|---|---|---|
| `view` | `RouteSynced<View>` | many | router only | **Route** (URL-synced) |
| `active_backend` | `RouteSynced<Option<BackendType>>` | many | router only | **Route** |
| `active_instance_id` | `RouteSynced<Option<String>>` | many | router only | **Route** |
| `active_account_id` | `RouteSynced<Option<String>>` | ~50 | router only | **Route** |
| `selected_server` | `RouteSynced<Option<String>>` | ~80 | router only | **Route** |
| `selected_channel` | `RouteSynced<Option<String>>` | ~80 | router only | **Route** |
| `right_sidebar_visible` | `bool` | ~15 | header toggle | **UiToggle** |
| `dm_right_sidebar_visible` | `bool` | ~10 | header toggle | **UiToggle** |
| `mobile_dm_contact_detail_visible` | `bool` | ~5 | mobile detail open/close | **UiToggle (mobile)** |
| `account_last_routes` | `HashMap<String, String>` | ~10 | nav-watcher + favorites_sidebar | **Persistence** (per-account scratch) |
| `account_last_dm_routes` | `HashMap<String, String>` | ~5 | nav-watcher | **Persistence** |
| `profile_modal_user` | `Option<User>` | ~5 | open_user_profile + close | **Modal** |
| `pending_direct_call` | `Option<PendingDirectCallRequest>` | ~3 | direct_call + dm consumer | **Transient action** |
| `thread_panel_open` | `Option<String>` | ~10 | thread panel open/close | **Modal/panel** |

**Suggested split — three focused signals:**

```rust
// crates/core/src/state.rs
pub struct AppState {
    pub nav: BatchedSignal<NavState>,        // route-synced subset
    pub ui_layout: BatchedSignal<UiLayout>,  // sidebar visibility, mobile detail, layout_mode, mirroring
    pub ui_overlays: BatchedSignal<UiOverlays>, // context menus, modals, dialogs, voice banner
    pub user_prefs: BatchedSignal<UserPrefs>,// member_list_*, forum_scope, overview_scope, view_filter
}

pub struct NavState {
    pub view: RouteSynced<View>,
    pub active_backend: RouteSynced<Option<BackendType>>,
    pub active_instance_id: RouteSynced<Option<String>>,
    pub active_account_id: RouteSynced<Option<String>>,
    pub selected_server: RouteSynced<Option<String>>,
    pub selected_channel: RouteSynced<Option<String>>,
    pub account_last_routes: HashMap<String, String>,
    pub account_last_dm_routes: HashMap<String, String>,
}

pub struct UiLayout {
    pub layout_mode: LayoutMode,
    pub mirror_menu_layout: bool,
    pub mirror_chat_messages: bool,
    pub right_sidebar_visible: bool,
    pub dm_right_sidebar_visible: bool,
    pub mobile_dm_contact_detail_visible: bool,
}

pub struct UiOverlays {
    pub context_menu: Option<ContextMenuState>,
    pub channel_context_menu: Option<ChannelContextMenuState>,
    pub dm_context_menu: Option<DmContextMenuState>,
    pub group_dm_context_menu: Option<GroupDmContextMenuState>,
    pub account_context_menu: Option<AccountContextMenuState>,
    pub attachment_context_menu: Option<AttachmentContextMenuState>,
    pub reaction_context_menu: Option<ReactionContextMenuState>,
    pub avatar_context_menu: Option<AvatarContextMenuState>,
    pub context_menu_stack: Vec<ActiveContextMenu>,
    pub profile_modal_user: Option<User>,
    pub thread_panel_open: Option<String>,
    pub active_moderation_dialog: Option<ModerationDialog>,
    pub pending_direct_call: Option<PendingDirectCallRequest>,
}

pub struct UserPrefs {
    pub settings_section: SettingsSection,
    pub member_list_grouping: MemberListGrouping,
    pub member_list_sort_order: MemberListSortOrder,
    pub member_list_show_offline: bool,
    pub forum_scope: String,
    pub overview_scope: String,
    pub view_filter: PostsOrComments,
    pub search_type_seed: Option<Vec<String>>,
    pub last_known_perms: Option<MemberPermissions>,
    pub sidebar_invalidated_tick: u32,
    pub is_setup_complete: bool,
}
```

**Why this split is the right one:**

1. **719 reads of *any* context-menu field** today re-render every
   `app_state` subscriber. Splitting `ui_overlays` off cuts ~80 % of
   those cascades — readers of `nav.selected_channel` (the hottest
   field) stop re-rendering on every menu open/close.
2. `nav` is route-synced (writes locked to the router); the others
   are not. The current single-struct mixing makes it ambiguous which
   fields can be touched from a click handler — splitting clarifies
   the contract per-struct.
3. `ui_layout` and `user_prefs` are persisted (settings); `ui_overlays`
   is *never* persisted. Different lifecycles, different invariants.
4. **One context-menu modal is open at a time** — the 8 separate
   `Option<…>` fields are a textbook case for a discriminated enum:

   ```rust
   pub enum ContextMenu {
       None,
       Server(ContextMenuState),
       Channel(ChannelContextMenuState),
       Dm(DmContextMenuState),
       GroupDm(GroupDmContextMenuState),
       Account(AccountContextMenuState),
       Attachment(AttachmentContextMenuState),
       Reaction(ReactionContextMenuState),
       Avatar(AvatarContextMenuState),
   }
   ```

   The codebase comment at `state.rs:585-590` already acknowledges
   the `context_menu_stack` is the planned replacement for these
   scalar fields. Land the enum migration as part of the split.

**Risks / boundaries:**

- 624 `BatchedSignal` references means migrating the type is
  non-trivial. Memory `feedback_signal_migration_namespace_blind` warns
  that the search/replace must catch `Signal<crate::path::T>` shapes
  too.
- Some downstream readers cross-cut multiple slices (e.g.
  `MainLayout` reads layout AND overlays). They'd take 2–3 contexts.
  That's OK — Dioxus encourages this (per-slice context).
- `RouteSynced` is its own discipline; the router writes to `NavState`
  in one batched write. Don't re-spread RouteSynced fields across
  slices.

---

## D.3 — `ChatData` audit

**File:** `crates/core/src/state/chat_data.rs:69-171`. Defined as
`pub struct ChatData { … }`.

**Field inventory by category:**

| Category | Fields | Reads in `crates/core/src/ui/` |
|---|---|---|
| **Lists (canonical, by-id lookups dominate)** | `servers`, `channels`, `messages`, `members`, `dm_channels`, `groups`, `notifications` | ~80 |
| **Per-account maps** | `friends`, `account_sessions`, `blocked_users`, `account_server_order`, `account_order` | 49 (account_sessions) + 64 (favorites/order) |
| **"Currently looking at"** | `current_server`, `current_channel`, `channel_load_error`, `channel_load_error`, `messages_loaded_via_anchor`, `loading`, `typing_users`, `active_group_members` | 26 (current_*) + 54 (others) |
| **Voice (call session)** | `voice_channel_participants`, `voice_connection`, `held_voice_connections`, `voice_media_settings` | 80 |
| **Drag-and-drop transient** | `dragging_server_id`, `drag_source`, `drag_over_id`, `favorited_server_ids` | 49 + 61 |
| **Per-account policy** | `content_policy`, `blocked_users` | ~10 |

**Suggested split — six focused signals:**

```rust
pub struct ChatLists {           // canonical data from backends
    pub servers: Vec<Server>,
    pub channels: Vec<Channel>,
    pub messages: Vec<Message>,
    pub members: Vec<User>,
    pub dm_channels: Vec<DmChannel>,
    pub groups: Vec<Group>,
    pub notifications: Vec<Notification>,
    // by-id indexes (D.1.2)
    pub servers_by_id: HashMap<String, usize>,
    pub channels_by_id: HashMap<String, usize>,
    pub dm_channels_by_id: HashMap<String, usize>,
    pub groups_by_id: HashMap<String, usize>,
    pub messages_by_id: HashMap<String, usize>,
}

pub struct ChatViewState {       // "what is the user looking at right now"
    pub current_server: Option<Server>,
    pub current_channel: Option<Channel>,
    pub channel_load_error: Option<String>,
    pub messages_loaded_via_anchor: bool,
    pub loading: bool,
    pub typing_users: Vec<String>,
    pub active_group_members: Vec<User>,
}

pub struct AccountSessions {     // per-account identity + ordering
    pub account_sessions: HashMap<String, Session>,
    pub account_order: Vec<String>,
    pub favorited_server_ids: Vec<String>,
    pub account_server_order: HashMap<String, Vec<String>>,
    pub friends: HashMap<String, Vec<User>>,
    pub blocked_users: HashMap<String, Vec<BlockedUser>>,
    pub content_policy: ContentPolicy,
}

pub struct VoiceState {          // active call(s)
    pub voice_channel_participants: HashMap<String, Vec<VoiceParticipant>>,
    pub voice_connection: Option<VoiceConnection>,
    pub held_voice_connections: Vec<VoiceConnection>,
    pub voice_media_settings: VoiceMediaSettings,
}

pub struct DragState {           // transient — drag/drop feedback only
    pub dragging_server_id: Option<String>,
    pub drag_source: DragSource,
    pub drag_over_id: Option<String>,
}
```

**Why this split:**

1. **Voice state has 80 read sites and never overlaps with chat
   list state.** Today, every `chat_data.batch(...)` that touches
   `messages` re-renders the call banner. The voice banner is the
   single biggest beneficiary of the split.
2. **Drag state writes are very frequent** during a drag (every
   `dragover`); they currently re-render every chat-list reader.
   Hang class #1 / #5 / #7 territory — splitting makes it
   structurally impossible.
3. **`ChatViewState` is the natural pair for the `nav` slice in D.2.**
   `nav.selected_channel` is the *intent* ("user clicked X");
   `chat_view.current_channel` is the *resolved data* ("here's the
   `Channel` struct for X"). They drift today (e.g.
   `routes.rs:1109` resolves the latter from the former through a
   backend call). Pairing them doc-wise (and keeping them in
   adjacent slices) makes the contract obvious. Don't merge them —
   they have different write privileges (router-only vs anyone).
4. **`AccountSessions` shares a lifecycle with NavState's
   `account_last_routes` / `account_last_dm_routes`** — both are
   per-account, both are persisted, both reset on logout. Worth
   considering a tighter pairing (or a shared `AccountScopedState`
   slice) although the current routes-vs-sessions split has clean
   ownership: NavState is route, AccountSessions is identity.

**Action enums per slice (D.1.3 applied):**

- `ChatViewState::apply(ViewAction::ClearForServerSwitch)` — fixes the
  23 manual-clear sites.
- `DragState::apply(DragAction::Started { source, server_id })` /
  `DragAction::Ended` — replaces 4-line clears at every drop handler.
- `VoiceState::apply(VoiceAction::Connected | OnHold | Hangup)` —
  the voice_banner currently does manual swaps between
  `voice_connection` and `held_voice_connections`.

**Cost:** Same migration shape as D.2. The `BatchedSignal<ChatData>`
is the most-touched signal in the app; landing this in one big PR is
not viable. Per-slice phasing (extract `VoiceState` first; it's
self-contained) is the way.

---

## D.4 — Things to LEAVE alone

Patterns that look duplicated but where ad-hoc IS the right call.
Listing explicitly so the master plan doesn't burn cycles here.

### D.4.1 — Per-row `Signal<bool>` toggles in settings pages

**Sites:** `crates/core/src/ui/account/settings/notifications.rs:53-59`
(7 booleans), `crates/core/src/ui/account/server/settings/notifications.rs`
(5 booleans), `crates/core/src/ui/agent/integrations.rs:134`,
`crates/core/src/ui/settings/voice_video.rs:76,106`, etc.

**Why leave alone:** each bool drives an independent settings row.
Bundling them into a struct doesn't reduce LOC, doesn't fix any
hang class, and forces every row to re-render when any other row
toggles (subscriber over-broadening — hang class #7 in reverse).
Per-flag signals are correct here.

### D.4.2 — `RouteSynced<T>` per route-synced field

**Sites:** the 6 `RouteSynced<…>` fields on `NavigationState`.

**Why leave alone:** `RouteSynced` is the existing discipline; it
locks writes to the router via compile-time gating
(`crate::ui::routes::sync_route_to_app_state`). A wrapper on top
("group all RouteSynced into one nested struct") would add a layer
without changing the write-permission story. The current shape is
already tighter than freeform fields — don't refactor.

### D.4.3 — `SettingsStorageCell`

**Sites:** `clients/client/src/ui_surface.rs:288` (definition) +
1 instance per backend in `clients/{demo, discord, matrix, lemmy,
hackernews, github, teams, stoat, server-client, …}`.

**Why leave alone:** **the shared base class already exists**. All
9 client crates already use the same `SettingsStorageCell` type via
re-export from `clients/client`. The "duplication" the request
hinted at is just per-instance ownership — that's intentional
(each backend instance owns its own storage cell, which guarantees
cross-backend isolation by construction; comment at
`ui_surface.rs:284-286`). The 34 grep hits are not duplication;
they're correct usage. Move along.

The only mild improvement would be auto-generating the
`get_setting_value` / `set_setting_value` impls via a derive macro
or a default trait method (the bodies are identical: 12 lines of
"check storage cell → fall back to declared default → return
NotFound" repeated 8 times across `clients/lemmy/src/lib.rs:671-690`,
`clients/discord/src/lib.rs:1741-1763`, `clients/teams/src/lib.rs:1064-1086`,
etc.). Worth doing, but it's a 90-LOC win — *much* lower priority
than D.1.1 / D.1.2 / D.1.4. Ship under the SOLID master plan as
a small "default trait body" item if there's bandwidth.

### D.4.4 — `BatchedSignal` itself

**Sites:** 624 references across the workspace.

**Why leave alone:** the type contract is doing exactly what it
needs to — closures-only writes, type-system-locked deprecated
shadow `.write()`. Don't try to "improve" it; CLAUDE.md hang
classes #1, #2, #5 are closed by it. The lints around it
(`forbid-signal-write.sh`, `forbid-effect-self-write.sh`,
`forbid-stale-effect-capture.sh`) are the audit surface — keep
adding migrations into the type, not new wrappers around the type.

### D.4.5 — `ContextMenuState` scalar `Option<…>` fields on AppState

**Sites:** 8 separate `Option<…>` fields at `state.rs:570-587`.

**Looks** like an obvious enum candidate (D.2 actually proposes
collapsing them). But there's an in-flight migration to
`context_menu_stack: Vec<ActiveContextMenu>` already documented
inline at `state.rs:585-590`. Don't propose a *third* shape — let
the existing migration finish, then prune the scalar fields. This
is "leave alone for now, watch the existing plan".

### D.4.6 — `use_signal(...)` for component-local UI

**Sites:** ~107 sites of `use_signal(|| false)` / `use_signal(|| true)`,
~78 `Signal<bool>` declarations.

Most of these are component-local UI flags (popover open, drag
hovered, edit mode active) that only ever live for a handler tree
in a single component. They are not state, they are UI. Leave alone.
Hang classes don't apply — these signals have one writer, one
reader, no async path.

---

## Cross-references to existing plan files

For the orchestrator integrating this shard:

- **`docs/plans/plan-batched-signal.md`** — D.1.3 ChatAction enum is
  Phase 4 (other hot-path signals) territory.
- **`docs/plans/plan-peek-vs-read.md`** — D.1.1 `with_backend(...)`
  helper would migrate render-time `client_manager.read()` to
  `.peek()` automatically.
- **`docs/plans/plan-backend-read-timeout.md`** — D.1.1 closes the
  remaining "raw `client_manager.read().get_backend()`" surface that
  the existing `forbid-raw-backend-read.sh` lint cannot reach
  (because the violation is on `client_manager`, not on `backend`).
- **`docs/plans/plan-use-spawn-once.md`** — D.1.4 `use_view_resource`
  combines the spawn-once + reactive-effect + timeout + backend-resolve
  triple into one hook.
- The 8 context-menu fields are already on a migration path — see
  the inline comment at `crates/core/src/state.rs:585-590`. Land
  D.2's split *after* that migration completes (or fold in).
