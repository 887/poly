# Phase 2.20 — Plugin Capabilities System

> Status: **Planning**
> Owner: TBD
> Last updated: 2026-04-11

## Problem

Poly's UI and MCP were written Discord-first. Every connected account — regardless of backend — renders the same navigation tabs (Chat / Friends / Notifications / Voice / Create-Server), the same notification filter dropdown (All / Mentions / Friend-requests / Server-invites / Voice-invites / Other), and the same MCP tool surface (`list_friends`, `list_dms`, `send_message`, …). For read-only feed plugins like Hacker News, and for posting-but-not-social plugins like Lemmy and GitHub, that's nonsense: the tabs are empty, the filter categories are empty, and the MCP tools silently return `[]`.

The research showed that the primitive for fixing this already exists: `BackendCapabilities` at `clients/client/src/types.rs:394-456` with 10 flags, and a default impl on the `ClientBackend` trait at `clients/client/src/lib.rs:369`. But **only GitHub overrides it**, and **nothing in the UI or MCP reads it**. Every other backend implicitly claims to support everything, and the host then hardcodes the Discord shape on top.

Phase 2.20 wires the capability model through end-to-end, extends it so that plugins can declare the shapes they *actually* have, and makes the UI, routing, and MCP all adapt per-plugin.

---

## Capability matrix (from public API research)

Legend: **Y** = supported, **N** = not supported, **L** = limited (see per-platform note).

| Concept | HN | Lemmy | GitHub | Matrix | Teams | Discord | Stoat |
|---|---|---|---|---|---|---|---|
| Top-level container (server/team/instance/org) | — | Instance | Org | — | Team | Guild | Server |
| Sub-container (channel/room/community/repo/story list) | Story list | Community | Repo + Discussions/Issues | Room | Channel + Chat | Channel | Channel |
| Writable (can post content) | N | Y | Y | Y | Y | Y | Y |
| DMs (1:1) | N | Y (`private_message`) | N | Y (1:1 rooms w/ `m.direct`) | Y (`/me/chats` oneOnOne) | L (bot DMs discouraged) | Y |
| Friends / user graph | N | N (follow communities, not users) | Followers, not friends | N | N | L (bots: no) | Y |
| Unified notifications inbox | N | Y (split: replies / mentions / PMs) | Y (`/notifications` ~15 reason codes) | Y (push-rules driven) | L (no read endpoint; sub only) | L (gateway-push only) | Y |
| Voice / video | N | N | N | Y (WebRTC + TURN) | Y (Cloud Comms) | Y (UDP Opus) | ? |
| Reactions | N | Votes only | Fixed emoji (8) | Free emoji via `m.annotation` | Fixed emoji | Free emoji | Y |
| Typing indicators | N | N | N | Y | N | Y | Y |
| Presence | N | N | N | Y | Y | Y (gateway) | Y |
| Threads / replies | Nested `kids[]` | Nested | Issue/PR threads | `m.thread` | Channel-only | Thread channels | Y |
| Search messages | N | Y | Y (code search) | Y | Y | Y | Y |

**Key insights for design:**
- *Friends* is a rare feature (Discord/Stoat/Matrix-partial). The default should be **off**, not on.
- *Notifications* exists for most platforms but **the categories are platform-specific**. A Discord-style "Voice invite" filter is meaningless on Lemmy, and a GitHub-style "ci_activity" filter is meaningless on Discord. Categories must be plugin-declared, not host-hardcoded.
- *Voice* exists on four of seven platforms. Default: **off**.
- *Reactions* have three shapes: votes-only, fixed emoji set, free emoji. A single boolean is insufficient — we need an enum.
- *DMs* have three shapes: none, limited (bot-policy constraints), full. Again, enum.
- Lemmy's "inbox" is **three separate endpoints** (`/user/replies`, `/user/mentions`, `/private_message/list`) — the capability system should let the plugin list the categories it cares about instead of pretending they come from one source.

---

## Defects (what's broken today)

Numbered so work packages can reference them.

### D1 — Hardcoded nav tabs in `account_server_bar.rs`
**File:** `crates/core/src/ui/account/common/account_server_bar.rs:174-189`
**What:** `AccountBarDmsButton`, `AccountBarFriendsButton`, `AccountBarNotifsButton`, `CreateServerButton` are all rendered unconditionally.
**Impact:** HN account shows Friends + DMs + Notifications + Voice tabs, all empty.

### D2 — Hardcoded notification filter enum
**File:** `crates/core/src/ui/account/common/notifications.rs:24-32`
**What:** `enum NotificationMenuFilter { All, Mentions, FriendRequests, ServerInvites, VoiceInvites, Other }` is private, fixed, and unrelated to which backend is active.
**Impact:** HN/Lemmy/GitHub accounts see "Voice invites" and "Friend requests" filter chips that are structurally empty.

### D3 — Hardcoded notification categories in locales
**File:** `locales/en/main.ftl:~126, ~154-170`
**What:** Strings like `notifications-server-invite = You've been invited to {$server}` and `create-server-btn = Create Server` are generic, so every backend reuses "Server" even when it should say "Community" (Lemmy), "Space" (Matrix), "Team" (Teams), or "Repo" (GitHub).
**Impact:** Confusing and wrong for non-Discord backends.

### D4 — MCP tool list has no capability filter
**File:** `mcp/chat-mcp/src/tools.rs:41-222` (and `dispatch` at 226-247)
**What:** `tool_list()` publishes every tool to every caller; `dispatch` calls the backend method unconditionally and an unsupported call returns `Ok(vec![])` from the no-op default — which looks like success with empty data.
**Impact:** AI agents calling `list_friends` on HN get `[]`, can't distinguish "no friends" from "concept doesn't exist".

### D5 — Account routes are unconditional
**File:** `crates/core/src/ui/routes.rs:122-328`
**What:** `DmsHome`, `DmChat`, `FriendsRoute`, `NotificationsRoute`, `CreateServerRoute`, `VoiceRoute` are all defined on every account. Typing a URL like `/hackernews/.../friends` is routable and renders the empty shell.
**Impact:** Deep-links to unsupported routes render an empty page instead of 404/redirect.

### D6 — `BackendCapabilities` exists but is never read
**File:** `clients/client/src/types.rs:394-456`, `clients/client/src/lib.rs:369`
**What:** The struct has 10 flags (`supports_voice`, `supports_video`, `supports_dms`, `supports_groups`, `supports_send_messages`, `supports_presence`, `supports_search`, `supports_reactions`, `supports_typing_indicators`, `supports_file_upload`). The trait default is `ALL` (every flag `true`). Only `clients/github/src/lib.rs` overrides it. Zero call sites read it.
**Impact:** The infrastructure exists but is dead code. Every non-GitHub plugin silently claims full support.

### D7 — No notification-category declaration
**What:** There's no way for a plugin to say "I emit notifications of kind X, Y, Z" — and conversely, there's no way for the host to ask. Defect 2 can't be fixed without this.
**Impact:** No migration path to per-plugin filters.

### D8 — No per-plugin terminology
**What:** Plugins can ship FTL (see phase 2.19 work) but there's no conventional key for "what is a server called on this backend" that the host can substitute into generic strings like "Create {container}".
**Impact:** Defect 3 can't be fixed without a terminology convention.

### D9 — `is_forum()` is a hardcoded slug match, not a capability
**File:** `clients/client/src/types.rs:65-70`
**What:** `pub fn is_forum(&self) -> bool { matches!(self.0.as_str(), "demo_forum" | "hackernews" | "lemmy" | "github") }`
**Impact:** Any new forum-shaped backend needs to be added to this list. It's a capability expressed as a hardcoded allowlist.

### D10 — Empty result is treated as "no items", not as "feature missing"
**File:** `crates/core/src/ui/account/common/notifications.rs`, `friends.rs`, `dms.rs` call sites
**What:** `get_friends() → Ok(vec![])` renders an empty grid with zero explanation. The user can't tell whether their HN account has no friends or whether HN has no friend concept.
**Impact:** Confusing UX; the empty-state placeholder doesn't reflect why it's empty.

### D11 — Send box / composer is always shown
**What:** Read-only backends (HN, maybe GitHub-read-only) still render the message composer at the bottom of channel views. Submitting sends an error-returning request instead of disabling the box.
**Impact:** Feels broken — the UI lets the user type and hit enter on a backend that rejects it.

### D12 — `backend_capabilities()` default is `ALL`
**File:** `clients/client/src/lib.rs:369-371`
**What:** Defaulting to "supports everything" is the opposite of safe — a plugin that forgets to declare its capabilities silently claims to support voice, video, DMs, etc.
**Impact:** Makes D6 much worse: plugins opt *out* of features by overriding, instead of opting *in*.

---

## Target architecture

### A. Extend `BackendCapabilities` into a richer declaration

Keep the name, but split the flat booleans into a structured form that covers the real variations. Proposal:

```rust
pub struct BackendCapabilities {
    // --- Container shape ---
    pub containers: ContainerModel,      // None | Flat | Nested (top-level + sub)
    pub container_label: ContainerLabel, // "Server" | "Space" | "Team" | "Community" | "Instance" | "Repo" | Custom(String)
    pub channel_label: ChannelLabel,     // "Channel" | "Room" | "Stream" | Custom(String)
    pub supports_create_server: bool,
    pub supports_create_channel: bool,

    // --- Messaging ---
    pub messaging: MessagingModel,       // ReadOnly | Writable
    pub supports_send_messages: bool,    // aligns with MessagingModel::Writable
    pub supports_attachments: bool,
    pub supports_threads: ThreadModel,   // None | Nested | ChannelOnly | First-class
    pub supports_replies: bool,

    // --- Social graph ---
    pub dms: DmSupport,                  // None | Limited | Full
    pub friends: FriendModel,            // None | Followers | MutualFriends
    pub presence: bool,
    pub typing_indicators: bool,

    // --- Notifications ---
    pub notifications: NotificationSupport, // None | InboxWithCategories(Vec<NotificationCategoryId>)

    // --- Interaction primitives ---
    pub reactions: ReactionModel,        // None | VotesOnly | FixedEmoji(Vec<String>) | FreeEmoji
    pub voice: VoiceSupport,             // None | Audio | AudioVideo
    pub search_messages: bool,

    // --- Discovery ---
    pub advertised_mcp_tools: Vec<&'static str>,  // names of MCP tools this plugin handles
}
```

The **default** becomes `BackendCapabilities::READ_ONLY_FEED` — the most restrictive — not `ALL`. Plugins opt in, not out. GitHub/HN stay with the default; Lemmy adds `messaging: Writable`, `dms: Full`, `notifications: InboxWithCategories([…])`; Discord/Matrix/Stoat go up to the full set.

### B. Per-plugin notification categories

Each backend declares its own categories; the host merges the categories from every active account and renders the filter dropdown dynamically.

```rust
pub struct NotificationCategory {
    pub id: &'static str,          // "mention" | "ci_activity" | "reply" | "private_message" | ...
    pub label_key: &'static str,   // FTL key, from the plugin's own bundle
    pub icon: &'static str,        // emoji
    pub matches: fn(&NotificationKind) -> bool, // classifier
}
```

Stored in `BackendCapabilities.notifications`. The host replaces the hardcoded `NotificationMenuFilter` enum in `notifications.rs` with a merged list from every active account's capabilities — plus an "All" synthetic filter.

### C. Per-plugin terminology via FTL convention

Every plugin bundle defines these keys when they apply:

```
plugin-<id>-container-label = Community     # or Space, Team, Repo, …
plugin-<id>-container-label-plural = Communities
plugin-<id>-subcontainer-label = Post
plugin-<id>-create-container = Create community
```

The host's generic strings (`create-server-btn`, `notifications-server-invite`) are rewritten to use a `{container}` argument resolved from the active backend's FTL at render time. Fallback: the host's own "Server".

### D. Route-level capability gating

Routes stay defined in `routes.rs`, but each one is **guarded** at the component level: the first thing `FriendsView`, `NotificationsView`, `DmsView`, `VoiceView` do is pull `BackendCapabilities` for the active account and, if the capability is missing, redirect to the account's home route with a toast ("This backend doesn't support friends"). Direct URL navigation to unsupported routes redirects instead of rendering empty.

The nav buttons in `account_server_bar.rs` are gated the same way — they simply don't render when the capability is missing.

### E. MCP capability-aware tool list

Two options; the plan picks **option 2**:

1. **Filter `tool_list()` per-request** — requires the MCP caller to pass an account context, which MCP spec doesn't natively support.
2. **Expose a `list_plugin_tools` tool** that takes `{backend, account_id}` and returns the subset of tools supported, plus a machine-readable "why not" for the ones excluded. Also have `dispatch()` return a proper `NotSupported` error (not `Ok([])`) when the backend doesn't declare the capability.

Option 2 keeps the static tool list (MCP-spec-compliant) but makes discovery and errors honest.

### F. `is_forum()` → capability-derived

Delete the hardcoded slug match. Replace with `caps.containers == ContainerModel::Flat && caps.messaging == Writable` (or similar — pick the actual semantics it's currently shielding).

---

## Work packages

Each WP is self-contained, gets a sonnet-tier coding agent in a worktree, and has a clear acceptance test. Dependencies are noted.

### WP-1 — Redesign `BackendCapabilities` struct (foundation)
**Scope:** `clients/client/src/types.rs`, `clients/client/src/lib.rs`
**What:** Replace the flat bool struct with the enum-based structure from section A above. Define `READ_ONLY_FEED`, `MESSAGING_NO_SOCIAL`, `FULL_SOCIAL_CHAT` presets. Change the trait default from `ALL` to `READ_ONLY_FEED`. Add `backend_capabilities()` overrides to every plugin (`stoat`, `matrix`, `discord`, `teams`, `lemmy`, `hackernews`, `github`, `demo`, `server-client`) matching the capability matrix above.
**Blocked by:** none
**Blocks:** WP-2, WP-3, WP-4, WP-5, WP-6, WP-8, WP-9
**Acceptance:** `cargo check --target wasm32-unknown-unknown`; each plugin's capabilities-override unit-tested in its own crate; host compiles unchanged (no call sites yet).

### WP-2 — Delete `is_forum()`, replace with capability check
**Scope:** `clients/client/src/types.rs:65-70`, all `is_forum()` call sites
**What:** Grep for every `is_forum()` call, replace with the equivalent capability read. Delete the method.
**Blocked by:** WP-1
**Acceptance:** `grep -r is_forum` is empty; unread-badge behaviour in `account_server_bar.rs` unchanged for HN/Lemmy/GitHub.

### WP-3 — Nav-button capability gating
**Scope:** `crates/core/src/ui/account/common/account_server_bar.rs:174-189`
**What:** Read `ClientManager.get_backend(account_id).backend_capabilities()` and conditionally render `AccountBarDmsButton` / `AccountBarFriendsButton` / `AccountBarNotifsButton` / `CreateServerButton` / voice button. Add a WASM-safe helper `capabilities_for(account_id)` on `ClientManager` that returns a cheap snapshot.
**Blocked by:** WP-1
**Acceptance:** Screenshot test (haiku test harness) — HN account sidebar shows only Chat + Stories nav buttons, not Friends/DMs/Notifications/Voice/Create-Server. Discord account still shows everything.

### WP-4 — Route-level redirect guards
**Scope:** `crates/core/src/ui/account/common/dms.rs`, `friends.rs`, `notifications.rs`, `voice.rs`, `create_server.rs`
**What:** Each view's `#[component]` fn opens by reading the active account's capabilities and redirecting to `Route::AccountHome` if the corresponding capability is missing. Show a transient toast ("{backend} doesn't support friends"). Deep-links to `/hackernews/.../friends` must redirect.
**Blocked by:** WP-1, WP-3
**Acceptance:** Manual URL test via web MCP — typing `/hackernews/news.ycombinator.com/hn-anonymous/notifications` redirects to the HN story list, not to an empty notifications page.

### WP-5 — Notification category registry
**Scope:** `clients/client/src/types.rs`, `crates/core/src/ui/account/common/notifications.rs:24-32`
**What:** Replace the hardcoded `NotificationMenuFilter` enum with a dynamic list built from `BackendCapabilities.notifications` across every active account. Each category references a plugin-owned FTL label key. The "All" synthetic filter is always present when at least one category exists.
**Blocked by:** WP-1
**Acceptance:** With only HN + GitHub + Discord active, the filter dropdown shows Discord's categories (Mentions, Friend-requests, Server-invites, Voice-invites, Other) *and* GitHub's (Mention, Review-requested, CI, Security-alert, …) — HN contributes none; the Voice-invites filter disappears when the Discord account is removed.

### WP-6 — Plugin-provided terminology
**Scope:** `clients/*/locales/en/plugin.ftl` (all 8 plugins), `locales/en/main.ftl`, `crates/core/src/i18n/*`, call sites of `create-server-btn`, `notifications-server-invite`, etc.
**What:** Define FTL convention `plugin-<id>-container-label[-plural]` and `plugin-<id>-subcontainer-label[-plural]`. Rewrite host strings that embed "server" / "channel" to use a `{$container}` argument fed from the active backend's FTL. Ship per-plugin overrides for Lemmy (Community), Matrix (Space), Teams (Team), GitHub (Repo), HN (Story).
**Blocked by:** none (parallel with WP-1)
**Acceptance:** Switching to a Lemmy account, the "+" tooltip reads "Create community"; switching to Matrix it reads "Create space".

### WP-7 — Composer/send-box read-only gating
**Scope:** `crates/core/src/ui/channel_content/message_composer.rs` (or wherever the composer lives)
**What:** Read `capabilities.messaging`; if `ReadOnly`, replace the composer with a small inline notice ("{backend} is read-only"). The notice is keyed on `plugin-<id>-read-only-notice` with a generic fallback.
**Blocked by:** WP-1
**Acceptance:** HN story view shows the read-only notice, no composer; Discord/Stoat unchanged.

### WP-8 — MCP `list_plugin_tools` + honest error for unsupported dispatch
**Scope:** `mcp/chat-mcp/src/tools.rs`, `mcp/chat-mcp/tests/mcp_integration.rs`
**What:** Add a `list_plugin_tools` MCP tool that takes `{backend, account_id}` and returns the list of tool names the backend supports, each with a one-line reason (`"backend is read-only"`, `"no friends concept"`, etc.). In `dispatch()`, read the capability for the tool being called; if the backend lacks it, return an `isError: true` content block with the reason instead of calling the no-op default. Add a test: `list_plugin_tools` for HN returns `["list_servers", "list_channels", "get_messages", "get_user"]`; `list_friends` on HN returns `isError: true` with reason "hackernews has no friends concept".
**Blocked by:** WP-1
**Acceptance:** New integration test passes; existing tests still green.

### WP-9 — "Feature unsupported" empty-state placeholders
**Scope:** `crates/core/src/ui/account/common/friends.rs`, `dms.rs`, `notifications.rs` (whatever empty-state components they use)
**What:** When a capability is missing *and the user somehow lands on the view anyway* (WP-4 redirects are the main path, this is the fallback), render a friendly "This backend doesn't have X" placeholder instead of an empty grid. Plugin-keyed copy.
**Blocked by:** WP-1
**Acceptance:** Toggling a capability flag in-app (dev toggle) swaps the view between the real UI and the placeholder.

### WP-10 — Capability test harness
**Scope:** `crates/core/tests/capabilities_matrix.rs`, `TEST_HARNESS.md`
**What:** Write a matrix test that instantiates each plugin, reads its declared capabilities, and asserts they match a `expected_capabilities.json` fixture (stored next to the test). Catches regressions where someone upgrades a plugin and forgets to update its declaration.
**Blocked by:** WP-1
**Acceptance:** `cargo test -p poly-core capabilities_matrix` passes for all 8 plugins.

### WP-11 — Continuous test plan (unit + integration + Playwright)
**Scope:** `docs/plans/phase-2.20-test-plan.md` (standalone), per-plugin `tests/capabilities.rs`, `mcp/chat-mcp/tests/mcp_integration.rs` additions, new `tests/capabilities/*.spec.ts` Playwright suites, `TEST_HARNESS.md` step 6.
**What:** Author the companion test plan that describes *how* WP-1…WP-10 get tested — unit tests per plugin for declared capabilities, MCP integration tests for `list_plugin_tools` and NotSupported dispatch, Playwright specs for nav gating / route redirects / composer read-only / notification filters / terminology, and a poly-web MCP capability smoke step. Extend `TEST_HARNESS.md` so `cargo check`, `clippy`, and `cargo test` all cover every plugin crate, chat-mcp, and the plugin-loader crate — not just `poly-core`.
**Blocked by:** interleaved with every other WP; this one runs continuously.
**Acceptance:** `TEST_HARNESS.md` completes green after each wave; test plan is kept in sync with implementation WPs as scope shifts.
**Reference:** `docs/plans/phase-2.20-test-plan.md`

---

## Execution order

Serial (WP-1 is foundational): **WP-1**
Then parallel: **WP-2, WP-3, WP-6, WP-8** (each sonnet agent in its own worktree)
Then parallel: **WP-4, WP-5, WP-7, WP-9**
Finally: **WP-10**

**WP-11 runs continuously** — the test plan (`phase-2.20-test-plan.md`) is authored
before wave 1 starts, then each wave ships its tests alongside the implementation
PRs. A haiku subagent runs `TEST_HARNESS.md` after every wave.

Rough estimate: 2 days of sonnet-agent time if WP-1 lands in ~4 hours.

## Out of scope for this phase

- Actual network capability discovery (asking the backend "what do you support" at runtime) — the declaration is static in the plugin binary. Dynamic discovery is phase 2.21.
- WASM-plugin capability declaration through WIT — this plan is for native backends first. WASM plugins inherit the new `BackendCapabilities` via a WIT record in a follow-up.
- Per-*account* (vs per-*backend*) capability overrides — e.g., a GitHub account whose PAT lacks `notifications` scope should downgrade its declared capabilities. Nice-to-have for phase 2.21.
- Rewriting the routes table to generate routes from declared capabilities. The current plan guards existing routes; route synthesis is a bigger refactor for phase 2.22.
