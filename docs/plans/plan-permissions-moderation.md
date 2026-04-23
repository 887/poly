# Plan — Permissions, Ownership, and Moderation

> **Created:** 2026-04-21
> **Last updated:** 2026-04-24
> **Status:** ✅ DONE — implementation shipped across 4 waves:
>
> - Wave 0 (verification): commit `1ffcf5bb46df` — Stoat + Lemmy endpoint verification, plan deltas applied, `permissions-moderation-verification.md` written.
> - Wave 1 (shared scaffolding): commit `4d5f90a58f7c` — `ClientBackend` trait gains 11 moderation methods + `get_server_roles` (added later in Wave 2); new types (`MemberPermissions`, `BannedMember`, `UpdateChannelParams`, `ModerationLogEntry`, `ModerationAction`, `Role`); 6 capability flags (`has_roles/kick/ban/timed_ban/channel_mgmt/moderation_log`) + 6 `should_show_*` predicates; UI shells for `RolesTab` / `BansTab` / `ModLogTab` / `EditChannelDialog` / `KickMemberDialog` / `BanMemberDialog` / `TimeoutMemberDialog`; FTL keys in en/de/es/fr; `MessageContextMenu` Delete + `UserRowContextMenu` Kick/Ban/Timeout wired.
> - Wave 2 (Discord + Matrix + Lemmy + Forgejo + GitHub): commit `8c9367bc20d7` — superset (concurrent worktree auto-rebase converged 5 agents into one). Discord: full mod surface incl. native timeout via `communication_disabled_until`. Matrix: power-level-based redact/kick/ban (no native timeout). Lemmy: ban with native `expires` (`timeout_member` = thin wrapper). Forgejo + GitHub: minimal — `delete_message` + `get_my_permissions` only, rest `NotSupported`. Dialogs + tabs populated; `ModerationDialog` enum + `active_moderation_dialog` AppState field + `ModerationDialogOverlay` in MainLayout.
> - Wave 3 (Stoat + Teams + poly-server): commit `c23018d42755` — superset with same converged-rebase pattern. Stoat: native timeout via `DataMemberEdit.timeout` (Wave-0-verified) + slow-mode field rename `slow_mode_secs` → `slowmode`. Teams: `kick_member` via `DELETE /teams/{t}/members/{m}` + `softDelete` for messages + `update_channel` (name + description only); `ban_member` / `reorder_channels` / `get_moderation_log` `NotSupported`; `EditChannelDialog` gates slow-mode and NSFW fields when `backend_slug == "teams"`. poly-server: server-side SQLite migration adds `role` column + `server_bans` + `server_modlog` tables; `RoleTier` middleware (Owner=3 / Admin=2 / Mod=1 / Member=0) gates 11 new REST endpoints; client trait impls + `WirePermissions` / `WireBanRecord` / `WireModlogEntry`. SurrealDB parity flagged TODO.
>
> Test counts (final): poly-stoat 24, poly-teams 28 (+12), poly-server-client 21 (+19), poly-discord +11 mod, poly-matrix +9 mod, poly-lemmy 30 (+10), poly-forgejo 25 (+3), poly-github 45 (+4), poly-server +15 (server-side mod), poly-client capability matrix updated. Verification report: `docs/plans/permissions-moderation-verification.md`.
>
> **Predecessor:** `docs/plans/plan-ui-polish-round-2.md` (Round 2, ✅ DONE)

---

## Section 0 Header

This plan covers the full-stack implementation of permissions, ownership, and moderation
across every Poly backend. It is the source of truth for a multi-week implementation push.
The plan is structured so individual backend phases can be delegated to parallel worktree
subagents once the shared host work (Section 5) lands.

**Scope guard:** This plan does NOT cover SSO/SAML, federated cross-instance moderation
(e.g. Matrix server ACLs propagating to other homeservers), or the Discord permission-overrides
matrix editor. Those are future work with explicit notes in Section 7.

---

## Section 1 Backend Research Summaries

### Section 1.1 Discord

**Sources:**
- https://docs.discord.com/developers/topics/permissions
- https://docs.discord.com/developers/resources/guild

#### Permission Model

Discord uses an integer **bitfield** per role. The `@everyone` role sets guild-wide baseline
permissions; additional roles OR their bits in. Channel-level overwrites then apply per
channel/category as `allow` and `deny` masks. The final effective permission for a user is:

```
base = @everyone.permissions
for each role in member.roles: base |= role.permissions
for each overwrite: apply allow/deny
```

Key moderation-relevant permission flag values (string-serialized `i64`, shown as `1 << N`):

| Flag name          | Bit shift | Decimal value |
|--------------------|-----------|---------------|
| `KICK_MEMBERS`     | 1         | 2             |
| `BAN_MEMBERS`      | 2         | 4             |
| `ADMINISTRATOR`    | 3         | 8             |
| `MANAGE_CHANNELS`  | 4         | 16            |
| `MANAGE_GUILD`     | 5         | 32            |
| `MANAGE_MESSAGES`  | 13        | 8192          |
| `MANAGE_ROLES`     | 28        | 268435456     |
| `MODERATE_MEMBERS` | 40        | 1099511627776 |

`MODERATE_MEMBERS` (`1 << 40`) is the "timeout" permission.
`ADMINISTRATOR` bypasses all overwrites.

Roles have a `position` integer; bots/users can only manage roles strictly below their highest
role. Roles are ordered lowest-to-highest by position.

#### Ownership Model

Each guild has exactly one owner (the user who created it). The owner cannot be kicked or banned
and has all permissions regardless of roles. Ownership can be transferred via
`PATCH /guilds/{guild.id}` with `owner_id`. Guild deletion requires ownership.

#### Moderation Actions and REST Endpoints

| Action              | Method   | Endpoint                                                              | Key params                                   | Required perm         |
|---------------------|----------|-----------------------------------------------------------------------|----------------------------------------------|-----------------------|
| Kick member         | DELETE   | `/guilds/{guild.id}/members/{user.id}`                                | —                                            | `KICK_MEMBERS`        |
| Ban member          | PUT      | `/guilds/{guild.id}/bans/{user.id}`                                   | `delete_message_seconds` (0-604800), `reason`| `BAN_MEMBERS`         |
| Unban member        | DELETE   | `/guilds/{guild.id}/bans/{user.id}`                                   | —                                            | `BAN_MEMBERS`         |
| List bans           | GET      | `/guilds/{guild.id}/bans`                                             | `limit`, `before`, `after`                   | `BAN_MEMBERS`         |
| Timeout member      | PATCH    | `/guilds/{guild.id}/members/{user.id}`                                | `communication_disabled_until` (ISO8601)     | `MODERATE_MEMBERS`    |
| Delete message      | DELETE   | `/channels/{channel.id}/messages/{message.id}`                        | —                                            | `MANAGE_MESSAGES` or own |
| Bulk delete msgs    | POST     | `/channels/{channel.id}/messages/bulk-delete`                         | `messages: [id...]` (max 100, max 14d old)   | `MANAGE_MESSAGES`     |
| Update channel      | PATCH    | `/channels/{channel.id}`                                              | `name`, `topic`, `nsfw`, `position`, `rate_limit_per_user`, `permission_overwrites` | `MANAGE_CHANNELS` |
| Reorder channels    | PATCH    | `/guilds/{guild.id}/channels`                                         | `[{id, position, parent_id?}]`               | `MANAGE_CHANNELS`     |
| Get guild roles     | GET      | `/guilds/{guild.id}/roles`                                            | —                                            | —                     |
| Modify role         | PATCH    | `/guilds/{guild.id}/roles/{role.id}`                                  | `permissions`, `name`, `position`            | `MANAGE_ROLES`        |
| Get my perms        | GET      | `/guilds/{guild.id}/members/@me`                                      | Response includes `roles`; compute via client| —                     |
| Get audit log       | GET      | `/guilds/{guild.id}/audit-logs`                                       | `action_type`, `user_id`, `limit`            | `VIEW_AUDIT_LOG`      |

**Slow mode:** set via `PATCH /channels/{channel.id}` with `rate_limit_per_user` (0-21600 seconds).
**NSFW gate:** set via `PATCH /channels/{channel.id}` with `nsfw: true`.
**Lock channel:** set `permission_overwrites` to deny `SEND_MESSAGES` for `@everyone`.

**Reference UI:** https://support.discord.com/hc/en-us/articles/206029707

---

### Section 1.2 Matrix

**Sources:**
- https://spec.matrix.org/v1.11/client-server-api/ (room membership, power levels, redactions)
- https://matrix.org/docs/communities/moderation/

#### Permission Model

Matrix uses **power levels** — integer values per-user per-room. The `m.room.power_levels`
state event defines:

```json
{
  "ban": 50,
  "events": { "m.room.name": 100, "m.room.power_levels": 100 },
  "events_default": 0,
  "invite": 50,
  "kick": 50,
  "redact": 50,
  "state_default": 50,
  "users": { "@alice:example.com": 100 },
  "users_default": 0
}
```

Default values (when key absent): `ban=50`, `kick=50`, `invite=50`, `redact=50`, `state_default=50`, `events_default=0`, `users_default=0`.

Common tiers in practice:
- `0` — regular user
- `50` — moderator (can kick, ban, redact, change room settings)
- `100` — administrator (can modify power levels, encryption, history visibility)

**Authorization rules (spec-normative):**
- To kick: sender's level ≥ `kick` level AND sender's level > target's level.
- To ban: sender's level ≥ `ban` level AND sender's level > target's level.
- To unban: sender's level ≥ max(`kick`, `ban`) AND sender's level > target's level.
- To redact another user's event: sender's level ≥ `redact` level.

There is no "timeout" concept in the Matrix spec. Temporary mutes are implemented by
removing `SEND_MESSAGES` from the user's permission level for a period, or via bots like
Mjolnir/Draupnir.

#### Ownership Model

Room creators start at power level 100 by default. There is no single "owner" concept — any
user at level 100 can promote others to 100. Ownership transfer = promote new user to 100,
demote self. Room deletion is not a first-class operation (rooms are federated; local servers
can forget rooms but they persist on other servers).

#### Moderation Actions and REST Endpoints

| Action               | Method | Endpoint                                                              | Request body / key params                   |
|----------------------|--------|-----------------------------------------------------------------------|---------------------------------------------|
| Kick member          | POST   | `/_matrix/client/v3/rooms/{roomId}/kick`                              | `{"user_id":"@alice:ex.com","reason":"..."}`|
| Ban member           | POST   | `/_matrix/client/v3/rooms/{roomId}/ban`                               | `{"user_id":"@alice:ex.com","reason":"..."}`|
| Unban member         | POST   | `/_matrix/client/v3/rooms/{roomId}/unban`                             | `{"user_id":"@alice:ex.com"}`               |
| Redact event (delete)| PUT    | `/_matrix/client/v3/rooms/{roomId}/redact/{eventId}/{txnId}`          | `{"reason":"..."}`                          |
| Set power levels     | PUT    | `/_matrix/client/v3/rooms/{roomId}/state/m.room.power_levels`         | full `m.room.power_levels` content          |
| Update room name     | PUT    | `/_matrix/client/v3/rooms/{roomId}/state/m.room.name`                 | `{"name":"new name"}`                       |
| Update room topic    | PUT    | `/_matrix/client/v3/rooms/{roomId}/state/m.room.topic`                | `{"topic":"new topic"}`                     |
| Get my power level   | GET    | `/_matrix/client/v3/rooms/{roomId}/state/m.room.power_levels`         | — (response includes `users` map)           |
| Get room members     | GET    | `/_matrix/client/v3/rooms/{roomId}/members`                           | —                                           |

**Slow mode / rate limiting:** Matrix has no native slow-mode equivalent. Synapse implements
a `m.room.message_burst_count` unstable event type but it is not in the stable spec as of v1.11.
This is out-of-scope for Poly — note as N/A.

**NSFW gating / channel lock:** Not a native Matrix concept. Rooms can be restricted via
join rules or space membership, but there is no "nsfw" boolean on rooms.

**Moderation Log:** Matrix has no server-side moderation log. Mjolnir/Draupnir (bots) maintain
their own ban lists as room state events in a dedicated "policy room". Poly should surface
the `m.ban` and `m.kick` events from the `m.room.power_levels` change history as a proxy
moderation log. This is documented as a limitation in Section 7.

**Reference UI:** https://matrix.org/docs/communities/moderation/ (Mjolnir/Draupnir setup)

---

### Section 1.3 Stoat (formerly Revolt)

**Sources:**
- https://developers.stoat.chat/developers/api/permissions/ (permission bitfield values)
- https://github.com/stoatchat/stoatchat (issue #291 — permission extensibility feature request)
- https://publicapi.dev/revolt-api (Revolt/Stoat API summary)

#### Permission Model

Stoat uses an integer **bitfield** (unsigned 64-bit). Permissions exist at both server level
and channel level. Channel-level settings override server-level using an allow/deny mask
(same pattern as Discord). Roles have a `rank` (position) determining hierarchy.

Known permission flag values (from official documentation at
https://developers.stoat.chat/developers/api/permissions/):

| Flag name              | Value (dec) | Bit |
|------------------------|-------------|-----|
| `ManageChannel`        | 1           | 0   |
| `ManageServer`         | 2           | 1   |
| `ManagePermissions`    | 4           | 2   |
| `ManageRole`           | 8           | 3   |
| `ManageCustomisation`  | 16          | 4   |
| `KickMembers`          | 64          | 6   |
| `BanMembers`           | 128         | 7   |
| `TimeoutMembers`       | 256         | 8   |
| `AssignRoles`          | 512         | 9   |
| `ChangeNickname`       | 1024        | 10  |
| `ManageNicknames`      | 2048        | 11  |
| `ChangeAvatar`         | 4096        | 12  |
| `RemoveAvatars`        | 8192        | 13  |
| `ViewChannel`          | 1048576     | 20  |
| `ReadMessageHistory`   | 2097152     | 21  |
| `SendMessage`          | 4194304     | 22  |
| `ManageMessages`       | 8388608     | 23  |
| `ManageWebhooks`       | 16777216    | 24  |
| `InviteOthers`         | 33554432    | 25  |
| `SendEmbeds`           | 67108864    | 26  |
| `UploadFiles`          | 134217728   | 27  |
| `React`                | 536870912   | 29  |
| `Connect` (voice)      | 1073741824  | 30  |
| `Speak` (voice)        | 2147483648  | 31  |
| `MuteMembers` (voice)  | 8589934592  | 33  |
| `DeafenMembers` (voice)| 17179869184 | 34  |
| `MoveMembers` (voice)  | 34359738368 | 35  |

#### Ownership Model

Each server has a single `owner` field (user ID). The owner holds all permissions implicitly.
Server transfer is possible via a dedicated API call. Deletion requires ownership.

#### Moderation Actions and REST Endpoints

The Stoat API inherits directly from Revolt's API surface. Based on known Revolt API patterns
(confirmed via https://publicapi.dev/revolt-api and the Go SDK at
https://pkg.go.dev/within.website/x/web/revolt):

| Action               | Method  | Endpoint                                          | Key params                                        |
|----------------------|---------|---------------------------------------------------|---------------------------------------------------|
| Kick member          | DELETE  | `/servers/{server_id}/members/{member_id}`        | —                                                 |
| Ban member           | PUT     | `/servers/{server_id}/bans/{user_id}`             | `{reason?: string, delete_message_seconds?: i64}` |
| Unban member         | DELETE  | `/servers/{server_id}/bans/{user_id}`             | —                                                 |
| List bans            | GET     | `/servers/{server_id}/bans`                       | —                                                 |
| Delete message       | DELETE  | `/channels/{channel_id}/messages/{msg_id}`        | Requires `ManageMessages` or message authorship   |
| Update channel       | PATCH   | `/channels/{channel_id}`                          | `name`, `description`, `nsfw`, `active`           |
| Get server roles     | GET     | `/servers/{server_id}`                            | `roles` field in response                         |
| Set member roles     | PATCH   | `/servers/{server_id}/members/{member_id}`        | `{roles: [role_id, ...]}`                         |

**NOTE on endpoint verification:** The Stoat developer documentation site returned 404 for
direct endpoint listing pages during research. The kick/ban endpoints above follow the Revolt
API structure which Stoat inherits (Stoat was Revolt; the codebase is the same). The implementer
should verify these against https://developers.stoat.chat/developers/api/reference.html/ before
coding. If the endpoint paths changed in the Stoat rebranding, adjust accordingly.

**Timeout:** Verified. Endpoint is `PATCH /servers/{server_id}/members/{member_id}` with body
`{"timeout": "<ISO8601 expiration datetime>"}`. Clear an active timeout via
`{"remove": ["Timeout"]}` in the same endpoint. Permission flag: `TimeoutMembers`
(bit 8, value 256). Source: `DataMemberEdit` in `revoltchat/api` schema.ts.

**Slow mode:** Verified. `PATCH /channels/{channel_id}` with `{"slowmode": <seconds>}`.
Field name is `slowmode` (uint64, max 21600 = 6 hours). Source: `DataEditChannel` in
`revoltchat/api` schema.ts.

**Reference UI:** https://stoat.chat (the official Stoat client app)

---

### Section 1.4 Microsoft Teams

**Sources:**
- https://learn.microsoft.com/en-us/graph/api/resources/channel?view=graph-rest-1.0
- https://learn.microsoft.com/en-us/graph/api/team-delete-members?view=graph-rest-1.0
- https://learn.microsoft.com/en-us/graph/api/chatmessage-softdelete?view=graph-rest-1.0

#### Permission Model

Teams uses a two-tier **owner/member** model per team:
- **Owner**: Can add/remove members, manage team settings, create/delete channels, delete messages
- **Member**: Can send messages, view channels; cannot manage membership or settings

Channel-level membership exists for **private channels** and **shared channels** only — standard
channels inherit the team's member list. There is no per-channel permission overwrite system
like Discord.

Microsoft Graph API requires OAuth2 delegated (work/school account) or application permissions.
Key permission scopes for moderation:
- `TeamMember.ReadWrite.All` — add/remove team members
- `TeamMember.ReadWriteNonOwnerRole.All` — restricted (cannot touch owners)
- `ChannelMessage.ReadWrite` — soft-delete channel messages
- `Channel.ReadWrite.All` — create/update/delete channels

**Important limitation:** `Delegated (personal Microsoft account)` is listed as "Not supported"
for member management and message deletion. Poly's Teams plugin uses Bearer token auth which
targets work/school accounts — personal accounts cannot perform moderation actions via Graph.

#### Ownership Model

Each team has one or more owners. The first owner is the team creator. Owners can add other
owners. There is no single "primary owner" distinct from co-owners. Team deletion requires
owner permission.

#### Moderation Actions and REST Endpoints

| Action                   | Method | Endpoint                                                                             | Required scope/perm                       |
|--------------------------|--------|--------------------------------------------------------------------------------------|-------------------------------------------|
| Remove team member       | DELETE | `/teams/{team-id}/members/{membership-id}`                                           | `TeamMember.ReadWrite.All`                |
| List team members        | GET    | `/teams/{team-id}/members`                                                           | `TeamMember.Read.All`                     |
| Update member role       | PATCH  | `/teams/{team-id}/members/{membership-id}`                                           | `TeamMember.ReadWrite.All`                |
| Soft-delete channel msg  | POST   | `/teams/{teamId}/channels/{channelId}/messages/{msgId}/softDelete`                   | `ChannelMessage.ReadWrite` (delegated)    |
| Soft-delete reply        | POST   | `/teams/{teamId}/channels/{channelId}/messages/{msgId}/replies/{replyId}/softDelete` | `ChannelMessage.ReadWrite`                |
| Update channel           | PATCH  | `/teams/{team-id}/channels/{channel-id}`                                             | `Channel.ReadWrite.All`                   |
| Delete channel           | DELETE | `/teams/{team-id}/channels/{channel-id}`                                             | `Channel.ReadWrite.All`                   |
| Archive channel          | POST   | `/teams/{team-id}/channels/{channel-id}/archive`                                     | Owner or `Channel.ReadWrite.All`          |

**No kick/ban concept:** Teams has no server-side "kick" (temporary removal) or "ban" (permanent
block) at the Graph API level for standard teams. Removing a member (`DELETE /teams/{id}/members/{id}`)
simply removes them; they can be re-added. There is no `ban` endpoint or blocklist for teams.
This is a hard limitation of the Microsoft Graph API and applies to both personal accounts
(unsupported entirely) and work accounts.

**No channel reorder:** Microsoft Graph API does not expose a channel position/reorder endpoint.
Channel order in Teams is managed client-side by the Teams native app, not via Graph.

**Slow mode equivalent:** Not available in Graph API.

**Moderation log:** No Graph API endpoint for Teams audit log on the moderation-actions level.
Microsoft 365 has compliance center audit logging (separate admin product) that is out of scope.

**Reference UI:** https://support.microsoft.com/en-us/office/teams-owner-member-and-guest-capabilities

---

### Section 1.5 Lemmy

**Sources:**
- https://mv-gh.github.io/lemmy_openapi_spec/ (unofficial OpenAPI spec for Lemmy v0.19/v1.0)
- https://join-lemmy.org/news/2025-02-03_-_Breaking_Changes_in_Lemmy_1.0

#### Permission Model

Lemmy uses a **positional role** system (not a bitfield):
- **Admin** (site-wide): can do anything including banning users from the entire instance
- **Moderator** (community-level): can ban/remove content within their community
- **User**: can post/comment; no moderation powers

A user can be a moderator for some communities and a regular user in others. Moderator status
is per-community and stored in the `community_moderator` table.

#### Ownership Model

Communities have a list of moderators; the first moderator is implicitly the "owner" though
the API treats all mods as equal. The site admin supersedes all community mods. There is no
explicit owner field separate from the moderators list.

#### Moderation Actions and REST Endpoints

Lemmy v1.0 retains `/api/v3/` compatibility while also exposing `/api/v4/` (both work).

| Action                     | Method | Endpoint                        | Required params                                              | Required role  |
|----------------------------|--------|---------------------------------|--------------------------------------------------------------|----------------|
| Ban from community         | POST   | `/api/v3/community/ban_user`    | `community_id`, `person_id`, `ban: bool`, `expires?: i64`, `reason?: string`, `remove_data?: bool` | Community mod  |
| Site-wide ban              | POST   | `/api/v3/user/ban`              | `person_id`, `ban: bool`, `expires?: i64`, `reason?: string`, `remove_data?: bool` | Admin          |
| Remove post (mod)          | POST   | `/api/v3/post/remove`           | `post_id`, `removed: bool`, `reason?: string`                | Community mod  |
| Remove comment (mod)       | POST   | `/api/v3/comment/remove`        | `comment_id`, `removed: bool`, `reason?: string`             | Community mod  |
| Add mod to community       | POST   | `/api/v3/community/mod`         | `community_id`, `person_id`, `added?: bool`                  | Community mod  |
| Get modlog                 | GET    | `/api/v3/modlog`                | `community_id?`, `mod_person_id?`, `page?`, `limit?`         | Public         |
| Get community              | GET    | `/api/v3/community`             | `name` or `id`; response includes `moderators`               | —              |

**No kick concept:** Lemmy has no "kick" — community membership is implicit (subscribe/unsubscribe).
Banning with `ban=true` prevents a user from posting in that community; there is no temporary
removal without a content impact.

**Temporary bans:** The `expires` field on ban endpoints accepts a Unix timestamp for
automatic expiration. This is the Lemmy equivalent of "timeout".

**Channel management:** Lemmy communities ("servers") have no sub-channels. `update_channel`
maps to `PUT /api/v3/community` for community settings, not to a per-channel concept.

**Reorder channels:** N/A — no sub-channels in Lemmy.

**Modlog `type_` values (verified):** `"All"`, `"ModRemovePost"`, `"ModLockPost"`,
`"ModFeaturePost"`, `"ModRemoveComment"`, `"ModRemoveCommunity"`, `"ModBanFromCommunity"`,
`"ModAddCommunity"`, `"ModTransferCommunity"`, `"ModAdd"`, `"ModBan"`, `"ModHideCommunity"`,
`"AdminPurgePerson"`, `"AdminPurgeCommunity"`, `"AdminPurgePost"`, `"AdminPurgeComment"`.
Use `type_=ModBanFromCommunity` for community-level ban lists; `type_=ModBan` is site-wide
admin bans only. The response is a `GetModlogResponse` object with separate arrays per type
(e.g. `banned_from_community[]`, `removed_posts[]`, `removed_comments[]`).

**v4 API note:** In Lemmy 1.0, v4 endpoints for modlog return combined data at
`GET /api/v4/modlog`. The v3 endpoint remains compatible. Use v3 for now to avoid breakage.

**Reference UI:** https://lemmy.world/modlog (public modlog example)

---

### Section 1.6 Forgejo

**Sources:**
- https://forgejo.org/docs/next/user/repo-permissions/
- https://codeberg.org/api/swagger (Forgejo OpenAPI spec — large file, key endpoints extracted)
- Web search results confirming collaborator endpoint patterns

#### Permission Model

Forgejo uses a **role-based** model with four levels per repository:

| Role           | Can do                                                                                       |
|----------------|----------------------------------------------------------------------------------------------|
| `read`         | View, clone, pull, create PRs                                                                |
| `write`        | Read + push, merge PRs, moderate/delete issues and comments                                  |
| `administrator`| Write + manage collaborators, configure branches, manage repo settings                       |
| `owner`        | Full control including repo deletion and transfer                                            |

For organizations, teams provide unit-based access control. Each team can set `No Access`,
`Read`, or `Write` on each unit: Code, Issues, Pull Requests, Releases, Wiki, Projects, Packages,
Actions. A full `Admin` flag covers the entire repo. Teams are mapped to repos via
`POST /orgs/{org}/teams/{id}/repos/{owner}/{repo}`.

#### Ownership Model

Every repo has a single owner (user or org). Org repos are owned by the org; individual users
own personal repos. Transfer via `POST /repos/{owner}/{repo}/transfer` (requires admin access).
Deletion via `DELETE /repos/{owner}/{repo}`.

#### Moderation Actions and REST Endpoints

Forgejo has no "kick" or "ban" in the chat sense. The closest operations are:

| Action                        | Method | Endpoint                                              | Notes                                         |
|-------------------------------|--------|-------------------------------------------------------|-----------------------------------------------|
| Add collaborator               | PUT    | `/repos/{owner}/{repo}/collaborators/{collaborator}`  | body: `{"permission": "write"}`               |
| Remove collaborator            | DELETE | `/repos/{owner}/{repo}/collaborators/{collaborator}`  | —                                             |
| Check collaborator             | GET    | `/repos/{owner}/{repo}/collaborators/{collaborator}`  | 204 = collaborator, 404 = not                 |
| Get collaborator permission    | GET    | `/repos/{owner}/{repo}/collaborators/{collaborator}/permission` | returns `AccessMode` |
| Block user from org            | PUT    | `/orgs/{org}/block/{username}`                        | Org-level block                               |
| Unblock user from org          | DELETE | `/orgs/{org}/block/{username}`                        | —                                             |
| List blocked users (org)       | GET    | `/orgs/{org}/list_blocked`                            | —                                             |
| Remove org member              | DELETE | `/orgs/{org}/members/{username}`                      | Requires org admin                            |
| Update team membership         | PUT    | `/orgs/{org}/teams/{id}/members/{username}`           | Add/change role in team                       |

**No "channel" concept:** Forgejo repos have no sub-channels. What Poly calls "channels"
in Forgejo are: issues forum channel, PRs forum channel, code channel. These are hardcoded
by the plugin mapping, not runtime-configurable via the API. `update_channel` and
`reorder_channels` are **N/A** for Forgejo.

**Delete issue/comment (moderation):** Forgejo exposes `DELETE /repos/{owner}/{repo}/issues/{index}/comments/{id}`
and `DELETE /repos/{owner}/{repo}/issues/{index}` for moderating content. These map to
`delete_message` for Poly's purposes.

**No moderation log:** Forgejo does not expose a public moderation log API endpoint. Admin
panel UI has audit logs but they are not accessible via REST. Note as out-of-scope.

**Reference UI:** https://forgejo.org/docs/next/user/repo-permissions/

---

### Section 1.7 GitHub

**Sources:**
- https://docs.github.com/en/rest/collaborators/collaborators?apiVersion=2022-11-28
- https://docs.github.com/en/rest/orgs/members?apiVersion=2022-11-28

#### Permission Model

GitHub uses **five named permission levels** for repository collaborators:

| Level      | Can do                                                             |
|------------|--------------------------------------------------------------------|
| `pull`     | Read-only: view, clone, fork, create PRs                           |
| `triage`   | Read + manage issues/PRs labels, milestones                        |
| `push`     | Write: push code, merge PRs                                        |
| `maintain` | Push + manage repository (not admin-level destructive actions)     |
| `admin`    | Full: manage collaborators, settings, secrets, deploy keys         |

Org-level roles: `owner` (org admin) and `member` (base). Teams overlay additional access.
The "calculated permission" is the highest grant across all sources (repo direct + teams + org).

#### Ownership Model

Each repo has a single owner (user or org). For org repos, org owners have full control.
Transfer via `POST /repos/{owner}/{repo}/transfer`. Deletion via `DELETE /repos/{owner}/{repo}`.

#### Moderation Actions and REST Endpoints

| Action                   | Method | Endpoint                                                          | Notes                                     |
|--------------------------|--------|-------------------------------------------------------------------|-------------------------------------------|
| Add/update collaborator  | PUT    | `/repos/{owner}/{repo}/collaborators/{username}`                  | body: `{"permission": "push"}`            |
| Remove collaborator      | DELETE | `/repos/{owner}/{repo}/collaborators/{username}`                  | Also cancels pending invites              |
| Get collaborator perm    | GET    | `/repos/{owner}/{repo}/collaborators/{username}/permission`       | Returns `permission`, `role_name`         |
| List collaborators       | GET    | `/repos/{owner}/{repo}/collaborators`                             | `?affiliation=direct&permission=admin`    |
| Remove org member        | DELETE | `/orgs/{org}/members/{username}`                                  | Requires org owner                        |
| Set org membership role  | PUT    | `/orgs/{org}/memberships/{username}`                              | body: `{"role": "admin"}`                 |
| Delete issue comment     | DELETE | `/repos/{owner}/{repo}/issues/comments/{comment_id}`              | Requires write/maintain/admin             |
| Delete issue             | DELETE | `/repos/{owner}/{repo}/issues/{issue_number}` (via PATCH)         | No hard-delete; use `PATCH` to close+lock |
| Lock issue (thread)      | PUT    | `/repos/{owner}/{repo}/issues/{issue_number}/lock`                | body: `{"lock_reason": "spam"}`           |

**No kick/ban concept:** Same as Forgejo — GitHub repos have no "kick member" or "ban user from
repo" API endpoint. Removing a collaborator is the closest equivalent. GitHub's block mechanism
(`PUT /user/blocks/{username}`) is account-level, not repo-level.

**No channel concept:** Same structure as Forgejo — issues, PRs, code are hardcoded channel
types in the plugin. `update_channel` and `reorder_channels` are **N/A** for GitHub.

**No moderation log:** GitHub does not expose a public moderation log API. Org audit log is
available only to org owners at `GET /orgs/{org}/audit-log` with the `audit_log:read` scope,
which Poly cannot reliably acquire. Note as out-of-scope.

**Reference UI:** https://docs.github.com/en/organizations/managing-user-access-to-your-organizations-repositories

---

### Section 1.8 poly-server

**Sources:** `clients/server-client/src/` (in-tree — we control the spec)

#### Proposed Permission Model

poly-server is Poly's own backend. Since we control the spec, we can design a clean role model.
Proposed role tiers (to be implemented in `servers/poly-server/`):

| Role            | Powers                                                                            |
|-----------------|-----------------------------------------------------------------------------------|
| `owner`         | Implicit: all permissions; can delete server, transfer ownership                  |
| `admin`         | All moderation actions; can manage roles/channels; cannot delete/transfer server  |
| `moderator`     | Can kick, ban (temporary), delete messages, set slow-mode                         |
| `member`        | Send messages, react, view channels                                               |

Roles are server-wide (no per-channel overrides in v1 — add as v2 enhancement). No bitfield;
stored as an enum in the DB: `owner | admin | moderator | member`.

#### Ownership Model

Single owner (user who created the server). Transfer via dedicated API endpoint. Deletion
requires ownership. The Ed25519 key-based auth means ownership is tied to the cryptographic
key, not a password.

#### Proposed Moderation Endpoints

Since we control the API, these endpoints should be added to poly-server:

| Action               | Method | Proposed endpoint                                                | Notes                              |
|----------------------|--------|------------------------------------------------------------------|------------------------------------|
| Get my permissions   | GET    | `/api/servers/{server_id}/members/@me/permissions`               | Returns role + computed perms      |
| Get member list      | GET    | `/api/servers/{server_id}/members`                               | Returns `[{user, role}]`           |
| Update member role   | PATCH  | `/api/servers/{server_id}/members/{member_id}/role`              | body: `{"role": "moderator"}`      |
| Kick member          | DELETE | `/api/servers/{server_id}/members/{member_id}`                   | —                                  |
| Ban member           | POST   | `/api/servers/{server_id}/bans`                                  | `{user_id, reason?, expires_at?}`  |
| Unban member         | DELETE | `/api/servers/{server_id}/bans/{user_id}`                        | —                                  |
| List bans            | GET    | `/api/servers/{server_id}/bans`                                  | —                                  |
| Delete message       | DELETE | `/api/channels/{channel_id}/messages/{message_id}`               | Requires mod+ or authorship        |
| Update channel       | PATCH  | `/api/channels/{channel_id}`                                     | `name`, `topic`, `slow_mode_secs`  |
| Reorder channels     | PATCH  | `/api/servers/{server_id}/channels/reorder`                      | `[{channel_id, position}]`         |
| Get mod log          | GET    | `/api/servers/{server_id}/modlog`                                | `limit`, `before`, `after`         |

---

## Section 2 Current State Matrix

| Backend      | Owner field exposed       | Roles type exposed       | `kick` impl | `ban` impl | `delete_message` impl | `update_channel` impl | UI for any of above |
|--------------|---------------------------|--------------------------|-------------|------------|-----------------------|-----------------------|---------------------|
| poly-server  | No (not yet in API)       | No (not yet in API)      | No          | No         | No                    | No                    | No                  |
| discord      | No (not surfaced to host) | No (not surfaced to host)| No          | No         | No                    | No                    | No (F-DC-1 was permission-denied display only) |
| matrix       | No                        | No                       | No          | No         | No                    | No                    | No                  |
| stoat        | No                        | No                       | No          | No         | No                    | No                    | No                  |
| teams        | No                        | No                       | No          | No         | No                    | No                    | No                  |
| lemmy        | No                        | No (community mods not surfaced) | No | No        | No                    | N/A (no sub-channels) | No                  |
| forgejo      | No                        | No                       | N/A         | N/A        | No (issues/comments exist but not wired) | N/A    | No                  |
| github       | No                        | No                       | N/A         | N/A        | No (issues/comments exist but not wired) | N/A    | No                  |

**Key findings from code audit:**
- `ClientBackend` trait (`clients/client/src/lib.rs`) has NO moderation methods (no `kick_member`, `ban_member`, `delete_message`, `update_channel`, `get_my_permissions`, etc.).
- `BackendCapabilities` (`clients/client/src/types.rs`) has NO moderation capability flags (`has_roles`, `has_kick`, `has_ban`, `has_channel_mgmt`, `has_moderation_log`).
- Server settings UI (`crates/core/src/ui/account/server/settings/mod.rs`) has 4 sections: Overview, Notifications, Profile, General. No Roles, Bans, or Mod Log tabs exist.
- The `PermissionDenied` variant already exists in `ClientError` — it was added for F-DC-1 (Discord's VIEW_CHANNEL restriction).
- `ForumTag.moderated` bool already exists in `types.rs` — the only permission-related type today.

---

## Section 3 Shared Abstraction Design

### Section 3.1 New `ClientBackend` Trait Methods

Add the following methods to `ClientBackend` in `clients/client/src/lib.rs`. All have
`NotSupported` default implementations so existing backends compile without change.

```rust
/// Return the calling user's effective permissions for a server (and optional channel).
///
/// `channel_id` is optional: pass `None` for server-wide perms, `Some(id)` for
/// channel-level overrides (e.g. Discord per-channel overwrites).
///
/// Returns a `MemberPermissions` struct with boolean flags. The default impl
/// returns `Err(NotSupported(...))`.
async fn get_my_permissions(
    &self,
    server_id: &str,
    channel_id: Option<&str>,
) -> ClientResult<MemberPermissions> {
    let _ = (server_id, channel_id);
    Err(ClientError::NotSupported("get_my_permissions".to_string()))
}

/// Return the permissions of a specific member.
///
/// Used by the host to display a member's role in the member list and
/// to gate Kick/Ban buttons (must be able to act on target's role level).
async fn get_member_permissions(
    &self,
    server_id: &str,
    member_id: &str,
) -> ClientResult<MemberPermissions> {
    let _ = (server_id, member_id);
    Err(ClientError::NotSupported("get_member_permissions".to_string()))
}

/// Update a member's role assignment.
///
/// For bitfield systems (Discord, Stoat) this replaces the member's role list.
/// For enum systems (poly-server) this sets the role.
/// For power-level systems (Matrix) this updates the user's power level.
async fn update_member_role(
    &self,
    server_id: &str,
    member_id: &str,
    role: MemberRole,
) -> ClientResult<()> {
    let _ = (server_id, member_id, role);
    Err(ClientError::NotSupported("update_member_role".to_string()))
}

/// Kick a member from a server. The member can re-join via invite.
async fn kick_member(
    &self,
    server_id: &str,
    member_id: &str,
    reason: Option<&str>,
) -> ClientResult<()> {
    let _ = (server_id, member_id, reason);
    Err(ClientError::NotSupported("kick_member".to_string()))
}

/// Ban a member from a server.
///
/// `expires_at` is an RFC3339 timestamp for temporary bans; `None` = permanent.
/// `delete_message_history_secs` deletes the member's messages sent within
/// the last N seconds (0 = delete none, up to backend maximum).
async fn ban_member(
    &self,
    server_id: &str,
    member_id: &str,
    reason: Option<&str>,
    expires_at: Option<&str>,
    delete_message_history_secs: u32,
) -> ClientResult<()> {
    let _ = (server_id, member_id, reason, expires_at, delete_message_history_secs);
    Err(ClientError::NotSupported("ban_member".to_string()))
}

/// Unban a previously banned member.
async fn unban_member(
    &self,
    server_id: &str,
    member_id: &str,
) -> ClientResult<()> {
    let _ = (server_id, member_id);
    Err(ClientError::NotSupported("unban_member".to_string()))
}

/// Return the current ban list for a server.
async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
    let _ = server_id;
    Err(ClientError::NotSupported("get_bans".to_string()))
}

/// Delete a message by channel and message ID.
///
/// Backends should check `is_own_message` vs `manage_messages` permission
/// server-side. The host gates the UI action on `my_perms.manage_messages ||
/// message.author_id == my_user_id`.
async fn delete_message(
    &self,
    channel_id: &str,
    message_id: &str,
) -> ClientResult<()> {
    let _ = (channel_id, message_id);
    Err(ClientError::NotSupported("delete_message".to_string()))
}

/// Update a channel's metadata.
///
/// All fields are optional. Backends may ignore fields they don't support.
async fn update_channel(
    &self,
    channel_id: &str,
    params: UpdateChannelParams,
) -> ClientResult<Channel> {
    let _ = (channel_id, params);
    Err(ClientError::NotSupported("update_channel".to_string()))
}

/// Reorder channels within a server.
///
/// `ordering` is a list of `(channel_id, new_position)` pairs. Backends that
/// do not support reordering (Teams, Forgejo, GitHub, Lemmy) return `NotSupported`.
async fn reorder_channels(
    &self,
    server_id: &str,
    ordering: Vec<(String, u32)>,
) -> ClientResult<()> {
    let _ = (server_id, ordering);
    Err(ClientError::NotSupported("reorder_channels".to_string()))
}

/// Return the moderation log for a server.
///
/// Results are sorted newest-first. `before` is an opaque cursor string from a
/// previous response. Backends without a moderation log return `NotSupported`.
async fn get_moderation_log(
    &self,
    server_id: &str,
    limit: u32,
    before: Option<&str>,
) -> ClientResult<Vec<ModerationLogEntry>> {
    let _ = (server_id, limit, before);
    Err(ClientError::NotSupported("get_moderation_log".to_string()))
}
```

### Section 3.2 New Types for `clients/client/src/types.rs`

```rust
/// The calling user's effective permissions in a server or channel.
///
/// Boolean flags — the host uses these to gate UI affordances without knowing
/// which backend-specific role system produced them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MemberPermissions {
    /// Can manage the server itself (rename, change settings, delete).
    pub manage_server: bool,
    /// Can manage channels (create, rename, delete, reorder).
    pub manage_channels: bool,
    /// Can manage roles (create, edit, assign).
    pub manage_roles: bool,
    /// Can kick members from the server.
    pub kick_members: bool,
    /// Can ban members from the server.
    pub ban_members: bool,
    /// Can delete or suppress messages by other users.
    pub manage_messages: bool,
    /// Can put members in timeout / mute.
    pub timeout_members: bool,
    /// The user's display role (highest role name, or "Owner", "Admin", "Member").
    pub display_role: String,
    /// Numeric power level for backends that use one (Matrix, custom). `None` for
    /// bitfield/enum backends that don't expose a numeric level.
    pub power_level: Option<i64>,
}

/// Backend-specific role assignment for `update_member_role`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberRole {
    /// Role represented by its backend-specific ID string (Discord role ID, poly-server role name).
    ById(String),
    /// Matrix power level integer.
    PowerLevel(i64),
}

/// A currently banned member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannedMember {
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub reason: Option<String>,
    /// RFC3339 timestamp when the ban expires; `None` = permanent.
    pub expires_at: Option<String>,
    /// RFC3339 timestamp when the ban was applied.
    pub banned_at: Option<String>,
}

/// Parameters for updating a channel.
///
/// All fields are optional. The backend ignores fields it doesn't support.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateChannelParams {
    pub name: Option<String>,
    pub topic: Option<String>,
    /// New position index for display ordering (0-based).
    pub position: Option<u32>,
    /// Slow-mode interval in seconds (0 = disabled).
    pub slow_mode_secs: Option<u32>,
    /// Whether the channel is NSFW / age-gated.
    pub nsfw: Option<bool>,
}

/// A single entry in the server's moderation log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationLogEntry {
    pub id: String,
    pub action: ModerationAction,
    pub moderator: User,
    pub target_user_id: Option<String>,
    pub target_display_name: Option<String>,
    pub channel_id: Option<String>,
    pub message_id: Option<String>,
    pub reason: Option<String>,
    /// RFC3339 timestamp.
    pub timestamp: String,
}

/// What moderation action was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationAction {
    MemberKicked,
    MemberBanned,
    MemberUnbanned,
    MemberTimedOut,
    MemberRoleUpdated,
    MessageDeleted,
    ChannelUpdated,
    Other(String),
}
```

### Section 3.3 New `BackendCapabilities` Fields

Add to `BackendCapabilities` in `clients/client/src/types.rs`:

```rust
pub struct BackendCapabilities {
    // ... existing fields unchanged ...

    /// Whether the backend exposes a role/permission system.
    /// Gates the Roles tab in server settings.
    pub has_roles: bool,

    /// Whether kick_member is supported.
    /// Gates the Kick button in member context-menu.
    pub has_kick: bool,

    /// Whether ban_member / get_bans / unban_member are supported.
    /// Gates the Bans tab and Ban button.
    pub has_ban: bool,

    /// Whether update_channel and (optionally) reorder_channels are supported.
    /// Gates the Edit Channel dialog and drag-handle in channel list.
    pub has_channel_mgmt: bool,

    /// Whether get_moderation_log is supported.
    /// Gates the Mod Log tab in server settings.
    pub has_moderation_log: bool,
}
```

Update all `BackendCapabilities` constants to include the new fields:

```rust
pub const READ_ONLY_FEED: Self = Self {
    // existing fields ...
    has_roles: false, has_kick: false, has_ban: false,
    has_channel_mgmt: false, has_moderation_log: false,
};
pub const MESSAGING_NO_SOCIAL: Self = Self {
    // existing fields ...
    has_roles: false, has_kick: false, has_ban: true,  // Lemmy has community bans
    has_channel_mgmt: false, has_moderation_log: true,
};
pub const FULL_SOCIAL_CHAT: Self = Self {
    // existing fields ...
    has_roles: true, has_kick: true, has_ban: true,
    has_channel_mgmt: true, has_moderation_log: false, // default false; backends override
};
```

Update `capabilities_for_slug` overrides:
- `"github" | "forgejo"` → `has_roles: false, has_kick: false, has_ban: false, has_channel_mgmt: false, has_moderation_log: false`
- `"lemmy"` → `has_roles: false, has_kick: false, has_ban: true, has_channel_mgmt: false, has_moderation_log: true`
- `"teams"` → `has_roles: false, has_kick: true, has_ban: false, has_channel_mgmt: true, has_moderation_log: false`
- `"matrix"` → `has_roles: true, has_kick: true, has_ban: true, has_channel_mgmt: true, has_moderation_log: false`
- `"discord"` → `has_roles: true, has_kick: true, has_ban: true, has_channel_mgmt: true, has_moderation_log: true`
- `"stoat"` → `has_roles: true, has_kick: true, has_ban: true, has_channel_mgmt: true, has_moderation_log: false`
- `"poly"` → `has_roles: true, has_kick: true, has_ban: true, has_channel_mgmt: true, has_moderation_log: true`

### Section 3.4 WIT Interface Extension

Add a new WIT interface `client-moderation` to `wit/messenger-plugin.wit` for WASM plugin
backends. Native backends use the Rust trait methods directly; WASM plugins use the WIT interface.

Key WIT types to add (mirrors the Rust types above):

```wit
interface client-moderation {
    use types.{client-error, user};

    record member-permissions {
        manage-server: bool,
        manage-channels: bool,
        manage-roles: bool,
        kick-members: bool,
        ban-members: bool,
        manage-messages: bool,
        timeout-members: bool,
        display-role: string,
        power-level: option<s64>,
    }

    record banned-member {
        user-id: string,
        display-name: string,
        avatar-url: option<string>,
        reason: option<string>,
        expires-at: option<string>,
        banned-at: option<string>,
    }

    record update-channel-params {
        name: option<string>,
        topic: option<string>,
        position: option<u32>,
        slow-mode-secs: option<u32>,
        nsfw: option<bool>,
    }

    // ... ModerationLogEntry, ModerationAction, MemberRole similarly ...

    get-my-permissions: func(server-id: string, channel-id: option<string>) -> result<member-permissions, client-error>;
    kick-member: func(server-id: string, member-id: string, reason: option<string>) -> result<_, client-error>;
    ban-member: func(server-id: string, member-id: string, reason: option<string>, expires-at: option<string>, delete-history-secs: u32) -> result<_, client-error>;
    unban-member: func(server-id: string, member-id: string) -> result<_, client-error>;
    get-bans: func(server-id: string) -> result<list<banned-member>, client-error>;
    delete-message: func(channel-id: string, message-id: string) -> result<_, client-error>;
    update-channel: func(channel-id: string, params: update-channel-params) -> result<_, client-error>;
    reorder-channels: func(server-id: string, ordering: list<tuple<string, u32>>) -> result<_, client-error>;
    get-moderation-log: func(server-id: string, limit: u32, before: option<string>) -> result<list<moderation-log-entry>, client-error>;
}
```

Add `export client-moderation;` to the `messenger-plugin` world.

### Section 3.5 Escape Hatches for Non-Fitting Backends

The following backends have special cases that do NOT fit the shared abstraction cleanly:

| Backend   | Escape hatch                                                                                                         |
|-----------|----------------------------------------------------------------------------------------------------------------------|
| Teams     | No ban concept — `ban_member` returns `NotSupported`. `has_ban = false`. Kick maps to `DELETE /teams/{id}/members/{id}`. |
| Forgejo   | No kick/ban at channel level — returns `NotSupported`. `update_channel` and `reorder_channels` return `NotSupported`. |
| GitHub    | Same as Forgejo. Collaborator removal maps loosely to `kick_member` for org repos but not personal repos.            |
| Lemmy     | No kick concept (community membership is implicit) — `kick_member` returns `NotSupported`. `has_kick = false`.       |
| Matrix    | No NSFW field in `update_channel` — ignore the `nsfw` param. No `slow_mode_secs` — return `NotSupported` for that specific sub-operation. |
| Stoat     | Native `timeout` field verified on `PATCH /servers/{id}/members/{id}` with ISO8601 expiration. Clear via `remove: ["Timeout"]`. Separate from `ban_member` (permanent only). No escape hatch needed. |

---

## Section 4 Per-Backend Implementation Plan

### Phase B-DS: Discord

> Files primarily touched: `clients/discord/src/lib.rs`, `clients/discord/src/api.rs`

- [ ] **B-DS-1** Plugin: implement `get_my_permissions(server_id, channel_id?)` via
  `GET /guilds/{guild.id}/members/@me` (get role IDs) + `GET /guilds/{guild.id}/roles` (get
  permission bitfields) + optional `GET /channels/{channel.id}` for overwrites. Return
  `MemberPermissions` with computed boolean flags. In `clients/discord/src/lib.rs::DiscordClient`.

- [ ] **B-DS-2** Plugin: implement `kick_member` via `DELETE /guilds/{guild.id}/members/{user.id}`.
  Requires `KICK_MEMBERS` permission. In `clients/discord/src/lib.rs::DiscordClient::kick_member`.

- [ ] **B-DS-3** Plugin: implement `ban_member` via `PUT /guilds/{guild.id}/bans/{user.id}`.
  Map `expires_at` → encode duration in reason (Discord bans are permanent; no native expiry API
  as of v10 — note this limitation explicitly, implement `expires_at` as `NotSupported` for now
  OR use a background task approach — see Section 7 for out-of-scope note).
  Map `delete_message_history_secs` → `delete_message_seconds`.
  In `clients/discord/src/lib.rs::DiscordClient::ban_member`.

- [ ] **B-DS-4** Plugin: implement `unban_member` via `DELETE /guilds/{guild.id}/bans/{user.id}`.

- [ ] **B-DS-5** Plugin: implement `get_bans` via `GET /guilds/{guild.id}/bans`
  (paginated; fetch all pages). Map to `Vec<BannedMember>`.

- [ ] **B-DS-6** Plugin: implement `delete_message` via `DELETE /channels/{channel.id}/messages/{message.id}`.
  In `clients/discord/src/lib.rs::DiscordClient::delete_message`.

- [ ] **B-DS-7** Plugin: implement `update_channel` via `PATCH /channels/{channel.id}`.
  Map `UpdateChannelParams.slow_mode_secs` → `rate_limit_per_user`,
  `nsfw` → `nsfw`, `name` → `name`, `topic` → `topic`, `position` → `position`.

- [ ] **B-DS-8** Plugin: implement `reorder_channels` via `PATCH /guilds/{guild.id}/channels`
  with `[{id, position, parent_id?}]`.

- [ ] **B-DS-9** Plugin: implement `get_moderation_log` via `GET /guilds/{guild.id}/audit-logs`
  with relevant `action_type` values (20=kick, 22=ban, 23=unban, 12=channel update, 72=msg delete).
  Map audit log entries to `ModerationLogEntry`.

- [ ] **B-DS-10** Update `backend_capabilities()` in `clients/discord/src/lib.rs`:
  `has_roles: true, has_kick: true, has_ban: true, has_channel_mgmt: true, has_moderation_log: true`.

- [ ] **B-DS-11** Plugin tests in `clients/discord/tests/`:
  - `test_get_my_permissions_admin` — returns all `true` flags for guild owner token
  - `test_kick_member` — asserts DELETE request sent to correct endpoint
  - `test_ban_member` — asserts PUT with correct body
  - `test_delete_message` — asserts DELETE to message endpoint
  - `test_update_channel` — asserts PATCH with name/topic/slow_mode
  - `test_get_moderation_log` — maps audit log entry to `ModerationLogEntry`

- [ ] **B-DS-12** Host UI: server-settings → **Roles** tab (gated on `has_roles`).
  File to create: `crates/core/src/ui/account/server/settings/roles.rs`.
  Displays role list with name + permission summary. Read-only in v1; role editing is Section 7 future work.

- [ ] **B-DS-13** Host UI: server-settings → **Bans** tab (gated on `has_ban`).
  File to create: `crates/core/src/ui/account/server/settings/bans.rs`.
  Displays banned members table with unban button. Each unban requires confirmation dialog.

- [ ] **B-DS-14** Host UI: server-settings → **Mod Log** tab (gated on `has_moderation_log`).
  File to create: `crates/core/src/ui/account/server/settings/modlog.rs`.
  Paginated list of moderation actions with actor, target, action, reason, timestamp.

- [ ] **B-DS-15** Host UI: channel context-menu → **Edit Channel** dialog
  (gated on `has_channel_mgmt && my_perms.manage_channels`).
  File to create: `crates/core/src/ui/dialogs/edit_channel.rs`.
  Fields: name, topic, slow-mode (slider 0-21600s), NSFW toggle.

- [ ] **B-DS-16** Host UI: channel list → **drag-handle** for reorder
  (gated on `my_perms.manage_channels`). Implement in
  `crates/core/src/ui/account/server/channel_list.rs`.

- [ ] **B-DS-17** Host UI: message context-menu → **Delete** item
  (gated on `message.author_id == my_user_id || my_perms.manage_messages`).
  Add to `crates/core/src/ui/context_menu/menus.rs::MessageContextMenu`.

- [ ] **B-DS-18** Host UI: member context-menu → **Kick / Ban / Timeout**
  (gated on `my_perms.kick_members`, `my_perms.ban_members`, `my_perms.timeout_members`).
  Add to `crates/core/src/ui/context_menu/menus.rs::UserRowContextMenu`.
  Kick/Ban open confirmation dialogs. Timeout opens a duration-picker.

- [ ] **B-DS-19** Manual test via poly-web (Discord Koala or Kangaroo accounts):
  - Right-click a message → Delete is visible; click → message removed.
  - Right-click a member → Kick visible if my_perms.kick_members; click → confirmation → member removed.
  - Server settings → Bans tab → lists bans; Unban a member.
  - Server settings → Mod Log → shows recent actions.
  - Channel context-menu → Edit Channel → rename; verify in Discord client.

---

### Phase B-MX: Matrix

> Files primarily touched: `clients/matrix/src/lib.rs`, `clients/matrix/src/api.rs`

- [ ] **B-MX-1** Plugin: implement `get_my_permissions(server_id, channel_id?)` by fetching
  `GET /_matrix/client/v3/rooms/{roomId}/state/m.room.power_levels` and returning a
  `MemberPermissions` built from the current user's power level vs the `ban`, `kick`, `redact`,
  `state_default` thresholds.

- [ ] **B-MX-2** Plugin: implement `kick_member` via `POST /_matrix/client/v3/rooms/{roomId}/kick`
  with `{"user_id": member_id, "reason": reason}`.

- [ ] **B-MX-3** Plugin: implement `ban_member` via `POST /_matrix/client/v3/rooms/{roomId}/ban`.
  Matrix bans are permanent — ignore `expires_at` (document in code; matrix has no native
  temporary ban; log a warning if `expires_at` is `Some`).

- [ ] **B-MX-4** Plugin: implement `unban_member` via `POST /_matrix/client/v3/rooms/{roomId}/unban`.

- [ ] **B-MX-5** Plugin: implement `get_bans` — query
  `GET /_matrix/client/v3/rooms/{roomId}/members?membership=ban` and map to `Vec<BannedMember>`.

- [ ] **B-MX-6** Plugin: implement `delete_message` via
  `PUT /_matrix/client/v3/rooms/{roomId}/redact/{eventId}/{txnId}` with `{"reason": reason}`.
  Generate `txnId` as a random UUID per spec.

- [ ] **B-MX-7** Plugin: implement `update_channel` for room name/topic:
  - `PUT /_matrix/client/v3/rooms/{roomId}/state/m.room.name` with `{"name": name}`
  - `PUT /_matrix/client/v3/rooms/{roomId}/state/m.room.topic` with `{"topic": topic}`
  - Ignore `slow_mode_secs` (Matrix has no equivalent — log `NotSupported` internally).
  - Ignore `nsfw` (not a Matrix concept).

- [ ] **B-MX-8** Plugin: `reorder_channels` → return `NotSupported`. Matrix rooms/spaces do
  not have a user-controllable position order at the spec level.

- [ ] **B-MX-9** Plugin: `get_moderation_log` → return `NotSupported`. Matrix has no server-side
  moderation log. Note this in `CLAUDE.md`-style inline comment.

- [ ] **B-MX-10** Update `backend_capabilities()` in `clients/matrix/src/lib.rs`:
  `has_roles: true, has_kick: true, has_ban: true, has_channel_mgmt: true, has_moderation_log: false`.

- [ ] **B-MX-11** Plugin tests in `clients/matrix/tests/`:
  - `test_get_my_permissions_moderator` — power level 50, assert kick/ban/redact true
  - `test_kick_member` — asserts POST to `/rooms/{id}/kick`
  - `test_ban_member` — asserts POST to `/rooms/{id}/ban`
  - `test_delete_message_redacts` — asserts PUT to `/rooms/{id}/redact/{eventId}/{txnId}`
  - `test_update_channel_name_and_topic` — asserts two PUT state events

- [ ] **B-MX-12** Host UI: Roles tab gated on `has_roles`. For Matrix, "roles" = power level
  configuration. Show a list of members with their current power level + a number input to
  change (requires `manage_roles = my_power_level >= state_default`).

- [ ] **B-MX-13** Host UI: Bans tab (same as B-DS-13 shared component).

- [ ] **B-MX-14** Host UI: Edit Channel dialog — name and topic fields only (no slow-mode, no NSFW).

- [ ] **B-MX-15** Host UI: message context-menu → Delete (redact). Wire in menus.rs.

- [ ] **B-MX-16** Host UI: member context-menu → Kick / Ban (no Timeout — return `NotSupported`).

- [ ] **B-MX-17** Manual test via poly-web (Matrix Owl/Axolotl accounts):
  - Redact a message → message shows as `[message redacted]`.
  - Kick a member → member leaves the room.
  - Ban a member → member cannot rejoin.
  - Server settings → Bans tab → shows banned members.
  - Edit Channel → rename → verify in Element.

---

### Phase B-ST: Stoat

> Files primarily touched: `clients/stoat/src/lib.rs`, `clients/stoat/src/api.rs`

**Pre-condition:** Verify kick/ban/timeout endpoint paths against
https://developers.stoat.chat/developers/api/reference.html/ before implementing.
The paths below follow Revolt's API and are most likely correct but were not directly
confirmed from documentation during research.

- [ ] **B-ST-1** Plugin: implement `get_my_permissions` by fetching the calling user's
  member object from `GET /servers/{server_id}/members/@me` and the server's roles from
  `GET /servers/{server_id}`. Compute MemberPermissions from the permission bitfield.

- [ ] **B-ST-2** Plugin: implement `kick_member` via `DELETE /servers/{server_id}/members/{member_id}`.
  Requires `KickMembers` permission (bit 6, value 64).

- [ ] **B-ST-3** Plugin: implement `ban_member` via `PUT /servers/{server_id}/bans/{user_id}`
  with body `{reason?: string, delete_message_seconds?: i64}`. Stoat bans are permanent — there
  is no `expires_at` field in the API. If `expires_at` is `Some`, return
  `ClientError::NotSupported("Stoat bans are permanent; use timeout_member for timed restrictions")`.
  Map `delete_message_history_secs` → `delete_message_seconds`.

- [ ] **B-ST-4** Plugin: implement `unban_member` via `DELETE /servers/{server_id}/bans/{user_id}`.

- [ ] **B-ST-5** Plugin: implement `get_bans` via `GET /servers/{server_id}/bans`.

- [ ] **B-ST-6** Plugin: implement `delete_message` via
  `DELETE /channels/{channel_id}/messages/{message_id}`.

- [ ] **B-ST-7** Plugin: implement `update_channel` via `PATCH /channels/{channel_id}`.
  Fields: `name`, `description` (→ `topic`), `nsfw` (→ `nsfw`), `archived` (→ `active`),
  `slow_mode_secs` → `slowmode` (Stoat's field name; uint64 seconds, max 21600).
  Verified from `DataEditChannel` schema. Remove any internal "no slow-mode" warning.

- [ ] **B-ST-8** Plugin: implement `reorder_channels` — verify if Stoat has a channel
  position/reorder endpoint. If not (Revolt did not have one), return `NotSupported` and set
  `has_channel_mgmt` to `true` but note reorder is partial.

- [ ] **B-ST-9** Update `backend_capabilities()`:
  `has_roles: true, has_kick: true, has_ban: true, has_channel_mgmt: true, has_moderation_log: false`.

- [ ] **B-ST-10** Plugin tests: kick, ban, delete_message, update_channel, get_my_permissions.

- [ ] **B-ST-11** Host UI: Roles, Bans, Edit Channel, Delete Message, Kick/Ban member context menu.
  Same shared components as Discord (B-DS-12 through B-DS-18).

- [ ] **B-ST-12** Manual test via poly-web (Stoat/Raccoon accounts).

---

### Phase B-TE: Teams

> Files primarily touched: `clients/teams/src/lib.rs`, `clients/teams/src/http.rs`

**Important limitations to surface to the user in the UI:**
- No ban concept (Teams API does not expose it).
- No channel reorder (Graph API does not expose position).
- Personal Microsoft accounts: ALL moderation features unsupported.
- Soft-delete only (messages show as deleted but content is preserved in compliance copy).

- [ ] **B-TE-1** Plugin: implement `get_my_permissions` by checking the caller's membership
  record via `GET /teams/{team-id}/members` and filtering for `me`. If `roles: ["owner"]`,
  return all permissions true. Otherwise member-level permissions (no kick/ban).
  **Note:** Teams API does not expose a per-channel permission concept.

- [ ] **B-TE-2** Plugin: implement `kick_member` via
  `DELETE /teams/{team-id}/members/{membership-id}`.
  The `membership-id` is the base64-encoded composite ID; the plugin must resolve it via
  `GET /teams/{team-id}/members` or cache it at login. Requires `TeamMember.ReadWrite.All`.

- [ ] **B-TE-3** Plugin: `ban_member` → return `ClientError::NotSupported("ban_member: Teams has no ban concept")`.

- [ ] **B-TE-4** Plugin: implement `delete_message` via
  `POST /teams/{teamId}/channels/{channelId}/messages/{msgId}/softDelete`.
  The existing Teams channel ID format (`"<team_id>/<channel_id>"`) means the plugin must
  split on `/` to get both IDs. Requires `ChannelMessage.ReadWrite` delegated scope.

- [ ] **B-TE-5** Plugin: implement `update_channel` via `PATCH /teams/{team-id}/channels/{channel-id}`.
  Supported fields: `displayName` (→ `name`), `description` (→ `topic`).
  No slow-mode, no nsfw, no position — ignore all and log a warning for unsupported fields.

- [ ] **B-TE-6** Plugin: `reorder_channels` → return `NotSupported`. Graph API has no
  channel position endpoint.

- [ ] **B-TE-7** Update `backend_capabilities()` in `clients/teams/src/lib.rs`:
  `has_roles: false, has_kick: true, has_ban: false, has_channel_mgmt: true, has_moderation_log: false`.

- [ ] **B-TE-8** Plugin tests: `test_kick_member_delegates_to_graph`,
  `test_ban_member_returns_not_supported`, `test_delete_message_soft_deletes`,
  `test_update_channel_patch`.

- [ ] **B-TE-9** Host UI: Edit Channel dialog (name + description only; gated on `has_channel_mgmt`).
  Host UI: Kick button in member context menu (gated on `has_kick`).
  Host UI: No Bans tab (has_ban=false).
  Host UI: Delete Message (same shared menus.rs addition).

- [ ] **B-TE-10** Manual test via poly-web (Teams Sheep/Walrus accounts):
  - Soft-delete a message → shows "This message was deleted".
  - Kick a member → member removed from team.
  - Edit Channel → rename → verify in Teams native app.

---

### Phase B-LE: Lemmy

> Files primarily touched: `clients/lemmy/src/lib.rs`, `clients/lemmy/src/api.rs`

- [ ] **B-LE-1** Plugin: implement `get_my_permissions` by checking if the calling user is
  in the community's `moderators` list (from `GET /api/v3/community?id={community_id}`)
  or if they are a site admin (from the `local_user_view.local_user.admin` field in the
  session). Map to `MemberPermissions{ban_members: is_mod, manage_messages: is_mod, ...}`.

- [ ] **B-LE-2** Plugin: `kick_member` → return `ClientError::NotSupported("kick_member: Lemmy has no kick concept; community membership is implicit")`.

- [ ] **B-LE-3** Plugin: implement `ban_member` via `POST /api/v3/community/ban_user`.
  Parameters: `community_id` (from server_id), `person_id` (from member_id), `ban: true`,
  `reason`, `expires` (Unix timestamp from `expires_at`), `remove_data: false`.

- [ ] **B-LE-4** Plugin: implement `unban_member` via `POST /api/v3/community/ban_user`
  with `ban: false`.

- [ ] **B-LE-5** Plugin: implement `get_bans` — Lemmy does not expose a `GET /community/bans`
  endpoint. The modlog shows ban events. Fetch from
  `GET /api/v3/modlog?community_id={id}&type_=ModBanFromCommunity` and map the
  `banned_from_community[]` array in the `GetModlogResponse` to `Vec<BannedMember>`.
  Note: `type_=ModBan` is site-wide admin bans; `type_=ModBanFromCommunity` is
  community-level bans. Use `ModBanFromCommunity`. Verified from real Lemmy v1.0 API.

- [ ] **B-LE-6** Plugin: implement `delete_message` by mapping to:
  - If `message_id` refers to a post: `POST /api/v3/post/remove` with `{post_id, removed: true, reason}`.
  - If `message_id` refers to a comment: `POST /api/v3/comment/remove` with `{comment_id, removed: true, reason}`.
  The plugin must detect which type a message ID refers to — use a prefix convention:
  encode Lemmy post IDs as `"post:{id}"` and comment IDs as `"comment:{id}"` in message IDs
  returned by `get_messages`. (Verify this encoding exists in `map_post_to_message` and
  `map_comment_to_message` in `clients/lemmy/src/api.rs`; add if missing.)

- [ ] **B-LE-7** Plugin: `update_channel` → return `NotSupported` (Lemmy has no sub-channels;
  "channel" = community and community update is admin-only and out-of-scope for v1).

- [ ] **B-LE-8** Plugin: `reorder_channels` → return `NotSupported`.

- [ ] **B-LE-9** Plugin: implement `get_moderation_log` via
  `GET /api/v3/modlog?community_id={id}&limit={limit}` (no `type_` filter — use `All`).
  Response is a `GetModlogResponse` object with separate arrays per action type:
  - `removed_posts[]` → `ModerationAction::MessageDeleted` (with post context)
  - `removed_comments[]` → `ModerationAction::MessageDeleted` (with comment context)
  - `banned_from_community[]` → `ModerationAction::MemberBanned`
  - `added_to_community[]` → `ModerationAction::MemberRoleUpdated`
  Merge all arrays, sort by timestamp descending. Verified response shape from real Lemmy API.

- [ ] **B-LE-10** Update `backend_capabilities()`:
  `has_roles: false, has_kick: false, has_ban: true, has_channel_mgmt: false, has_moderation_log: true`.

- [ ] **B-LE-11** Plugin tests: `test_ban_member`, `test_unban_member`, `test_delete_post_message`,
  `test_delete_comment_message`, `test_get_moderation_log`.

- [ ] **B-LE-12** Host UI: Bans tab (has_ban=true). Delete Message in message context menu.
  Mod Log tab (has_moderation_log=true). No Roles tab, no Kick button.

- [ ] **B-LE-13** Manual test via poly-web (Lemmy Beaver/Hedgehog accounts):
  - Ban a community member → they can no longer post.
  - Remove (soft-delete) a post.
  - View Mod Log tab.

---

### Phase B-FJ: Forgejo

> Files primarily touched: `clients/forgejo/src/lib.rs`, `clients/forgejo/src/api.rs`

- [ ] **B-FJ-1** Plugin: implement `get_my_permissions` by checking if the authenticated user
  is the repo owner or has admin collaborator access via
  `GET /repos/{owner}/{repo}/collaborators/{username}/permission`.
  Map to `MemberPermissions{manage_server: is_admin, manage_messages: is_write_or_admin, ...}`.

- [ ] **B-FJ-2** Plugin: `kick_member` → for personal repos, return `NotSupported`.
  For org repos, removing a collaborator maps to kick:
  `DELETE /repos/{owner}/{repo}/collaborators/{username}`.
  However, this only removes direct access; org team membership is unchanged.
  Document this limitation clearly in the code.

- [ ] **B-FJ-3** Plugin: `ban_member` → return `NotSupported`.
  Forgejo has org-level blocking (`PUT /orgs/{org}/block/{username}`) but no repo-level ban.
  Out-of-scope for v1 (see Section 7).

- [ ] **B-FJ-4** Plugin: implement `delete_message` — in Forgejo, "messages" are issue comments.
  Map to `DELETE /repos/{owner}/{repo}/issues/comments/{comment_id}`.
  The `message_id` must encode the comment ID; verify the encoding in
  `clients/forgejo/src/mapping.rs`.

- [ ] **B-FJ-5** Plugin: `update_channel` → return `NotSupported` (channels are hardcoded issue/PR/code types).

- [ ] **B-FJ-6** Plugin: `reorder_channels` → return `NotSupported`.

- [ ] **B-FJ-7** Plugin: `get_moderation_log` → return `NotSupported`. Forgejo has no public
  moderation log REST endpoint.

- [ ] **B-FJ-8** Update `backend_capabilities()`:
  `has_roles: false, has_kick: false, has_ban: false, has_channel_mgmt: false, has_moderation_log: false`.
  (For org repos: `has_kick: true` — but this requires knowing at runtime whether the
  current repo is org-owned. Proposed: check during `authenticate` and set a flag. Mark as TODO.)

- [ ] **B-FJ-9** Plugin tests: `test_delete_issue_comment`, `test_get_my_permissions_admin`.

- [ ] **B-FJ-10** Host UI: Delete Message item in message context-menu only
  (gated on `manage_messages || is_own_message`). No Roles, Bans, Mod Log, or Edit Channel tabs.

- [ ] **B-FJ-11** Manual test via poly-web (Forgejo Otter/Flamingo accounts).

---

### Phase B-GH: GitHub

> Files primarily touched: `clients/github/src/lib.rs`, `clients/github/src/api.rs`

- [ ] **B-GH-1** Plugin: implement `get_my_permissions` by calling
  `GET /repos/{owner}/{repo}/collaborators/{username}/permission` for the authenticated user.
  Map `admin` → `manage_server: true, manage_channels: false, manage_messages: true`.

- [ ] **B-GH-2** Plugin: `kick_member` → `DELETE /repos/{owner}/{repo}/collaborators/{username}`.
  Only meaningful for repos where user is a direct collaborator. For org repos this does not
  remove org team membership — document the limitation.

- [ ] **B-GH-3** Plugin: `ban_member` → return `NotSupported`. GitHub has no repo-level ban.

- [ ] **B-GH-4** Plugin: implement `delete_message` (issue comment deletion) via
  `DELETE /repos/{owner}/{repo}/issues/comments/{comment_id}`.
  Verify comment ID encoding in `clients/github/src/mapping.rs`.

- [ ] **B-GH-5** Plugin: `update_channel` → return `NotSupported`.

- [ ] **B-GH-6** Plugin: `reorder_channels` → return `NotSupported`.

- [ ] **B-GH-7** Plugin: `get_moderation_log` → return `NotSupported`.

- [ ] **B-GH-8** Update `backend_capabilities()`:
  `has_roles: false, has_kick: false, has_ban: false, has_channel_mgmt: false, has_moderation_log: false`.
  (Same note as Forgejo: `has_kick: true` for repos where the user is an admin collaborator.)

- [ ] **B-GH-9** Plugin tests: `test_delete_issue_comment_gh`, `test_get_my_permissions_admin_gh`.

- [ ] **B-GH-10** Host UI: Delete Message in message context-menu only.

- [ ] **B-GH-11** Manual test via poly-web (GitHub Penguin/Chameleon accounts).

---

### Phase B-PS: poly-server

> Files primarily touched: `clients/server-client/src/backend.rs`, `servers/poly-server/src/`

- [ ] **B-PS-1** Server: Add `role` column (`owner | admin | moderator | member`) to the
  `server_members` table in poly-server. Add migration. In `servers/poly-server/`.

- [ ] **B-PS-2** Server: Add REST endpoints (see Section 1.8 proposed endpoints) to poly-server's
  axum router: `GET/POST/DELETE /api/servers/{id}/bans`, `PATCH /api/servers/{id}/members/{id}/role`,
  `DELETE /api/servers/{id}/members/{id}` (kick), `DELETE/PATCH /api/channels/{id}`,
  `PATCH /api/servers/{id}/channels/reorder`, `GET /api/servers/{id}/modlog`.

- [ ] **B-PS-3** Server: Enforce permission checks on all new endpoints using the `role` field.
  Middleware: parse JWT → look up `server_members.role` → check against required role tier.

- [ ] **B-PS-4** Client: implement `get_my_permissions` via
  `GET /api/servers/{server_id}/members/@me/permissions` in `clients/server-client/src/backend.rs`.

- [ ] **B-PS-5** Client: implement `kick_member`, `ban_member`, `unban_member`, `get_bans`,
  `delete_message`, `update_channel`, `reorder_channels`, `get_moderation_log`.

- [ ] **B-PS-6** Update `backend_capabilities()`:
  `has_roles: true, has_kick: true, has_ban: true, has_channel_mgmt: true, has_moderation_log: true`.

- [ ] **B-PS-7** Server tests in `servers/poly-server/tests/`: test each new endpoint with
  correct role, insufficient role, and non-member scenarios. Use the existing
  `servers/test-poly-server/` test infrastructure.

- [ ] **B-PS-8** Client tests: test each new client method against the test server.

- [ ] **B-PS-9** Host UI: full Roles tab, Bans tab, Mod Log tab, Edit Channel dialog,
  drag-to-reorder, Kick/Ban/Delete buttons.

- [ ] **B-PS-10** Manual test via poly-web.

---

## Section 5 Host-Side Shared Work (Lands First)

This phase has no backend dependency and can be done in parallel with backend research.
It must land before any per-backend phase to avoid merge conflicts.

- [ ] **H-1** Add new fields to `BackendCapabilities` in `clients/client/src/types.rs`:
  `has_roles`, `has_kick`, `has_ban`, `has_channel_mgmt`, `has_moderation_log` (all `bool`).
  Update ALL `const` presets and the `capabilities_for_slug` match arm. Update pack-F
  capability-gate unit tests in the same file.

- [ ] **H-2** Add new types to `clients/client/src/types.rs`:
  `MemberPermissions`, `MemberRole`, `BannedMember`, `UpdateChannelParams`, `ModerationLogEntry`,
  `ModerationAction`. All derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.

- [ ] **H-3** Add new methods to `ClientBackend` trait in `clients/client/src/lib.rs`:
  `get_my_permissions`, `get_member_permissions`, `update_member_role`, `kick_member`,
  `ban_member`, `unban_member`, `get_bans`, `delete_message`, `update_channel`,
  `reorder_channels`, `get_moderation_log`. All have `NotSupported` default implementations.

- [ ] **H-4** Extend `wit/messenger-plugin.wit` with `client-moderation` interface (Section 3.4).
  Add `export client-moderation;` to the world. Re-generate WIT bindings for any WASM backends.

- [ ] **H-5** Create shared server-settings tab framework additions:
  - `crates/core/src/ui/account/server/settings/roles.rs` — `RolesTab` component.
    Props: `account_id`, `server_id`. Fetches role list; displays member-role rows.
    Gated in parent on `caps.has_roles`.
  - `crates/core/src/ui/account/server/settings/bans.rs` — `BansTab` component.
    Fetches `get_bans`, displays list, each row has Unban button (confirmation required).
    Gated on `caps.has_ban`.
  - `crates/core/src/ui/account/server/settings/modlog.rs` — `ModLogTab` component.
    Paginated. Gated on `caps.has_moderation_log`.

- [ ] **H-6** Add the new tabs to `ServerSettingsSection` enum in
  `crates/core/src/ui/account/server/settings/mod.rs`:
  `Roles`, `Bans`, `ModLog`. Render conditionally based on `BackendCapabilities`.

- [ ] **H-7** Create dialog components:
  - `crates/core/src/ui/dialogs/edit_channel.rs` — `EditChannelDialog`.
    Props: `account_id`, `channel_id`. Fields: name (text), topic (textarea), slow-mode (number
    input 0-21600), nsfw toggle. Gated on `caps.has_channel_mgmt && my_perms.manage_channels`.
  - `crates/core/src/ui/dialogs/kick_member.rs` — `KickMemberDialog`.
    Props: `account_id`, `server_id`, `member_id`, `member_display_name`. Confirmation modal.
  - `crates/core/src/ui/dialogs/ban_member.rs` — `BanMemberDialog`.
    Props: `account_id`, `server_id`, `member_id`. Fields: reason (text), duration (select:
    permanent / 1h / 1d / 7d / 30d / custom), delete-message-history (toggle).

- [ ] **H-8** Add drag-handle to channel list items in
  `crates/core/src/ui/account/server/channel_list.rs`.
  Gated on `my_perms.manage_channels`. Use HTML5 `draggable` + ondragstart/ondragover/ondrop;
  on drop call `reorder_channels`. Only render the handle when `caps.has_channel_mgmt`.

- [ ] **H-9** Add "Delete" item to `MessageContextMenu` in
  `crates/core/src/ui/context_menu/menus.rs`.
  Gated on: `my_user_id == message.author.id || my_perms.manage_messages`.
  Confirmation required (destructive variant). On confirm → `delete_message(channel_id, msg_id)`.

- [ ] **H-10** Add "Kick", "Ban", "Timeout" items to `UserRowContextMenu` in
  `crates/core/src/ui/context_menu/menus.rs`.
  - Kick: gated on `caps.has_kick && my_perms.kick_members`. Opens `KickMemberDialog`.
  - Ban: gated on `caps.has_ban && my_perms.ban_members`. Opens `BanMemberDialog`.
  - Timeout: gated on `my_perms.timeout_members`. Opens a duration-picker inline or as dialog.
  All three are `destructive` variant. Separated from primary actions by a divider.

- [ ] **H-11** Add "Edit Channel" to `ChannelContextMenu` in
  `crates/core/src/ui/context_menu/menus.rs`.
  Gated on `caps.has_channel_mgmt && my_perms.manage_channels`. Opens `EditChannelDialog`.

- [ ] **H-12** Add FTL keys for all new strings. Files to update:
  - `locales/en/main.ftl`
  - `locales/de/main.ftl`
  - `locales/es/main.ftl`
  - `locales/fr/main.ftl`

  New key groups (prefix: `perm-`):
  ```
  perm-roles-tab = Roles
  perm-bans-tab = Bans
  perm-modlog-tab = Mod Log
  perm-kick-action = Kick
  perm-kick-confirm = Are you sure you want to kick {$name}?
  perm-ban-action = Ban
  perm-ban-confirm = Ban {$name} from this server?
  perm-ban-reason-label = Reason (optional)
  perm-ban-duration-label = Duration
  perm-ban-duration-permanent = Permanent
  perm-ban-duration-1h = 1 hour
  perm-ban-duration-1d = 1 day
  perm-ban-duration-7d = 7 days
  perm-ban-duration-30d = 30 days
  perm-timeout-action = Timeout
  perm-unban-action = Unban
  perm-unban-confirm = Unban {$name}?
  perm-delete-message-action = Delete Message
  perm-delete-message-confirm = Delete this message? This action cannot be undone.
  perm-edit-channel-action = Edit Channel
  perm-edit-channel-title = Edit Channel
  perm-channel-name-label = Channel Name
  perm-channel-topic-label = Topic
  perm-slow-mode-label = Slow Mode (seconds)
  perm-nsfw-label = Age-Restricted (NSFW)
  perm-roles-empty = This server has no roles configured.
  perm-bans-empty = No members are banned from this server.
  perm-modlog-empty = No moderation actions recorded.
  perm-modlog-entry-kicked = {$mod} kicked {$target}
  perm-modlog-entry-banned = {$mod} banned {$target}
  perm-modlog-entry-unbanned = {$mod} unbanned {$target}
  perm-modlog-entry-msg-deleted = {$mod} deleted a message
  ```

- [ ] **H-13** Update `ServerSettingsSection` `const` array in `server/settings/mod.rs` to
  conditionally render new tabs, keyed on capabilities. Update `SERVER_SETTINGS_SECTIONS`
  to not be a fixed-size array (make it a `Vec` computed from capabilities at runtime).

- [ ] **H-14** Add `get_my_permissions` calls to the server-load flow in
  `crates/core/src/state.rs` (or wherever `ChatData` is populated). Cache permissions
  in `ChatData` as `Signal<HashMap<server_id, MemberPermissions>>`. Refresh on
  `ServerUpdated` events.

---

## Section 6 Test Plan

### Unit Tests Per Plugin

Each backend phase specifies test functions. The shared pattern:

1. Use the existing per-backend test server infrastructure (`servers/test-{backend}/`).
2. Test each new method against a running test server (or a mock HTTP layer for unit tests).
3. Test `NotSupported` is returned correctly for methods the backend doesn't implement.
4. All test files must include `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`
   per the project memory rule.

### Test Server Fixtures Needed

The following test servers need new fixtures or member accounts to enable moderation testing:

| Backend    | Fixture needed                                                                              | Test server location                  |
|------------|---------------------------------------------------------------------------------------------|---------------------------------------|
| discord    | A test guild with at least two members (one with KICK_MEMBERS perm, one target)             | `servers/test-discord/` (mock server) |
| matrix     | A test room with Alice at power level 100 and Bob at power level 0                          | `servers/test-matrix/`                |
| stoat      | A test server with owner account + regular member                                           | `servers/test-stoat/`                 |
| teams      | A test team with owner + member (needs real Microsoft test tenant or offline mock)           | `servers/test-teams/` (mock server)   |
| lemmy      | A test community with mod account + regular member + at least one banned member             | `servers/test-lemmy/`                 |
| forgejo    | A test repo with admin collaborator + regular collaborator + at least one issue comment     | `servers/test-forgejo/`               |
| github     | A test repo (use `gh` CLI mock or real GH test repo) with admin + collaborator              | Mock via `GhCli` test harness         |
| poly-server| Owner account + moderator account + member account + banned member fixture                  | `servers/test-poly-server/`           |

### Integration Tests

After Section 5 (host work) lands:

- `crates/core/tests/permissions_gate.rs` — verifies that UI components do not render
  moderation affordances when `BackendCapabilities::has_kick = false` etc. Test each
  capability flag independently.
- `crates/core/tests/delete_message_context_menu.rs` — verifies the Delete item appears
  when `my_perms.manage_messages = true` and does not appear when false.

### Haiku Subagent Test Harness

Run `TEST_HARNESS.md` via a haiku-tier subagent after each phase lands:
- Skip step 4 (unit tests) only if the change is UI-only CSS/RSX.
- Always run step 3 (WASM build) regardless.
- For backend phases, run `cargo test -p poly-{backend}` in addition to TEST_HARNESS.md.

---

## Section 7 Out of Scope (Deliberate Exclusions)

The following are explicitly NOT part of this plan. Each has a brief rationale.

| Feature                                              | Reason out of scope                                                                                     | Future work ticket suggestion                |
|------------------------------------------------------|---------------------------------------------------------------------------------------------------------|----------------------------------------------|
| Discord permission-overrides matrix editor           | Complex UI; requires per-channel allow/deny bitfield editing per role. High surface area for a v1.      | `plan-discord-channel-permissions-editor.md` |
| Discord temporary bans                               | Discord API (v10 as of 2026-04) has no native temporary-ban field. Third-party bots use guild scheduled events. Out of protocol scope. | When Discord adds the feature. |
| Matrix server ACLs (`m.room.server_acl`)             | Federated server banning — applies to entire homeservers, not individuals. Niche admin operation.       | Appendix to this plan when needed.           |
| Matrix Mjolnir/Draupnir integration                  | Bot-based moderation policy rooms. Out of scope for Poly's direct-API approach.                         | `plan-matrix-moderation-bots.md`             |
| Teams personal account moderation                    | Microsoft Graph does not support moderation actions for personal accounts (documented explicitly).       | Blocked by Microsoft. No future ticket needed. |
| Teams audit log via Microsoft 365 Compliance Center  | Requires separate admin product license and separate authentication flow. Completely out of scope.        | N/A                                          |
| Lemmy instance-wide admin actions                    | Site-admin actions (suspend user from entire instance, purge user) require admin role. Out of scope; Poly targets community moderators. | `plan-lemmy-admin-tools.md`                 |
| Forgejo org-level blocking (`/orgs/{org}/block/...`) | Requires the user to be an org admin; Poly's Forgejo plugin targets regular repo collaborators.         | `plan-forgejo-org-admin.md`                  |
| GitHub org audit log (`/orgs/{org}/audit-log`)       | Requires `audit_log:read` OAuth scope which is only grantable to org owners. Poly cannot reliably request this. | When GitHub relaxes the scope requirement.  |
| SSO / SAML integration for any backend               | Organizational authentication layer; completely out of protocol scope.                                  | `plan-enterprise-sso.md`                     |
| Cross-instance federated moderation (e.g. banning a user from all Matrix rooms in a Space) | Requires coordinating across multiple Matrix homeservers. Complex distributed systems problem. | Long-term Matrix federation plan.           |
| Advanced role editors (custom Discord roles, Stoat role colors) | Editing role properties beyond assignment. High complexity for marginal gain. | `plan-role-editor.md`                        |
| Stoat timeout endpoint                               | Endpoint not verifiable from available documentation during research. Implement after verification. | TODO comment in B-ST-3; re-assess when Stoat publishes stable API reference. |
| Lemmy `get_bans` from community ban list             | No dedicated `/community/bans` endpoint confirmed in v0.19 / v1.0. Relies on modlog filtering. | When Lemmy adds a dedicated ban list endpoint. |
| poly-server channel-level permission overrides       | Per-channel overrides (Discord-style) add significant DB and API complexity. v2 feature.               | Appendix to this plan post-v1 launch.        |

---

## Section 8 Rollout Order Recommendation

```
Section 5 Host-side shared work (H-1 through H-14)
    ↓
    ├─── Phase B-DS (Discord) — most complete API, best for de-risking shared abstractions
    │       ↓
    │    Verify shared components work end-to-end with Discord
    │
    ├─── Phase B-PS (poly-server) — we control the server; fastest iteration, cleanest design
    │
    └─── Phases B-MX, B-ST, B-TE, B-LE, B-FJ, B-GH  ← parallel-safe (disjoint files)
              ↓
         Integration tests (Section 6)
              ↓
         TEST_HARNESS.md via haiku subagent
```

**Dependency rules:**
- `Section 5 → all backend phases` (trait methods must exist before backend implements them)
- `B-DS → B-MX, B-ST` (Discord validates the shared component design; iteration before parallel rollout reduces rework)
- `B-FJ` and `B-GH` have the lowest blast radius and can be done at any time after Section 5
- `B-PS` has a server-side component (`servers/poly-server/`) and should be coordinated with
  whoever owns the poly-server service; the client-side is otherwise independent

**Parallelism notes for worktree subagents:**
- Each backend phase touches only its `clients/{name}/src/` files. File overlap is minimal.
- The three new `crates/core/src/ui/account/server/settings/` files (roles.rs, bans.rs, modlog.rs)
  are created in Section 5, so backend phases only call them as props consumers — no overlap.
- The `menus.rs` file IS shared across phases (Kick/Ban added once in Section 5 as gated items).
  Do NOT have multiple parallel agents edit `menus.rs` simultaneously.
- FTL files (`locales/*/main.ftl`) are shared — add all keys in Section 5, one commit. Backend
  phases should NOT touch FTL files directly.

---

*End of plan. Total: 8 backend research sections, 1 current state matrix, shared abstraction
design for trait + types + capabilities + WIT, 8 per-backend implementation phases
(~80 checkboxes), 1 host-side shared phase (~14 items), test plan with fixture requirements,
7 out-of-scope items with future work pointers, dependency-annotated rollout order.*
