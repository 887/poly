# Shard A — Single Responsibility Survey

> Investigation pass only — no refactor performed. Line numbers are
> snapshots from 2026-05-03; expect drift on a churn-y codebase.

The existing plan `docs/plans/plan-component-lints.md` already
identifies the rsx!-cap problem and ships the lint that will force
splits on every new oversize component. This survey ranks where the
existing cap *already-existing* offenders should be cut first, and
flags non-component SRP rot the cap doesn't see (god structs, mega
trait impls, mixed-purpose backend libs, fixture mountains).

---

## A.1 — Top 5 SRP wins (by ROI = impact / effort)

### 1. `crates/core/src/ui/account/common/chat_view.rs` (6809 lines, 1 file)

**What's mixed (proven by `grep` survey, see "Evidence" below):**

The file simultaneously owns —

1. **Chat-view orchestration** — `ChatView` component + render tree.
2. **State plumbing for the orchestrator** — `ChatViewSignals` (35
   fields, line 949) and `ChatViewMarkupCtx` (70 fields, line 1902).
   The 70-field struct is the largest god struct in the workspace;
   `ChatViewSignals` is #4. Both serve the same component and are
   manually mirrored — every new piece of UI state requires editing
   two structs in lockstep.
3. **Twelve `use_*_effect` hooks** (lines 1248-1869) mixing reactive
   flows for member list, search, pinned messages, history, command
   preload, unread marker visibility, mobile side-column, header-
   actions overflow, auto-dismiss divider — each effect is its own
   "reason to change" and several are direct subjects of the
   countermeasures in CLAUDE.md (`use_history_state_effect` is the
   `effect-self-write` poster child from hang class #8).
4. **A virtualization engine** — `should_virtualize_messages`,
   `estimate_message_row_height`, `estimate_message_block_height`,
   `recompute_history_spacers`, `compute_message_virtual_window`,
   `read_message_list_viewport_metrics`, `trim_message_window_*`,
   `set_message_virtual_window`, `wait_for_next_animation_frame`,
   `spawn_message_list_scroll_work` (lines 1982-2342). 360 lines of
   pure scroll/window math that has nothing to do with chat semantics.
5. **A renderer** — 25+ `render_*` helpers for header, header info,
   header right, header overflow, drag overlay, layout shell, main
   column, search tab button, agent toggle, member toggle, slash-
   command popup, etc. (lines 2343-end).
6. **Twelve sub-components** — `ChatHeaderActions` (347 lines),
   `ChatUtilityRail` (205 lines), `AttachmentsView` (108 lines),
   `ChatSettingsPanel` (96 lines), `MessageInlineEdit`,
   `SlashCommandPopup`, `MsgContextMenuOverlay`, `DmContactRow`,
   `DmContactListPanel`, `SearchResultCard`, `PinnedMessageCard`,
   `SearchFilterPopup`. Two of them (`ChatHeaderActions`,
   `ChatUtilityRail`) blow the 150-line component cap — they belong in
   sibling modules, not this file.
7. **Composer-domain helpers** — slash-command filtering
   (`filtered_slash_commands`, line 138), built-in command apply
   (`apply_builtin_command`), reply preview snippet, attachment
   previews (`build_attachment_previews`, `append_attachment_previews`,
   `pending_attachment_to_attachment`), search filter completion logic
   (`build_search_filter_options`, `apply_search_filter_completion`,
   etc.). All useful in isolation, all currently unreachable from any
   sibling because they're private to this 6800-line file.

**Split shape (target — ~10 modules):**

```
ui/account/common/chat_view/
    mod.rs                        # ChatView component + render entry (≤300 lines)
    signals.rs                    # ChatViewSignals + use_chat_view_signals
    markup_ctx.rs                 # ChatViewMarkupCtx + builder
    effects/
        member_list.rs            # use_member_list_effect + use_member_list_preferences_effect
        search_messages.rs
        pinned_messages.rs
        history_state.rs          # known effect-self-write hotspot
        command_preload.rs
        unread_marker.rs
        auto_dismiss_divider.rs
        mobile_side_column.rs
        composer_focus.rs
        header_overflow.rs        # already cfg-gated mobile/desktop
    virtualization.rs             # should_virtualize_messages + estimate_* + window math
    composer_helpers.rs           # slash command + reply preview + attachment previews
    search_filter.rs              # build_/filter_/apply_ search filter options
    header.rs                     # ChatHeaderActions component (currently 347 lines)
    utility_rail.rs               # ChatUtilityRail (205 lines)
    side_panels.rs                # AttachmentsView + ChatSettingsPanel + SlashCommandPopup + …
```

**Effort: L (one focused week, but parallelisable across worktrees per
sub-step).**

**Why this beats alternatives:** This file is touched on every chat-
related feature (the recent jj log shows forum, reddit, lemmy work
landing here repeatedly). Two of CLAUDE.md's hang-class plans
(`use_reactive_effect` and `peek-vs-read`) had to call out functions
*inside this file by name* as their canonical examples — meaning this
file *is* where the bugs land. Splitting it gives every other refactor
plan smaller targets to grep through, not just SRP virtue.

---

### 2. `clients/{discord,matrix,stoat}/src/lib.rs` — 60-77 async fns each, all in one impl block

**What's mixed:** Each backend crate has its `ClientBackend` impl as a
single 1500-2000-line `impl ClientBackend for XxxClient { … }` block:

- `discord/src/lib.rs` line 672 — 67 async fns through line ~2100.
- `matrix/src/lib.rs` line 625 — 63 async fns.
- `stoat/src/lib.rs` line 504 — 77 async fns.
- `demo/src/lib.rs` — three separate `ClientBackend for DemoClient*`
  impls in one file (lines 111, 729, 1223).

`ClientBackend` itself is a kitchen-sink trait covering: auth,
servers, channels, threads, forum posts, messages, members, friends,
DMs, groups, voice, presence, permissions, moderation (kick/ban/
timeout/unban), bans listing, channel CRUD + reorder, moderation log,
roles, context menu, plugin settings, settings sections, sidebar
declaration. ISP violation — every backend implementation has ~15
methods that just return `NotSupported` because the trait demands them.

**Split shape:** Two-step.

- **Step A (low-risk, per-backend):** Inside each backend crate, split
  the giant impl into `mod auth`, `mod servers`, `mod channels`,
  `mod messages`, `mod members`, `mod voice`, `mod moderation`,
  `mod context_menu`, `mod plugin_settings`. Each module re-attaches
  to the impl via `impl XxxClient { … }` blocks. No public API change.
- **Step B (cross-crate, riskier):** Split `ClientBackend` into
  capability traits — `MessagingBackend`, `VoiceBackend`,
  `ModerationBackend`, `PluginSettingsBackend`, `ContextMenuBackend`.
  Backends implement only the ones they support; consumers ask via
  `Option<&dyn VoiceBackend>` instead of probing
  `caps.voice != VoiceSupport::None` and then calling a method that
  returns `NotSupported`.

**Effort: Step A — M (mechanical, ~1 day per backend, parallelisable
across worktrees because the four backends don't share files). Step B —
L (touches every consumer including UI, persona MCP, and tests).**

**Why this beats alternatives:** Step A pays off the second a backend
maintainer needs to find "where does Discord handle X" — currently
they grep a 2k-line file. Step B kills the `NotSupported` smell
permanently and makes adding a new backend (e.g. the in-flight reddit
work) a typed scope rather than a "implement 60 stubs" chore. Step A
is the prerequisite for Step B and stands alone if Step B never lands.

---

### 3. `mcp/chat-mcp/src/tools.rs` (4081 lines) — 80+ `handle_*` fns + dispatch

**What's mixed:** One file owns —

1. **Tool schema** — `tool_list()` + `tool_list_for_backend()` (lines
   213-1399 — that's 1186 lines of JSON schema builders).
2. **A dispatch table** — `dispatch()` at line 1400 routes ~80 tool
   names to ~80 `handle_*` functions with a flat `match`.
3. **80+ handler implementations**, naturally grouped:
   - Auth (login/logout/test_signin/test_lifecycle): lines 1511-1975.
   - Backend wrappers (list_servers/channels/dms, send_message,
     send_typing, etc.): 1586-1750.
   - Plugin/sidebar/menu/composer/message-action handlers: 1750-2230.
   - **Memory** handlers (remember_fact, recall_facts, store_chat_note,
     get_reply_context, …): 2232-2421.
   - **Drafts** (create/list/approve/edit/discard): 2448-2589.
   - **Chat style** (set/get/list/forget): 2590-2632.
   - **Subscriptions / events / typing simulation / unread**: 2632-2912.
   - **Persona** meta-tools (list/get/create/update/delete/sources/
     tool_whitelist/invoke/heartbeat/memory/audit_query/audit_export):
     2913-3383 — own-grouped, persona/quality-gate plans already
     identify these.
   - **Client settings** (list/get/set version + mechanism): 3384-3700.
4. Plus `audit()` / `audit_client_settings()` cross-cutting helpers.

The persona-quality-gate plan (CLAUDE.md "Persona-subsystem footguns")
already proves split groups exist — the lint scripts grep
`handle_meta_persona_*` as a closed family. It's already a
sub-namespace; just promote it to an actual module.

**Split shape:**

```
mcp/chat-mcp/src/tools/
    mod.rs                # dispatch() + tool_list() + tool_list_for_backend()
    schema.rs             # the 1186-line JSON tool schema builders
    auth.rs               # login/logout/test_*
    backend.rs            # list_servers/channels/dms, send_message, send_typing
    plugin_sidebar.rs     # plugin/sidebar/menu/composer/message-action handlers
    memory.rs             # remember_fact + recall + chat_note + chat_summary + reply_context
    drafts.rs
    chat_style.rs
    events.rs             # subscribe/unsubscribe/poll + typing simulation + unread summary
    persona/              # promote handle_meta_persona_* group
        mod.rs
        crud.rs
        sources_and_whitelist.rs
        invoke.rs
        memory.rs
        audit.rs          # audit_query + audit_export
    client_settings.rs
    audit_helpers.rs      # audit() + audit_client_settings()
```

**Effort: M (mechanical move, no behaviour change; ~3-5 days).**

**Why this beats alternatives:** The persona-quality-gates already
treat these as families via regex lints — the lints get simpler
(per-module scopes instead of pattern-match across one giant file),
and any new MCP tool author won't have to scroll past 80 unrelated
handlers to find the right place to add their tool.

---

### 4. `clients/client/src/types.rs` (1891 lines) — kitchen-sink type pile

**What's mixed:** 70+ public types in one file spanning eight unrelated
type families (line numbers from the survey above):

- Backend identity / capabilities — `BackendId`, `BackendCapabilities`
  (15 fields, line 722, the second-worst god struct), `CapabilityRow`,
  `MessagingModel`, `DmSupport`, `FriendModel`, `NotificationSupport`,
  `LandingPage`, `VoiceSupport`, `HostCap`, `Mechanism`,
  `PluginManifest`, `SignupMethod`, `ContainerLabelForm`. (lines 12-1097.)
- Connection / auth — `ConnectionStatus`, `AccountPresence`,
  `AuthCredentials`, `Session`, `Account`. (lines 240-1571.)
- Server / channels / forums — `Server` (16 fields, line 426, also a
  god struct), `Category`, `Channel`, `ChannelType`, `ForumTag`,
  `ThreadInfo`, `ThreadMetadata`, `ForumPost`, `ForumSortOrder`,
  `UpdateChannelParams`. (lines 426-1140, 1726-1738.)
- Files — `FileKind`, `FileEntry`, `FileContent`, `ExecOutput`. (598-645.)
- Messaging — `MessageContent`, `Attachment`, `MessageReplyPreview`,
  `Message`, `Reaction`, `CustomEmoji`, `StickerItem`, `MessageQuery`,
  `MessageSearchQuery`, `MessageSearchHit`. (1133-1339.)
- Users / DMs / groups — `User`, `PresenceStatus`, `Group`,
  `DmChannel`. (1340-1403.)
- Notifications — `Notification`, `NotificationKind`. (1404-1453.)
- Content policy / moderation — `SensitiveContentLevel`,
  `DmSpamFilterLevel`, `ContentPolicy`, `BlockedUser`,
  `MemberPermissions`, `MemberRole`, `BannedMember`,
  `ModerationLogEntry`. (1454-1750.)
- Voice — `VoiceParticipant`, `VoiceConnectionKind`,
  `VoiceConnection`. (1574-1637.)
- Commands — `CommandScope`, `ChatCommand`. (1638-1675.)

**Split shape:**

```
clients/client/src/types/
    mod.rs            # `pub use` re-exports for backwards compat
    backend.rs        # BackendId + Capabilities + Mechanism + PluginManifest
    auth.rs           # Session, Account, AuthCredentials, ConnectionStatus
    server.rs         # Server, Category, Channel, ForumTag, ThreadInfo, ForumPost
    file.rs           # FileEntry, FileContent, FileKind, ExecOutput
    message.rs        # Message, MessageContent, Attachment, Reaction, …
    user.rs           # User, PresenceStatus, Group, DmChannel
    notification.rs
    moderation.rs     # ContentPolicy, MemberPermissions, BannedMember, …
    voice.rs
    command.rs        # CommandScope, ChatCommand
```

**Effort: S/M — pure file shuffle plus a `mod.rs` re-export. Zero
behaviour change. Workspace-wide compile re-ack.**

**Why this beats alternatives:** Every backend crate `use`s this
module. Right now adding a single field forces an `impl` block 1800
lines below the struct definition, no IDE jump for the cluster of
related types, and Discord-specific moderation types live next door
to voice-call structs. Cheap, mechanical, immediate quality-of-life
win for every backend author.

---

### 5. `mcp/chat-mcp/src/memory.rs` (2695 lines) — eight schemas + one `MemoryDb`

**What's mixed:** A single `MemoryDb` struct (line 31) bundles eight
distinct SQLite schemas (extracted from `run_migrations`):

| Lines | Table | Purpose |
|------:|------|---------|
| 57-67 | `contact_facts` | Per-user facts the agent remembers |
| 69-79 | `chat_notes` | Per-chat freeform notes |
| 80-89 | `chat_summaries` | Per-chat distilled summaries |
| 90-102 | `drafts` | Pending outbound message drafts |
| 103-117 | `chat_style` | Per-chat tone / persona |
| 118-132 | `personas` | Persona definitions |
| 133-145 | `persona_sources` | Per-persona memory source bindings |
| 146-151 | `persona_tool_whitelist` | Per-persona allowed tool slugs |
| 152-165 | `persona_facts` | Persona-scoped facts |
| 166-174 | `persona_outbound_allowlist` | Throttle / target rules |
| 175-195 | `persona_audit` | Forensic trail (covered by lint P2) |
| 196-218 | `client_settings_audit` | Per-backend setting changes |

The `impl MemoryDb` block (line 35) has 57 `pub fn`s, including
several outsized ones — `update_persona` (102 lines, 9 params),
`set_chat_style` (77 lines, 9 params), `query_persona_audit` (81
lines, 11 params), `record_persona_audit` (37 lines, 10 params),
`create_persona` (39 lines, 10 params). The 8+-param hits are
documented in section A.1's footer search above.

**Split shape:**

```
mcp/chat-mcp/src/memory/
    mod.rs              # MemoryDb opens conn, owns migrations registration
    error.rs            # MemoryError + From impls
    migrations.rs       # all CREATE TABLE / migration registration
    facts.rs            # remember_fact / recall_facts / forget_fact / search_facts
    chat_notes.rs
    chat_summaries.rs
    drafts.rs
    chat_style.rs       # ChatStyle + set/get/list/forget
    persona/
        mod.rs          # MemoryDb-attached persona ops re-exported
        crud.rs         # create_persona / update_persona (currently 102-line monster)
        sources.rs      # add_persona_source + list
        whitelist.rs    # tool_whitelist
        facts.rs        # add_persona_fact + queries
        outbound.rs     # outbound_allowlist + count_outbound_sends_today
        audit.rs        # record_persona_audit + query/export
    client_settings_audit.rs
    helpers.rs          # now_iso8601, days_to_ymd, drain, bind_opt_str, collect_*
```

The `pub fn update_persona(persona_slug, name, …, 9 params)` and
sister `create_persona(10 params)` should also become builder/struct-
arg patterns at the same time — `PersonaUpdate { … }.apply(db)` —
since the parameter explosion is its own SRP smell.

**Effort: M (~3 days). The `audit()` helpers in `tools.rs` already
expect a flat `MemoryDb` interface, so re-exports from `mod.rs` keep
the call sites unchanged.**

**Why this beats alternatives:** Persona-quality-gate plans (the P1/
P2/P4 lints from CLAUDE.md) already grep the persona-scoped tables as
a closed group; pulling `persona/` into its own subdir simplifies the
lint regexes and prevents future drift. Schema splits prepare the
ground for per-table connection pooling if SQLite lock contention
ever appears.

---

## A.2 — Other notable but lower-priority offenders

- **`mcp/web-devtools-mcp/src/main.rs:196-1728`** — `ChromeCdpBackend`
  + `impl DevtoolsBackend for ChromeCdpBackend` is a 700-line trait
  impl wrapping a 830-line struct; mixes CDP wire protocol, Chrome
  process management, build-status tracking, log buffering, watchdog,
  CLI dispatch. Same shape as backend split #2 — extract `cdp_wire`,
  `chrome_process`, `build_status`, `log_buffer`, `cli_dispatch`
  modules.
- **`crates/core/src/state.rs`** — eleven distinct `*ContextMenuState`
  structs (lines 397-541) + `AppState` (25 fields, line 542) +
  `NavigationState`. The eleven menu states would compress nicely to
  one `ActiveContextMenu { kind: ContextMenuKind, position: Position,
  payload: ContextMenuPayload }` enum-tagged structure. Already partly
  there (`ActiveContextMenu` at line 397 looks like the start) — finish
  the unification.
- **`crates/core/src/ui/favorites_sidebar.rs`** — three giant
  components in one file: `AccountIcon` (458 lines), `FavoriteServerIcon`
  (291 lines), `FavoritesBar` (260 lines). Named explicitly in
  `plan-component-lints.md` as the canonical offender; the lint forbids
  *new* growth but doesn't migrate the existing three. Each one of
  these is independently a Top-5 candidate within the apps/web target.
- **`crates/core/src/ui/search.rs`** — `SearchPage` (368 lines),
  `AccountFilter` (9 params), `AvatarNodeRow` (8 params),
  `ServerNode` (10 params). Param explosion is the SRP signal: each
  multi-arg fn is doing two jobs — composing AND threading parent state.
- **`crates/core/src/ui/settings/plugins.rs:376`** — `PluginsSettings`
  (351 lines, blowing the cap). Mixes plugin list rendering with
  install / configure / remove flows.
- **`crates/core/src/ui/account/common/dm_context_menu.rs:37`** —
  `DmContextMenu` (349 lines). Context-menu plan covers detection;
  splitting the body is still TODO.
- **`crates/core/src/ui/account/common/media_viewer.rs:26`** —
  `MessageMediaViewerOverlay` (313 lines).
- **`crates/core/src/ui/agent/persona/edit_modal.rs:365`** —
  `PersonaEditModal` (281 lines), with `IdentitySection` (8 params).
- **`crates/core/src/ui/account/common/user_profile_modal.rs:108`** —
  `UserProfileModal` (243 lines).
- **`crates/core/src/ui/account/common/saved_items_view.rs:106`** —
  `SavedItemsView` (238 lines).
- **`crates/core/src/ui/client_ui/view/list_body.rs:67` (234 lines),
  `tree_body.rs:47` (227), `split_body.rs:32` (197),
  `toolbar.rs:51` (195)** — entire `client_ui/view/` cluster is over
  cap. Each "body" is paired with its own toolbar; canonical SRP split
  is body↔toolbar↔model trio per layout type.
- **`crates/core/src/ui/account/common/channel_list.rs:832` —
  `ServerChannelView` (232 lines).** Already noted in CLAUDE.md as a
  prior hang site (RwLock comment block 193-195). Splitting helps
  bisect future hangs.
- **`crates/core/src/ui/agent/persona/{outbound_allowlist_editor,
  talk_to_overlay,audit_panel}.rs`** — 192-225 line components in
  the persona subtree. Persona is a discrete subsystem; refactor
  alongside the `mcp/chat-mcp/src/persona/` split (item 5).
- **`apps/poly-host/src/lib.rs`** (2044 lines) — Router + KV/HTTP/
  exec/plugins/accounts handlers + state struct + path resolution + a
  tests block (1129-2025) that's nearly half the file. Tests should
  go to `tests/` or sub-mods; production code splits to `state.rs`,
  `router.rs`, `handlers/{kv,plugin_kv,http,exec,plugins,accounts}.rs`.
- **`crates/core/src/ui.rs:1652` — `App` component (232 lines).**
  Outside chat_view, this is the second-largest single-file component.
  Worth splitting into `App`-shell + `AppRoutes` once Top-5 #1 lands.
- **`crates/core/src/ui/account/common/notifications.rs:403` —
  `NotificationItemContent` (213 lines, 9 params).**
- **`crates/core/src/ui/voice_banner.rs:115` — `VoiceBannerChannelLink`
  (11 params).** Worst param count in the UI. Likely two responsibilities
  (link rendering + channel-state probing).
- **`crates/core/src/storage.rs:202` — `AppSettings` struct (24 fields).**
  Splits naturally by settings tab (privacy, notifications, voice,
  plugins, theme).

---

## A.3 — Things that LOOK oversize but are FINE (do not waste cycles)

- **`clients/demo/src/data.rs` (5806 lines).** This is fixture/seed
  data — 47 of its 92 functions are `pub fn demo*_servers/channels/
  messages/notifications/users(…) -> Vec<…>`, returning hand-crafted
  test data. 235 `Message {…}`/`Channel {…}`/`Server {…}` literals.
  The "function" granularity (one fn per fixture group) is correct;
  splitting into `data/{servers,channels,messages,…}.rs` would only
  swap one navigational convention (jump-by-symbol) for another
  (jump-by-file) at no SRP win. **Skip unless it grows past 8k.**
- **`clients/demo/src/lib.rs` (1874 lines).** Holds three different
  demo backends (`DemoClient`, `DemoClient2`, `DemoClient3`) for
  cat/dog/forum demo modes. Each `impl ClientBackend for DemoClient*`
  is large but the file's *organising principle is clear* — one demo
  variant per impl block, ~700 lines each. The right split, if any,
  is `demo1.rs` / `demo2.rs` / `demo3.rs`, but the gain is marginal
  because the three demos diverge by data, not by code shape, and the
  shared helpers (`record_sent_message`, etc.) are already in
  `data.rs`. **Skip — not enough payoff for the disruption.**
- **`mcp/chat-mcp/src/tools.rs` lines 213-1399 — `tool_list()` JSON
  schema.** It's ~1186 lines of one-shot table-driven JSON-schema
  declarations. Looks horrible; behaves fine. Item A.1 #3 still
  recommends moving this to `schema.rs` for navigation, but **don't
  try to split it further into per-tool modules** — the schema is
  intentionally one document so a single grep can verify shape parity
  across all tools. One file, one responsibility (= "the MCP wire
  schema"), even though it's long.
- **`crates/core/src/ui/routes.rs` `#[component]`s (`ServerChat` 107,
  `ServerHome` 88, `ServerMediaViewerRoute` 87, `DmsHome` 75,
  `DmChat` 60, others smaller).** None over the 150-line cap. The 2515-
  line file feels heavy because it lists ~40 `#[component]`s back-to-
  back, but each one is a thin route adapter that delegates to a real
  view. Splitting per-route would multiply file count without breaking
  any single responsibility apart. **Skip.**
- **`mcp/chat-mcp/src/memory.rs` test module (lines 1902-2695, 793
  lines).** All `#[cfg(test)] mod tests` — by SRP, the testing
  responsibility is *separate from* the production code, but Rust
  convention puts tests next to source for the in-module access. Don't
  pull these out unless the production-side split (item A.1 #5)
  inspires per-table test files anyway.
- **`mcp/web-devtools-mcp/src/main.rs` `dispatch_web_cli` (~1835)
  switch tower.** Genuinely table-driven CLI dispatch; flattening to
  one file is the right shape. The CDP backend below it is the SRP
  problem (covered in A.2), not the dispatcher.
- **`apps/poly-host/src/lib.rs` lines 1129-2025 — tests block.** Same
  reasoning as the memory test block — 900 lines of tests at the
  bottom of the file are correct-as-is; the production half (lines
  1-1128) is what needs the split called out in A.2.
- **`clients/client/src/types.rs` derived enums with 5+ variants (e.g.
  `BackendCapabilities`, `MessagingModel`, `DmSupport`, …).** These
  are domain-modelling enums; the variant count reflects the problem
  space, not bad code. Split the *file* (item A.1 #4); leave the
  enums alone.

---

## Evidence table — where each claim came from

| Claim | Source |
|------|--------|
| chat_view 6809 lines, 35-field signals, 70-field markup ctx | `wc -l`, `grep -n "^struct ChatView"`, manual read of L949+L1902 |
| Twelve `use_*_effect` hooks in chat_view | `grep -n "^fn use_.*_effect"` lines 1248-1869 |
| Hang-class #8 `use_history_state_effect` is in this file | CLAUDE.md hang-class table + chat_view.rs:1676 |
| Backend impl block sizes | `grep -c "async fn " clients/*/src/lib.rs` |
| ChromeCdpBackend impl 701 lines | `awk` brace-walk on `impl DevtoolsBackend for ChromeCdpBackend` |
| 80+ handle_* in tools.rs | `grep -n "^[a-z]*async fn handle_" mcp/chat-mcp/src/tools.rs` |
| Memory.rs has 8 schemas in one struct | `grep -n "CREATE TABLE" mcp/chat-mcp/src/memory.rs` |
| God-struct list (15+ fields) | python brace-walk over `crates/core/src` + `clients/client/src` + `mcp/chat-mcp/src` |
| 150-line component cap | `docs/plans/plan-component-lints.md` (DONE) |
| Component-cap offenders ranked | python brace-walk over `#[component]` blocks in `crates/core/src/ui/**` |
| 8+-param functions list | python regex over fn signatures across all source roots |

End of shard A.
