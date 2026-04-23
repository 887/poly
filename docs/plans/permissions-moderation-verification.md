# Moderation API verification — Wave 0 of permissions plan

> Generated: 2026-04-23
> Plan reference: `docs/plans/plan-permissions-moderation.md` (Section 1.3 + 1.5 + Phase B-ST + B-LE)
> Test servers: Stoat port 9101, Lemmy port 9104 (both confirmed up via `/health`)

---

## Stoat (port 9101)

### How the test server was authenticated

`POST /test/auth/token {"username":"stoat"}` → token `45c89bf8-…` (header: `x-session-token`).
Seeded data: servers `SRV001` ("The Burrow", owner `STOAT01`), `SRV002` ("Midnight Dumpster",
owner `RACCOON01`); members `STOAT01`, `RACCOON01`, `LEMMING01`; channels `CH001`–`CH005`;
messages seeded in all channels.

### Endpoint table

All endpoints below were curled against the running test-stoat (port 9101).

| Plan endpoint | HTTP result | Root cause | Notes |
|---|---|---|---|
| `DELETE /servers/{id}/members/{id}` (kick) | **404** | Route not registered | Not in `lib.rs` router |
| `PUT /servers/{id}/bans/{user_id}` (ban) | **404** | Route not registered | Not in `lib.rs` router |
| `DELETE /servers/{id}/bans/{user_id}` (unban) | **404** | Route not registered | Not in `lib.rs` router |
| `GET /servers/{id}/bans` (list bans) | **404** | Route not registered | Not in `lib.rs` router |
| `DELETE /channels/{id}/messages/{msg_id}` (delete msg) | **405 Method Not Allowed** | Route exists (`GET /channels/{id}/messages/{msg_id}`) but only `GET`+`HEAD` are registered; no `DELETE` handler | Axum returns 405, not 404 — the route pattern matches but verb is wrong |
| `PATCH /channels/{id}` (update channel) | **405 Method Not Allowed** | `/channels/{id}` is registered as `GET` only | Same as above |
| `PATCH /servers/{id}/members/{id}` (member edit / timeout) | **404** | Route not registered | Separate path from member list |

**Summary:** Zero of the six moderation endpoints are implemented in test-stoat today. The
server/ban family returns 404 (route absent entirely); the channel endpoints return 405
(route exists for GET but lacks the moderation verbs). `PATCH /servers/{id}/members/{id}`
(used for both role assignment and timeout) is also absent.

### Correct endpoint paths confirmed from Revolt/Stoat official API spec

Source: `https://raw.githubusercontent.com/revoltchat/api/master/src/schema.ts` (the
TypeScript schema for the Stoat/Revolt API — Stoat inherited Revolt's API surface
verbatim). Also cross-referenced with `revolt.js` SDK and `publicapi.dev/revolt-api`.

The paths in the plan match the actual Revolt/Stoat API exactly:

| Action | Correct path | Correct verb |
|---|---|---|
| Kick member | `/servers/{server_id}/members/{member_id}` | `DELETE` |
| Ban member | `/servers/{server}/bans/{target}` | `PUT` |
| Unban member | `/servers/{server}/bans/{target}` | `DELETE` |
| List bans | `/servers/{target}/bans` | `GET` |
| Delete message | `/channels/{target}/messages/{msg}` | `DELETE` |
| Update channel | `/channels/{target}` | `PATCH` |

Minor note: the Revolt spec uses `{server}` and `{target}` as path-param names in some routes
vs the plan's `{server_id}` / `{user_id}`. These are parameter names only and do not affect
the path shape. The plan's variable names are clearer; keep them.

### Native timeout / mute concept — VERIFIED

**Finding: Stoat/Revolt has a native `timeout` field on `PATCH /servers/{server_id}/members/{member_id}`.**

From `DataMemberEdit` in `https://raw.githubusercontent.com/revoltchat/api/master/src/schema.ts`:

```typescript
DataMemberEdit: {
  nickname?: string;               // optional nickname override
  avatar?: string;                 // Autumn attachment ID
  roles?: string[];                // replace role list
  timeout?: string;                // ISO8601 datetime — timeout expiration
  can_publish?: boolean;           // voice publishing
  can_receive?: boolean;           // voice receiving
  voice_channel?: string;          // move member between voice channels
  remove?: FieldsMember[];         // clear specific fields
}
```

The `timeout` field accepts an ISO 8601 datetime string (the time at which the timeout
expires). Setting it to a future timestamp mutes the member; clearing it (via `remove:
["Timeout"]` in `FieldsMember`) ends the timeout early. The permission flag is
`TimeoutMembers` (bit 8, value 256), matching the plan's permission table.

Additionally, `DataEditChannel` exposes a `slowmode` field:

```typescript
DataEditChannel: {
  name?: string;
  description?: string;
  owner?: string;
  icon?: string;
  nsfw?: boolean;
  archived?: boolean;
  voice?: VoiceInformation;
  slowmode?: number;       // uint64, delay in seconds, max 6 hours (21600)
  remove?: FieldsChannel[];
}
```

This means Stoat **does** support slow-mode via `PATCH /channels/{id}` with
`slowmode` (not `slow_mode_secs` or `rate_limit_per_user` — Stoat uses its own field name).

`DataBanCreate` for `PUT /servers/{server}/bans/{target}`:

```typescript
DataBanCreate: {
  reason?: string;
  delete_message_seconds?: number;  // int64, bulk-delete member's recent messages
}
```

Stoat bans are **permanent** — there is no `expires_at` / `expires` field in `DataBanCreate`.
Temporary restriction is handled entirely via the `timeout` field on member edit, not via bans.

### Recommendation

All Stoat endpoint paths in the plan are confirmed correct. Apply these corrections:

1. **Section 1.3 timeout note:** Replace "Likely `PATCH /servers/{server_id}/members/{member_id}`
   with a `timeout` field — TODO verify" with confirmed: the field is `timeout: ISO8601`, cleared
   via `remove: ["Timeout"]`.

2. **Section 1.3 slow-mode note:** Replace "Not confirmed" with confirmed: field name is
   `slowmode` (uint64 seconds, max 21600) in `DataEditChannel`.

3. **Section 1.3 ban note:** Add `delete_message_seconds` to the ban body shape.

4. **Phase B-ST-3:** Remove "encode intent in reason string" fallback for `expires_at` — the
   correct behaviour is to use `timeout` for all timed restrictions, leaving `ban_member` as
   permanent-only. Update the escape-hatch table in Section 3.5.

5. **Phase B-ST-7:** The `slowmode` field name on `PATCH /channels/{id}` must be used (not
   `slow_mode_secs` or `rate_limit_per_user`).

6. **Test-stoat:** The following routes need to be added to `servers/test-stoat/src/lib.rs`
   before Wave 3 / B-ST tests can run (these are Wave-3 pre-conditions, not Wave-0 scope):
   - `DELETE /servers/{id}/members/{id}` → kick handler
   - `PUT /servers/{id}/bans/{target}` → ban handler
   - `DELETE /servers/{id}/bans/{target}` → unban handler
   - `GET /servers/{id}/bans` → list bans handler
   - `DELETE /channels/{id}/messages/{msg}` → delete message handler
   - `PATCH /channels/{id}` → update channel handler (add verb to existing route)
   - `PATCH /servers/{id}/members/{id}` → member edit handler (timeout + roles)

---

## Lemmy (port 9104)

### How the test server was authenticated

`POST /test/auth/token {"username":"beaver"}` → token `e58f7b43-…` (header: `Authorization: Bearer`).
Seeded communities: id=1 ("general"), id=2 ("programming").

### Modlog filter endpoint table

| Plan endpoint | HTTP result | Root cause | Notes |
|---|---|---|---|
| `GET /api/v3/modlog?community_id=1` | **404** | Route not registered | Not in `lib.rs` router |
| `GET /api/v3/modlog?community_id=2&type_=ModBan` | **404** | Route not registered | Not in `lib.rs` router |
| `GET /api/v3/modlog` (no params) | **404** | Route not registered | Not in `lib.rs` router |
| `POST /api/v3/community/ban_user` | **404** | Route not registered | Not in `lib.rs` router |
| `POST /api/v3/post/remove` | **404** | Route not registered | Not in `lib.rs` router |

**Summary:** Neither `/api/v3/modlog` nor the community ban / content-removal endpoints are
implemented in test-lemmy. The router in `servers/test-lemmy/src/lib.rs` only registers:
auth, communities, posts, private messages, users, site info, comments, and the test bypass.

### Real Lemmy v1.0 modlog API — verified via external source

Source: `https://rdrr.io/cran/remmy/man/lemmy_get_modlog.html` (R-language Lemmy API client
documentation, which tracks the Lemmy v3 API closely).

`GET /api/v3/modlog` parameters confirmed:

| Parameter | Type | Notes |
|---|---|---|
| `community_id` | numeric (optional) | Filters to a specific community |
| `mod_person_id` | numeric (optional) | Filters by moderator who took the action |
| `other_person_id` | numeric (optional) | Filters by the person acted upon |
| `type_` | string (optional) | Filter by action type |
| `page` | numeric (optional) | Pagination |
| `limit` | numeric (optional) | Results per page |
| `auth` | JWT string | Authentication |

`type_` accepted values:
`"All"`, `"ModRemovePost"`, `"ModLockPost"`, `"ModFeaturePost"`, `"ModRemoveComment"`,
`"ModRemoveCommunity"`, **`"ModBanFromCommunity"`**, `"ModAddCommunity"`,
`"ModTransferCommunity"`, `"ModAdd"`, **`"ModBan"`**, `"ModHideCommunity"`,
`"AdminPurgePerson"`, `"AdminPurgeCommunity"`, `"AdminPurgePost"`, `"AdminPurgeComment"`.

**Correction to plan:** The plan (B-LE-5) uses `type_=ModBan`. However `ModBan` is the
**site-wide** ban; community-level bans are `ModBanFromCommunity`. For the community ban list
(which is what the Bans tab needs), the correct filter is `type_=ModBanFromCommunity`.
`ModBan` returns site-wide admin bans which are out of scope for community mod UX.

The response structure (confirmed from the OpenAPI spec at `https://mv-gh.github.io/lemmy_openapi_spec/`)
returns a `GetModlogResponse` with separate arrays per action type:
- `banned_from_community: ModBanFromCommunityView[]` — for community ban events
- `banned: ModBanView[]` — for site-wide bans
- `removed_posts: ModRemovePostView[]`
- `removed_comments: ModRemoveCommentView[]`
- etc.

When `type_=ModBanFromCommunity`, only `banned_from_community` is populated; other arrays
are empty.

### Real Lemmy `POST /api/v3/community/ban_user` — confirmed

Confirmed via the OpenAPI spec. Request body (matches plan Section 1.5):
```json
{
  "community_id": 2,
  "person_id": 42,
  "ban": true,
  "expires": 1750000000,   // Unix timestamp (i64), null for permanent
  "reason": "string",
  "remove_data": false
}
```
Response: `BanFromCommunityResponse { banned_person: PersonView, banned: bool }`.

To **unban**, send the same endpoint with `ban: false` — plan B-LE-4 is correct.

### Recommendation

1. **Section 1.5 / B-LE-5:** Change `type_=ModBan` to `type_=ModBanFromCommunity` for the
   community-level Bans tab. Keep `type_=ModBan` only in `get_moderation_log` with `type_=All`
   as the default.

2. **B-LE-9:** When mapping modlog response, the `banned_from_community` array field holds
   community ban events. Update B-LE-9 to call out the correct response field name.

3. **Test-lemmy pre-conditions:** The following routes need to be added before Wave 3 / B-LE
   tests can run:
   - `POST /api/v3/community/ban_user`
   - `POST /api/v3/post/remove`
   - `POST /api/v3/comment/remove`
   - `GET /api/v3/modlog`

---

## Action items for `docs/plans/plan-permissions-moderation.md`

### Section 1.3 patches (Stoat)

1. Replace the timeout TODO note with:
   > **Verified:** Endpoint is `PATCH /servers/{server_id}/members/{member_id}` with body
   > `{"timeout": "<ISO8601 expiration>"}`. Clear via `{"remove": ["Timeout"]}`.
   > Permission flag: `TimeoutMembers` (bit 8, value 256). This is native — no workaround needed.

2. Replace the slow-mode TODO note with:
   > **Verified:** `PATCH /channels/{channel_id}` with `{"slowmode": <seconds>}` (uint64, max 21600).
   > Field name is `slowmode` (not `slow_mode_secs`). Confirmed from `DataEditChannel` schema.

3. Add `delete_message_seconds` to the ban body shape in the endpoints table.

### Section 1.5 patches (Lemmy)

4. Update the `get_bans` note in B-LE-5 to use `type_=ModBanFromCommunity` (not `ModBan`).
   Also add: response field is `banned_from_community[]` within the `GetModlogResponse` object.

### Phase B-ST patches

5. B-ST-3: Remove the fallback "encode expires in reason string" clause. Stoat has no native
   ban expiry; timed restrictions use `timeout` via a separate `timeout_member` call, not ban.
   Permanent bans stay permanent.

6. B-ST-7: Change `slow_mode_secs` → `slowmode` in the PATCH body. Add that the field is
   supported natively (remove the "TODO: verify" and "no slow-mode in Stoat API" comment).

7. Section 3.5 escape hatch for Stoat: Remove the FIXME note about `timeout` being unverified;
   replace with confirmed `PATCH /servers/{id}/members/{id}` with `timeout` field.

### Phase B-LE patches

8. B-LE-5: Change `type_=ModBan` → `type_=ModBanFromCommunity`.

9. B-LE-9: Add response mapping note: `GetModlogResponse.banned_from_community[]` for ban
   events, `GetModlogResponse.removed_posts[]` / `removed_comments[]` for remove events.

---

## Test-server stub work required (pre-condition for Wave 3)

Neither test server implements moderation endpoints. This is expected — these stubs don't
exist because Wave 1 (shared scaffolding) hasn't shipped yet. The Wave-3 B-ST and B-LE agents
must add the stubs to their respective test servers as part of their commits.

| Server | Routes to add |
|---|---|
| `servers/test-stoat/` | kick, ban, unban, list-bans, delete-message, update-channel, member-edit |
| `servers/test-lemmy/` | community/ban_user, post/remove, comment/remove, modlog |
