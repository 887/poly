# Poly Server Protocol

> Reference document for the poly-server HTTP + WebSocket protocol.  
> This document is the authoritative specification for client implementors.  
> See `phase-2.2-plan.md` for the implementation checklist.  
> **Last Updated:** 2026-03-15

---

## 1. Overview

Poly Server is a self-hosted chat backend exposing:
- **REST API** over HTTP/1.1 or HTTP/2 for all data operations.
- **WebSocket** at `/ws` for real-time server-push events.
- **Attachment serving** at `/attachments/:id` with access-control enforcement.

All JSON payloads use `Content-Type: application/json`. All endpoints (except those
marked `No Auth`) require an `Authorization: Bearer <token>` header.

---

## 2. Authentication Flow

```
Client                                Server
  │                                     │
  │  POST /auth/accounts                │
  │  { public_key }                     │
  │ ─────────────────────────────────► │
  │ ◄──────────────────────────────── │ 200 { accounts: [{ user_id, username, display_name, avatar_url }] }
  │                                     │
  │  POST /auth/signup                  │
  │  { public_key, email, username, display_name, device_name? }
  │ ─────────────────────────────────► │
  │ ◄──────────────────────────────── │ 201 { token, device_id, user_id }
  │                                     │
  │  POST /auth/challenge               │
  │  { public_key, user_id? }           │
  │ ─────────────────────────────────► │
  │ ◄──────────────────────────────── │ 200 { challenge, expires_at }
  │                                     │
  │  POST /auth/verify                  │
  │  { public_key, user_id?, challenge, signature, device_name? }
  │ ─────────────────────────────────► │
  │ ◄──────────────────────────────── │ 200 { token, device_id, user_id }
  │                                     │
  │  All subsequent requests:           │
  │  Authorization: Bearer <token>      │
  │ ─────────────────────────────────► │
```

### Signup semantics

- **Email is required** for Poly Server account creation.
- **Ed25519 identity key is also required** and becomes the cryptographic identity for that account.
- **One identity key may own multiple Poly Server accounts on the same server.** Clients should call `POST /auth/accounts` first, let the user choose an existing account if any are returned, and still offer creating another account.
- If multiple accounts exist for one key, `POST /auth/challenge` / `POST /auth/verify` require `user_id` so the server knows which account to sign in.
- Public profile endpoints do **not** expose the email address. Only `GET /users/me` returns the caller's own email.
- Poly clients may create the local identity during signup if one does not already exist.

**Token structure** (JWT, HS256):
```json
{
  "sub": "user:ulid",
  "device_id": "device:ulid",
  "exp": 1234567890,
  "iat": 1234567890
}
```

**Token lifetime**: Defaults to 30 days — configurable via `POLY_SERVER_JWT_EXPIRY_SECS`.

**Device revocation**: Each session is tracked as a `device` record. The server checks
`device.revoked` on every authenticated request. Revoked tokens return `401` even if the
JWT signature is still valid.

---

## 3. Error Responses

All errors return JSON:
```json
{ "error": "human-readable description" }
```

| HTTP Status | `AppError` | When |
|---|---|---|
| 400 | `BadRequest` | Malformed input (empty content, invalid status, etc.) |
| 401 | `Unauthorized` | Missing / invalid / revoked token |
| 403 | `Forbidden` | Valid token but insufficient permissions |
| 404 | `NotFound` | Resource does not exist (or you cannot see it) |
| 409 | `Conflict` | Duplicate (username already taken, etc.) |
| 500 | `Internal` | Server-side error (logged server-side) |

---

## 4. Server Info (No Auth)

```
GET /server-info
```
Response:
```json
{
  "name": "My Poly Server",
  "version": "0.1.0",
  "invite_only": false
}
```

---

## 5. Server (Guild) Management

### Access Model
- Creating a server automatically makes the creator its **owner** and first member.
- Server owners can: update/delete the server, manage channels/categories, kick members.
- Server members can: read channels, send messages, add reactions, upload files.

### Invite Codes
```
POST /servers/:id/invites
Body: { "max_uses": 10, "expires_in_secs": 86400 }   // both optional
Response: { "code": "abc123", "server_id": "...", ... }

POST /invites/:code/use
→ Adds caller to server membership. Returns updated server record.
```

---

## 6. Channel Access Model

| Channel Type | Readable By | Writable By |
|---|---|---|
| Server text channel | Server members | Server members |
| Server voice channel | Server members | Server members (signalling) |
| DM | Both participants | Both participants |
| Group DM | All participants | All participants |

Membership/participation is checked server-side on every message read/write.

---

## 7. Message Protocol

### Sending a message
```
POST /channels/:id/messages
Body: {
  "content": "Hello!",           // required unless attachments present
  "reply_to": "message:ulid",   // optional
  "attachments": ["attachment:ulid"]  // optional, IDs from POST /attachments
}
```

### Pagination
```
GET /channels/:id/messages?limit=50&before=message:ulid
```
Returns messages **newest-first** (descending by record ID, which encodes creation time).
Use `before` cursor for pagination — pass the ID of the oldest message from the previous
page to get the next page.

### Soft Delete
- `DELETE /messages/:id` sets `deleted = true` and replaces content with `[deleted]`.
- Any references to the message (replies) are unaffected — they show `reply_to` pointing
  at the `[deleted]` record, not a broken reference.
- Hard delete is never performed via the API.

---

## 8. File Attachment Protocol

```
      Client                          Server
        │                               │
        │  POST /attachments            │
        │  Content-Type: multipart/...  │
        │  [file bytes]                 │
        │ ──────────────────────────► │
        │ ◄────────────────────────── │ 201 { id, filename, mime_type, size_bytes }
        │                               │
        │  POST /channels/:id/messages  │
        │  { content, attachments: [id] }
        │ ──────────────────────────► │
        │ ◄────────────────────────── │ 201 MessageResponse (attachments embedded)
        │                               │
        │  GET /attachments/:id         │
        │  Authorization: Bearer ...    │
        │ ──────────────────────────► │ (checks channel read access)
        │ ◄────────────────────────── │ 200 [file bytes] + Content-Type
```

**Access control**:
- Orphan attachments (not yet linked to a message) → only the uploader.
- Linked attachments → anyone who can read the channel the message belongs to.

**Limits**:
- Max upload size: 50 MiB
- Supported: any MIME type (client is responsible for rendering)

**Storage**: Files stored on disk as `{UUID}.{ext}` — path cannot be guessed externally.

---

## 9. WebSocket Protocol

### Connection
```
ws://host/ws?token=<JWT>
```
Authentication is via query param (Bearer header not supported on WS upgrade).

### Event format
```json
{ "event": "<event_name>", "data": { ... } }
```

### Server → Client events

| `event` | `data` fields | When |
|---|---|---|
| `ping` | `{ ts }` | On connect + periodically |
| `message_created` | `MessagePayload` | New message in any readable channel |
| `message_edited` | `MessagePayload` | Message edited |
| `message_deleted` | `{ channel_id, message_id }` | Message soft-deleted |
| `reaction_added` | `{ message_id, user_id, emoji }` | Reaction added |
| `reaction_removed` | `{ message_id, user_id, emoji }` | Reaction removed |
| `typing_start` | `{ channel_id, user_id }` | User started typing |
| `presence_update` | `{ user_id, status }` | User came online/offline |
| `device_revoked` | `{ device_id }` | Server revoked this device's session |
| `voice_state_update` | `{ channel_id, user_id, joined }` | User joined/left voice |
| `friend_request_received` | `{ request_id, from_user_id }` | Incoming friend request |
| `friend_request_responded` | `{ request_id, status }` | Response to outgoing request |
| `server_member_added` | `{ server_id, user_id }` | Someone joined a server |
| `server_member_removed` | `{ server_id, user_id }` | Someone left or was kicked |
| `server_updated` | `Server` | Server name/icon changed |
| `channel_created` | `Channel` | New channel in a server |
| `channel_deleted` | `{ channel_id }` | Channel removed |

### `MessagePayload` structure
```json
{
  "id": "message:ulid",
  "channel_id": "channel:ulid",
  "author_id": "user:ulid",
  "content": "Hello!",
  "reply_to_id": null,
  "edited_at": null,
  "deleted": false,
  "attachments": ["attachment:ulid"],
  "created_at": "2026-03-01T00:00:00Z"
}
```

### Client → Server messages
```json
{ "type": "typing_start", "channel_id": "channel:ulid" }
{ "type": "heartbeat" }
{ "type": "voice_join", "channel_id": "channel:ulid" }
{ "type": "voice_leave", "channel_id": "channel:ulid" }
{ "type": "voice_signal", "target_user_id": "user:ulid", "sdp": "..." }
```

---

## 10. Voice / Screen Share Protocol

Voice and screen sharing use **WebRTC peer-to-peer** with the Poly Server acting as a
**signalling relay** only. No media passes through the server.

```
User A                  Server (WS)              User B
  │                        │                       │
  │  voice_join ch:X       │                       │
  │ ─────────────────────► │                       │
  │                        │  voice_state_update   │
  │                        │  (A joined ch:X)      │ ─────────────────────►
  │                        │                       │  voice_join ch:X
  │                        │ ◄──────────────────────
  │                        │  voice_state_update
  │ ◄─────────────────────  (B joined ch:X)
  │                        │
  │  voice_signal          │  (relay to B)
  │  { target: B, sdp: offer }
  │ ─────────────────────► │ ─────────────────────►
  │                        │  voice_signal         │
  │                        │  (relay to A)         │
  │ ◄─────────────────────  { target: A, sdp: answer }
  │                        │ ◄──────────────────────
  │         P2P audio/video stream established     │
  │ ◄──────────────────────────────────────────── │
```

Screen share is treated as an additional video track in the same WebRTC connection.

---

## 11. Client Integration Checklist

For a Poly client to connect to a poly-server instance:

1. **Discovery**: `GET /server-info` → show server name, check `invite_only`.
2. **Auth**: `POST /auth/signup` or `POST /auth/signin` → store JWT securely.
3. **WS connect**: Open `ws://host/ws?token=<JWT>` — reconnect on disconnect.
4. **Load servers**: `GET /servers` → render server list.
5. **Load channels**: `GET /servers/:id/channels` → render channel tree.
6. **Load DMs**: `GET /channels/@dms` → render DM list.
7. **Load messages**: `GET /channels/:id/messages` → render chat with pagination.
8. **Send message**: `POST /channels/:id/messages` + listen for `message_created` WS event.
9. **Upload file**: `POST /attachments` → include ID in message body.
10. **Voice**: `voice_join` WS message → WebRTC signalling via `voice_signal` relay.
11. **Settings**: `GET /auth/devices`, `DELETE /auth/devices/:id` for device management.
