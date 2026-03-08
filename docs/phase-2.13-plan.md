# Phase 2.13 — DMs, Group DMs & Rich Demo Data

> **Status:** ✅ Complete  
> **Started:** 2026-03-04  
> **Completed:** 2026-03-04  
> **Goal:** First-class DM and Group DM experience with rich, individualized demo data and visual group member management.

---

## Overview

Phase 2.13 builds out the DM and Group DM features to a production-quality baseline:
- **Rich demo data** — every DM contact gets a unique conversation thread; every group gets a community-appropriate name and realistic message history; every demo server gets enough messages to showcase the chat UI properly.
- **Group chat UI improvements** — DM and group chat headers distinguish between individual DMs (user avatar + status) and group chats (member count + members button). Right sidebar shows group members.
- **Group member management** — a slide-in member panel for group chats shows member list with presence indicators and a "Remove" action (demo: updates local state; real backends: calls new `remove_group_member` API).
- **New group DM** — "New Group DM" button starts a group with selected contacts.

---

## Architecture Decisions

| ID | Decision |
|---|---|
| D2.13-A | Groups use the existing `DmChat` route (`dm_id = group_id`). No new route added — the route is already semantically correct for any non-server conversation. |
| D2.13-B | Right sidebar in `DmsLayout` is toggled by a new `dm_right_sidebar_visible` field in `NavState`, separate from the server's `right_sidebar_visible`. |
| D2.13-C | `remove_group_member` is added to `ClientBackend` with a default `Err(NotSupported)` impl. Demo client updates `ChatData::groups` locally and re-renders. |
| D2.13-D | Chat header detects DM vs Group by checking whether `current_server` is `None` and `current_channel` has a `server_id` of `""` (synthesized). Group detection: `channel_id` appears in `ChatData::groups`. |
| D2.13-E | Demo group IDs are prefixed `"group-"` and demo DM IDs are prefixed `"dm-"` so detection is trivial without a separate flag. |

---

## Checklist

### 2.13.1 — Rich Demo DM Messages
- [x] Add `demo_dm_messages(dm_channel_id: &str) -> Vec<Message>` to `data.rs`
  - `dm-user-alice`: conversation about the Poly project (code review, excitement)
  - `dm-user-bob`: casual chat (gaming, weekend plans)
  - `dm-user-charlie`: technical help (Rust lifetime errors)
  - `dm-user-diana`: design feedback on themes
  - `dm-user-eve`: reminder about a meeting
- [x] Update `DemoClient::get_messages` for dm-* IDs to call `demo_dm_messages` instead of `demo_messages`
- [x] Update `DemoClient2::get_messages` similarly for its DM IDs

### 2.13.2 — Rich Demo Groups (Cat Account)
- [x] Add `demo_groups_v2()` with 4 themed groups:
  - `group-rust-study`: "Rust Study Group" — Alice, Bob, Charlie (Poly Dev community) — messages: discussing lifetimes, sharing docs
  - `group-weekend-warriors`: "Weekend Warriors" — Diana, Eve, Frank (Gaming Lounge crossover) — messages: game night scheduling
  - `group-midnight-jams`: "Midnight Jams" — Grace, Henry (Music Enthusiasts) — messages: sharing playlists
  - `group-team-poly`: "Poly Core Team" — Alice, Bob, Charlie, Diana (dev discussions) — messages: sprint planning
- [x] Add `demo_group_messages(group_id: &str) -> Vec<Message>` with 4-8 messages per group
- [x] Update `DemoClient::get_messages` to route `group-*` IDs through `demo_group_messages`
- [x] Update `DemoClient::get_groups` to use `demo_groups_v2()`

### 2.13.3 — Groups for Dog Account (Demo2)
- [x] Add `demo2_groups()` with 3 themed groups:
  - `group2-oss-contributors`: "OSS Contributors" — Alice, Bob, Charlie (Open Source Hub)
  - `group2-bookworms`: "Bookworms" — Diana, Eve (Book Club crossover)
  - `group2-meal-prep`: "Meal Prep Squad" — Frank, Grace, Henry (Cooking Corner)
- [x] Add messages for each `demo2` group in `demo_group_messages()`
- [x] Update `DemoClient2::get_groups` to return `demo2_groups()`
- [x] Update `DemoClient2::get_messages` to route `group2-*` IDs through `demo_group_messages`
- [x] Set correct `account_id = DEMO2_ACCOUNT_ID` on all demo2 groups

### 2.13.4 — Group Member Sidebar
- [x] Add `dm_right_sidebar_visible: bool` to `NavState` (default `false`)
- [x] Add `active_group_members: Vec<User>` to `ChatData` (populated when a group is clicked)
- [x] Create `crates/core/src/ui/account/common/dm_user_sidebar.rs`:
  - `DmUserSidebar` component — shown when DM layout + `dm_right_sidebar_visible = true`
  - Shows "Members" header + count
  - Lists all members with avatar, display name, presence dot
  - For groups: shows "Remove" button (trash icon) per member excluding self
  - Remove action calls `remove_group_member()` on backend and removes from local state
- [x] Wire `DmUserSidebar` into `DmsLayout` (right column when `dm_right_sidebar_visible`)

### 2.13.5 — Chat Header: DM / Group Differentiation
- [x] Extract `ChatHeader` into a separate component or sub-function in `chat_view.rs`
- [x] DM header (when `channel_id` starts with `"dm-"`):
  - Show colored avatar circle (initial letter, no `#` prefix) + user display name
  - Show online presence status dot from `ChatData::members` or demo user list
  - **No** `#` prefix in the channel name
- [x] Group header (when `channel_id` starts with `"group-"`):
  - Show `👥 {group_name}` (no `#` prefix)
  - Show member count pill  
  - Show "Members" toggle button (sets `dm_right_sidebar_visible`)
- [x] Server channel header (current behavior) unchanged

### 2.13.6 — `remove_group_member` Backend Method
- [x] Add `remove_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()>` to `ClientBackend` trait with default `Err(ClientError::NotSupported("remove_group_member".to_string()))`
- [x] Demo implementation: returns `Ok(())` (UI updates local state on success)
- [x] Add i18n strings: `group-members-title`, `group-member-remove`, `group-member-remove-confirm`
- [x] Add same strings to DE/FR/ES locale files (placeholders ok)

### 2.13.7 — Rich Demo Server Messages (Remaining Servers)
- [x] Dog account servers are sparse — add richer messages:
  - `ch2-general` (Open Source Hub): 500+ messages, links/images/reactions, explicit scroll-up pagination load-test channel
  - `ch2-announcements` (Open Source Hub): 6+ messages, code review discussion
  - `ch2-contributions` (Open Source Hub): 4+ messages, PR links
  - `ch2-recommendations` (Book Club): 6+ messages, book opinions
  - `ch2-recipes` (Cooking Corner): 6+ messages, recipe sharing
  - `ch2-techniques` (Cooking Corner): 4+ messages, technique tips
  - `ch2-workouts` (Fitness Crew): 6+ messages, workout tracking
  - `ch2-nutrition` (Fitness Crew): 4+ messages, meal logs
- [x] Add cat account `ch-rust` and `ch-dioxus` messages (currently fallback generic messages)
- [x] Add `ch-production` (Music) messages

### 2.13.8 — Visual Verification
- [x] Launch app via web-devtools MCP
- [x] Switch to cat account (demo-cat), verify DMs:
  - Click Alice DM → unique conversation shown
  - Click Bob DM → different conversation
  - Click Charlie DM → Rust help thread
  - Click Diana DM → design feedback thread
  - Click Eve DM → meeting reminder thread
- [x] Verify Groups (cat account):
  - Click "Rust Study Group" → messages visible, member list button in header
  - Click member list button → sidebar opens with member list
  - Click "Remove" on a member → member disappears
  - Click "Weekend Warriors", "Midnight Jams", "Poly Core Team" → each has unique messages
- [x] Navigate dog account's servers:
  - Open Source Hub: click `#general`, `#announcements`, `#contributions` → rich messages
  - `#general`: confirm initial open lands at the bottom of the recent window, unread banner + unread divider render, and scrolling to the top prepends older history
  - Book Club: current-read, recommendations → rich discussion
  - Cooking Corner: recipes, techniques → rich content
  - Fitness Crew: workouts, nutrition → rich content
- [x] Navigate dog account's groups:
  - "OSS Contributors", "Bookworms", "Meal Prep Squad" → unique messages each
- [x] Navigate cat account's servers:
  - Poly Dev: general, off-topic (rich), rust-help (now rich), dioxus (now rich)
  - Gaming Lounge: minecraft (rich), valorant (now rich)
  - Music Enthusiasts: recommendations (rich), production (now rich)
- [x] Take screenshots of at least 3 views to confirm rendering
- [x] Run `cargo cranky --workspace` — zero warnings
- [x] Run `cargo check -p poly-web --target wasm32-unknown-unknown`

---

## i18n Keys Required

```fluent
# Group DMs
group-members-title = Members
group-member-remove = Remove
group-member-remove-tooltip = Remove {$name} from this group

# DM header
dm-header-subtitle = Direct Message
```

---

## Files to Create / Modify

| File | Action |
|---|---|
| `docs/phase-2.13-plan.md` | Create (this file) |
| `clients/demo/src/data.rs` | Add: `demo_dm_messages`, `demo_group_messages`, `demo_groups_v2`, `demo2_groups`, enrich `demo2_messages` |
| `clients/demo/src/lib.rs` | Update: `get_messages` routing for DM/group IDs; `get_groups` for both clients |
| `clients/client/src/types.rs` | Add: `remove_group_member` default impl to `ClientBackend` |
| `crates/core/src/state/mod.rs` | Add: `dm_right_sidebar_visible` to `NavState` |
| `crates/core/src/state/chat_data.rs` | Add: `active_group_members: Vec<User>`, populate on group click |
| `crates/core/src/ui/account/common/dm_user_sidebar.rs` | Create: `DmUserSidebar` component |
| `crates/core/src/ui/account/common/mod.rs` | Export `DmUserSidebar` |
| `crates/core/src/ui/account/common/chat_view.rs` | Update header: DM vs Group vs Server differentiation |
| `crates/core/src/ui/account/common/channel_list.rs` | Update: populate `active_group_members` on group click |
| `crates/core/src/ui/routes.rs` | Update `DmsLayout`: add right sidebar column |
| `locales/en/main.ftl` | Add: group-members-title, group-member-remove |
| `locales/de/main.ftl` | Add: German translations |
| `locales/fr/main.ftl` | Add: French translations |
| `locales/es/main.ftl` | Add: Spanish translations |

---

## Session Log

### Session 1 (2026-03-04)
- Created plan document.
- Added `ClientError::NotSupported` + `remove_group_member` default trait method.
- Added `dm_right_sidebar_visible` to `NavState`, `active_group_members` to `ChatData`.
- Added i18n strings to all 4 locales (en/de/fr/es).
- Built `demo_dm_messages` — 5 unique personalized DM conversations.
- Built `demo_groups_v2` — 4 themed cat account groups (Rust Study Group, Weekend Warriors, Midnight Jams, Poly Core Team).
- Built `demo2_groups` — 3 themed dog account groups (OSS Contributors, Bookworms, Meal Prep Squad).
- Built `demo_group_messages` — unique messages for all 7 groups.
- Built `demo2_messages_rich` — rich messages for 10 previously sparse channels.
- Updated both DemoClient and DemoClient2 routing (prefix-based dispatch: `dm-*`, `group-*`, `group2-*`).
- Created `DmUserSidebar` + `DmMemberRow` components with presence dots and Remove button.
- Wired `DmsLayout` to show sidebar panel, `GroupChannelItem` to populate members, `DMChannelItem` to clear state.
- Updated chat header in `chat_view.rs` to differentiate DM (avatar + name), Group (👥 + count + Members toggle), Server (unchanged `# channel-name`).
- Added CSS for all new classes: `dm-user-sidebar`, `dm-chat-header-info`, `dm-chat-avatar`, `group-chat-icon`, `dm-member-row`, `dm-member-remove-btn`, etc.
- Verified via web-devtools MCP: DM headers render correctly (Alice → colored A avatar + "Direct Message"), group headers render correctly ("Rust Study Group" + "3 Members" + toggle button opens member sidebar), server channels unchanged (`# announcements`).
- `cargo cranky --workspace` → zero warnings. WASM check clean. `cargo fmt` applied.

### Session 2 (2026-03-08)
- Added Dog account `Open Source Hub / #general` as a heavy load-test channel with 560 deterministic demo messages, mixed links, images, reactions, and a latest-message ID of `msg2-general-559`.
- Raised the server/channel unread metadata so `#general` opens with an 11+ unread state and a realistic unread banner/divider.
- Replaced default full-history loads for server channels and DMs with bounded recent-page queries (`limit` based on unread context) so chats now open on the recent tail instead of loading the entire history.
- Wired the existing near-top scroll hook in `chat_view.rs` to real `before`-cursor pagination, including scroll-offset preservation after prepending older messages.
- Added a shared `chat_history.rs` helper module for initial message queries, unread-divider placement, scroll metrics, and scroll restoration.
- Added Discord-style unread UI inside the message list: sticky blue unread banner + red unread divider + Mark as Read action.
- Verified live in the running web build:
  - fresh open of `#general` rendered 36 recent messages (`524..559`) with `distanceFromBottom ≈ 0.22`
  - unread banner + unread divider both present
  - top-scroll fetch prepended older history (`count 36 → 132`, first message `524 → 428`) while preserving viewport offset
- Desktop-devtools MCP bridge did not become reachable in this environment even after restart/manual launch attempts, so visual verification was completed on the web devtools build as a best-effort fallback.
