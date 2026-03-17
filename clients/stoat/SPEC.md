# poly-stoat Specification

> **Last Updated:** 2026-03-17  
> **Sources:**
> - Local OpenAPI snapshot: `clients/stoat/api-1.json`
> - Stoat developer docs: `https://developers.stoat.chat/api-reference`
> - Reference JS client: `https://github.com/stoatchat/javascript-client-api`

---

## 1. Purpose

This document is the working protocol/feature spec for the Poly Stoat client.
It records:

- which Stoat/Revolt endpoints exist,
- how they map into `poly-client`,
- which features a Discord-like Stoat implementation should support,
- which slices are already implemented,
- and which E2E tests currently protect the implementation.

The most important architectural rule remains unchanged:

> **All Stoat-specific protocol logic must stay isolated inside `clients/stoat`.**  
> `poly-core` and app crates must only talk through `poly-client` / WIT/plugin boundaries.

---

## 2. Current Implemented Slice

### 2.1 Implemented today

- Base URL normalization and instance identity derivation
- Root config fetch: `GET /`
- Native auth transport:
  - `POST /auth/session/login`
  - `GET /users/@me`
  - `POST /auth/session/logout`
- Native server/channel retrieval:
  - `GET /servers/{id}`
  - `GET /channels/{id}`
  - `GET /sync/unreads`
- Native paginated message retrieval:
  - `GET /channels/{target}/messages`
  - supports `before`, `after`, `around`/`nearby`, and `limit`
  - supports both `BulkMessageResponse` shapes (`array<Message>` and expanded envelope)
- Native user/presence lookup:
  - `GET /users/{target}`
  - avatar URL resolution through Autumn when available
- Native social retrieval:
  - `GET /users/@me` relationship list → friend roster
  - `GET /users/dms` → DM and group discovery
  - `GET /channels/{target}/members` → group DM roster
- Native server member lookup:
  - `GET /servers/{target}/members`
  - server-member nickname/avatar overrides applied on top of user records
- Stored token resume using `X-Session-Token`
- Typed handling of Stoat login result variants:
  - `Success`
  - `MFA`
  - `Disabled`
- Current mock-backed native E2E coverage for:
  - login success
  - token resume
  - MFA error branch
  - disabled-account error branch
  - logout
  - root config fetch
  - server detail / categories / unread enrichment
  - channel list/detail retrieval
  - DM-channel rejection for server-only channel APIs
  - message retrieval with bundled users/members, replies, reactions, and attachment mapping
  - user lookup, avatar URL mapping, and presence lookup
  - server member roster lookup for server-backed channel member lists
  - friend list hydration from Stoat relationship metadata
  - DM list mapping with unread counts and last-message previews
  - group DM mapping with member rosters and last-message previews
  - group member lookup through the dedicated group-members endpoint

### 2.2 Not implemented yet

- Actual WASM guest parity for auth/data operations
- realtime websocket handling
- friend-request mutations / DM creation flows
- reactions/pins/search
- unread sync / ack integration
- voice/video / Vortex integration

---

## 3. Auth and Session Spec

### 3.1 Endpoints

| Endpoint | Purpose | Status |
|---|---|---|
| `GET /` | Fetch instance config (`RevoltConfig`) including `ws` | Implemented |
| `POST /auth/session/login` | Email/password login (`DataLogin`) | Implemented |
| `GET /users/@me` | Resolve current user profile | Implemented |
| `POST /auth/session/logout` | Invalidate current session | Implemented |
| `GET /auth/session/all` | Enumerate sessions | Planned |
| `DELETE /auth/session/{id}` | Revoke one session | Planned |
| `DELETE /auth/session/all` | Revoke all sessions | Planned |
| `PATCH /auth/session/{id}` | Edit friendly session name | Planned |

### 3.2 Login request

Primary supported branch from `DataLogin`:

```json
{
  "email": "alice@example.test",
  "password": "correct horse battery staple",
  "friendly_name": "Poly"
}
```

### 3.3 Login response variants

`ResponseLogin` is a tagged union on `result`:

- `Success`
  - fields: `_id`, `user_id`, `token`, `name`, `last_seen`, optional `origin`
- `MFA`
  - fields: `ticket`, `allowed_methods`
- `Disabled`
  - fields: `user_id`

### 3.4 Poly mapping rules

- Stoat session token header: `X-Session-Token`
- `GET /users/@me` after login constructs `poly_client::Session.user`
- `Session.instance_id` = normalized Stoat base URL with scheme removed and `/` replaced by `~`
- `Session.backend_url` = full Stoat base URL
- `Session.icon_emoji` = `🦦`

### 3.5 Deferred auth work

- MFA continuation (`mfa_ticket`, `mfa_response`)
- onboarding checks (`GET /onboard/hello`, `POST /onboard/complete`)
- policy acknowledgement (`POST /policy/acknowledge`)
- exact session ID recovery for token-restore flows via session inventory endpoints

---

## 4. Discord-like Stoat Feature Matrix

The following features were identified from the Stoat OpenAPI spec and the
reference JS client as relevant for a full Discord-like implementation.

### 4.1 Core shell / discovery

| Capability | Stoat API | Poly mapping | Priority |
|---|---|---|---|
| Instance config | `GET /` | connection/bootstrap | P1 |
| Self profile | `GET /users/@me` | current account | P1 |
| Fetch user | `GET /users/{target}` | user cards/profile popouts | P1 |
| Fetch user profile | `GET /users/{target}/profile` | extended profile panel | P2 |

### 4.1a Current user/presence mapping notes

Current Poly implementation for Stoat user identity and presence:

- `get_user(id)` uses `GET /users/{id}` plus `GET /` to resolve `features.autumn.url` for avatars
- Stoat `status.presence` currently maps as:
  - `Online` → `PresenceStatus::Online`
  - `Idle` → `PresenceStatus::Idle`
  - `Focus` / `Busy` → `PresenceStatus::DoNotDisturb`
  - `Invisible` → `PresenceStatus::Invisible`
- when `status.presence` is absent, Poly falls back to Stoat's `online: bool`
- bundled users inside message payloads now reuse the same avatar-aware user mapping

### 4.2 Servers and channels

| Capability | Stoat API | Poly mapping | Priority |
|---|---|---|---|
| Fetch server | `GET /servers/{target}` | `Server` details | P1 |
| Fetch members | `GET /servers/{target}/members` | server member list | P1 |
### 4.2a Current member-list mapping notes

Current Poly implementation for Stoat server/channel member lists:

- `get_channel_members(channel_id)` first resolves the channel via `GET /channels/{id}`
- for server channels, Poly then calls `GET /servers/{server_id}/members`
- returned Stoat `members` and `users` arrays are joined on user id
- member-level overrides currently win over user-level identity fields when present:
  - member `nickname` overrides user `display_name`
  - member `avatar` overrides user `avatar`

| Fetch channel | `GET /channels/{target}` | `Channel` details | P1 |
| Create server channel | `POST /servers/{server}/channels` | future mutation UI | P3 |
| Channel permissions | `/channels/{target}/permissions/*` | moderation/admin | P3 |
| Roles / ranks / server perms | `/servers/{target}/roles*`, `/servers/{target}/permissions*` | admin UI | P3 |

### 4.3 Messaging

| Capability | Stoat API | Poly mapping | Priority |
|---|---|---|---|
| Fetch messages | `GET /channels/{target}/messages` | history window | P1 |
| Send message | `POST /channels/{target}/messages` | composer send | P1 |
| Edit message | `PATCH /channels/{target}/messages/{msg}` | edit UI | P2 |
| Delete message | `DELETE /channels/{target}/messages/{msg}` | delete UI | P2 |
| Fetch single message | `GET /channels/{target}/messages/{msg}` | deep link / reply context | P2 |
| Search messages | `POST /channels/{target}/search` | channel search | P2 |
| Pin / unpin | `POST`/`DELETE /channels/{target}/messages/{msg}/pin` | pinned items | P2 |
| Bulk delete | `DELETE /channels/{target}/messages/bulk` | moderation tools | P3 |
| Ack message | `PUT /channels/{target}/ack/{message}` | read state | P1 |

### 4.3a Current message mapping notes

Current Poly implementation for Stoat message history:

- `MessageQuery::around` maps to Stoat `nearby`
- `MessageQuery::after` maps to Stoat `after` + `sort=Oldest`
- initial / `before` fetches map to `sort=Latest`, then Poly re-sorts the returned window chronologically for UI rendering
- Stoat message IDs are ULIDs, so Poly derives message timestamps from the ULID prefix when no explicit timestamp field is present in the REST payload
- reply previews are resolved when the replied-to message is included in the returned batch; no extra follow-up request is issued yet
- attachment URLs are currently built from `GET /` → `features.autumn.url` plus the Stoat file `tag` and `_id`

### 4.4 Social / DM / groups

| Capability | Stoat API | Poly mapping | Priority |
|---|---|---|---|
| Fetch DMs | `GET /users/dms` | DM list | P1 |
| Open DM | `GET /users/{target}/dm` | DM create/open | P1 |
| Mutuals | `GET /users/{target}/mutual` | profile mutual panel | P2 |
| Send friend request | `POST /users/friend` | friend add flow | P2 |
| Accept/remove friend | `PUT`/`DELETE /users/{target}/friend` | friend management | P2 |
| Block / unblock | `PUT`/`DELETE /users/{target}/block` | safety controls | P3 |
| Create group | `POST /channels/create` | group DM creation | P2 |
| Group members | `GET /channels/{target}/members` | group roster | P2 |
| Add/remove group member | `PUT`/`DELETE /channels/.../recipients/...` | group management | P2 |

### 4.4a Current social / DM / group mapping notes

Current Poly implementation for Stoat social surfaces:

- `get_friends()` uses `GET /users/@me` and reads the returned `relations` array.
  - only entries with status `Friend` are hydrated into Poly users
  - each friend user record is resolved through `GET /users/{id}` so avatars/presence use the same mapping as standalone user lookups
- `get_dm_channels()` uses `GET /users/dms` and keeps only Stoat `DirectMessage` channels.
  - Stoat `SavedMessages` is now surfaced as a Poly `DmChannel` using the authenticated user's own `User` record, because Poly's current `DmChannel` model requires a `user`
  - the other participant is chosen from the channel `recipients` array by excluding the authenticated user id
  - unread badges come from `GET /sync/unreads`
  - last-message previews are hydrated with a one-message `GET /channels/{target}/messages` fetch so bundled user metadata is preserved
- `open_direct_message_channel(user_id)` uses `GET /users/{target}/dm`.
  - targeting another user returns the one-to-one DM
  - targeting the authenticated user returns the Saved Messages channel
- `open_saved_messages_channel()` is a convenience wrapper around the self-targeted open-DM flow
- `get_groups()` uses `GET /users/dms` and keeps only Stoat `Group` channels.
  - members are resolved through `GET /channels/{target}/members`
  - last-message previews are hydrated from `GET /channels/{target}/messages`
- `get_channel_members(channel_id)` now supports both:
  - server channels via `GET /servers/{server}/members`
  - group DMs via `GET /channels/{target}/members`

Still pending in this area:

- friend-request mutations
- add/remove group member mutations
- broader UI polish for distinguishing self-DM / Saved Messages presentation if Poly later adds a dedicated saved-notes concept

### 4.5 Interaction polish

| Capability | Stoat API | Poly mapping | Priority |
|---|---|---|---|
| React / unreact | `/channels/{target}/messages/{msg}/reactions/*` | reactions bar | P2 |
| Clear reactions | `DELETE /channels/{target}/messages/{msg}/reactions` | moderation tools | P3 |
| Invites | `/channels/{target}/invites`, `/invites/{target}` | invite workflows | P2 |
| Server emoji | `GET /servers/{target}/emojis`, `/custom/emoji/*` | custom emoji picker | P2 |
| Sync unreads | `GET /sync/unreads` | unread badges | P1 |
| Settings sync | `/sync/settings/fetch`, `/sync/settings/set` | future per-account sync | P3 |

### 4.5a Current server/channel retrieval constraint

The published Stoat REST schema currently documents:

- `GET /servers/{target}`
- `GET /channels/{target}`
- `GET /sync/unreads`

but does **not** show an obvious authenticated joined-server collection
endpoint like `GET /servers` for the current account.

Current Poly implementation consequence:

- `get_server(id)` is implemented via `GET /servers/{id}`
- `get_channels(server_id)` is implemented by fetching the server for its
  channel IDs, then resolving each channel with `GET /channels/{id}`
- `get_channel(id)` is implemented for server channels
- `get_servers()` remains intentionally unsupported until joined-server
  discovery is sourced from Bonfire ready-state / sync cache or a dedicated
  REST list endpoint is confirmed

### 4.6 Voice / Vortex

| Capability | Stoat API | Poly mapping | Priority |
|---|---|---|---|
| Join call token | `POST /channels/{target}/join_call` | voice connect bootstrap | P1 |
| Stop ringing | `PUT /channels/{target}/end_ring/{target_user}` | DM/group call UX | P2 |

### 4.7 Lower-priority platform features

These exist in Stoat but are not core to the first Poly messenger experience:

- bots
- webhooks
- account deletion / reset / email change flows
- push subscription APIs
- moderation / reports

They should remain documented, but are not the first shipping priorities for
the Poly Stoat client.

---

## 5. E2E Coverage Matrix

### 5.1 Current automated coverage

#### Native transport integration tests (`clients/stoat/tests/integration.rs`)

- `stoat_fetch_server_config_round_trip`
- `stoat_authenticate_email_password_success`
- `stoat_authenticate_with_token_resume`
- `stoat_authenticate_mfa_response_returns_auth_failed`
- `stoat_authenticate_disabled_response_returns_auth_failed`
- `stoat_logout_clears_native_session`
- `stoat_get_server_maps_categories_and_unreads`
- `stoat_get_channels_fetches_server_channels_with_unreads`
- `stoat_get_channel_fetches_single_server_channel`
- `stoat_get_channel_rejects_dm_channels`
- `stoat_get_messages_maps_users_replies_attachments_and_reactions`
- `stoat_get_messages_accepts_plain_array_bulk_response`

These tests hit a real local HTTP server and validate the native Stoat client
transport end-to-end for the currently implemented slice.

#### WASM plugin host tests (`crates/plugin-host-tests/tests/client_e2e/stoat.rs`)

Current plugin coverage still targets the stub guest contract:

- backend identity
- unimplemented auth returns error
- empty list behavior
- not-found semantics
- valid empty event stream
- logout/set_presence do not trap

### 5.2 Required future live E2E stages

Once a disposable Stoat test account / self-hosted instance is available, add:

1. real login against Stoat
2. server list retrieval
3. channel list retrieval
4. message history retrieval
5. send/edit/delete message
6. reaction + pin workflow
7. DM open + DM history
8. friend request workflow
9. unread sync + ack behavior
10. websocket realtime message event test
11. voice join token test

---

## 6. Implementation Order

Recommended next slices after the current auth + server/channel + message slice:

1. joined-server discovery (Bonfire ready-state / sync cache)
2. message send
3. DMs / groups / friends
4. reactions / pins / search
5. unread sync / ack refinement
6. websocket realtime events
7. voice bootstrap (`join_call`)

This order preserves small, crash-safe increments while building toward a full
Stoat implementation.