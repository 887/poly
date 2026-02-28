# Client Backend Research

> **Compiled:** 2026-02-28  
> **Phase:** 1 (Planning & Research)

---

## 1. Stoat (formerly Revolt)

### Overview
- **Website:** https://stoat.chat (formerly revolt.chat)
- **Developer Docs:** https://developers.stoat.chat
- **GitHub:** Moving to `github.com/stoatchat` (was `revoltchat`)
- **Protocol:** REST API + WebSocket (Bonfire) + WebRTC (Vortex)
- **Backend:** Written in Rust (ironic that there's no Rust client SDK)
- **License:** AGPL-3.0 (server), varies for clients
- **Self-hosted:** Yes, fully self-hostable with Docker

### Rust Client Libraries

| Crate | Version | Downloads | Status |
|---|---|---|---|
| `revolt-rs` | 0.0.3 | 3,607 | **Unmaintained (2+ years)** |
| `rive` | 1.2.0 | 9,894 | **Unmaintained (2+ years)** |

**Verdict:** No viable Rust SDK. Must build from API docs.

### API Structure (from developer docs)

**Authentication:**
- `POST /auth/session/login` — email + password → session token
- `POST /auth/session/logout` — invalidate session
- Token passed via `x-session-token` header

**Servers:**
- `GET /servers/{server_id}` — server info
- `GET /servers/{server_id}/members` — member list
- Server has: name, icon, banner, categories (channel groupings)

**Channels:**
- `GET /channels/{channel_id}` — channel info
- Types: Text, Voice, Group (DM), DM, SavedMessages
- Text channels have: name, description, permission overrides

**Messages:**
- `GET /channels/{channel_id}/messages` — paginated messages
- `POST /channels/{channel_id}/messages` — send message
- `PATCH /channels/{channel_id}/messages/{msg_id}` — edit
- `DELETE /channels/{channel_id}/messages/{msg_id}` — delete
- Message has: content, author, attachments, embeds, reactions

**Users:**
- `GET /users/{user_id}` — user profile
- `GET /users/{user_id}/mutual` — mutual friends/servers
- Relationships: Friend, Outgoing, Incoming, Blocked, None

**WebSocket (Bonfire):**
- URL: `wss://ws.revolt.chat` (or custom for self-hosted)
- Events: Ready, Message, MessageUpdate, MessageDelete, ChannelCreate, ServerUpdate, UserUpdate, etc.
- Heartbeat: periodic ping/pong

**Voice (Vortex):**
- WebRTC-based voice server
- `POST /channels/{channel_id}/join_call` — get voice server info
- SDP exchange via signaling
- Supports: voice chat in voice channels

### Self-Hosted Support
- Different base URLs for REST, WebSocket, voice
- All configurable — just change the base URL
- Same API structure

---

## 2. Matrix

### Overview
- **Spec:** https://spec.matrix.org
- **SDK:** `matrix-sdk = "0.16.0"` (official Rust SDK)
- **Powers:** Element X (iOS/Android), Fractal, iamb
- **Protocol:** Client-Server API over HTTPS + sync
- **Federation:** Fully federated — any homeserver talks to any other
- **E2EE:** Olm/Megolm (Vodozemac in Rust)

### Rust SDK Ecosystem

| Crate | Version | Purpose |
|---|---|---|
| `matrix-sdk` | 0.16.0 | Mid-level client API |
| `matrix-sdk-ui` | 0.16.0 | High-level timeline/room list |
| `matrix-sdk-crypto` | 0.16.0 | E2EE (Vodozemac) |
| `matrix-sdk-sqlite` | 0.16.0 | SQLite storage backend |
| `matrix-sdk-indexeddb` | 0.16.0 | IndexedDB for WASM |

**Verdict:** Production-ready. Best library situation of all our backends.

### Concept Mapping

**Spaces (MSC1772):**
- Matrix Spaces are hierarchical collections of rooms
- A Space = Poly Server
- Sub-spaces = Categories
- Rooms in a Space = Channels
- This is the closest Matrix concept to Discord guilds/Stoat servers

**Rooms without Spaces:**
- Many Matrix rooms exist independently (no Space)
- Poly solution: "Fake servers" — user creates local groupings
- Stored in SurrealKV, not on Matrix homeserver
- Displayed like regular servers in the sidebar

**Room Types:**
- Regular room → Text channel
- DM (2 people) → DmChannel
- Multi-person room (not in Space) → Group chat
- Voice/video: Matrix VoIP (m.call.* events)

### Auth Flows
1. **Username/Password**: `POST /_matrix/client/v3/login` with `m.login.password`
2. **SSO**: Redirect to Identity Provider, callback with login token
3. **OIDC**: Native OIDC support (newer homeservers)
4. **Token**: Direct token auth for existing sessions

### E2EE Details
- **Olm**: 1:1 channel establishment (Double Ratchet variant)
- **Megolm**: Group encryption (one ratchet per session, forward secrecy on rotation)
- **Cross-signing**: Device verification chains
- **QR verification**: Scan QR codes between devices
- **Emoji verification**: Compare emoji sequences
- **Key backup**: Server-side encrypted key storage

### VoIP (Voice/Video)
- Matrix defines VoIP via `m.call.*` events
- SDP (Session Description Protocol) exchange via room events
- ICE candidate exchange via room events
- WebRTC for actual media transport
- 1:1 calls well-supported
- Group calls: Element Call / MSC3401 — newer, check matrix-sdk support

### Public Homeservers
- **matrix.org** — largest, default
- **envs.net** — privacy-focused
- **tchncs.de** — German, privacy-focused
- **nitro.chat** — community
- Room directory: `GET /_matrix/client/v3/publicRooms?server={homeserver}`

---

## 3. Discord

### Overview
- **API:** REST + Gateway WebSocket + Voice Gateway
- **TOS:** **Explicitly prohibits** unofficial clients
- **Rust crates:** Pre-alpha only
- **Risk Level:** HIGH

### Available Rust Crates

| Crate | Version | Downloads | Notes |
|---|---|---|---|
| `discord_client_gateway` | 0.2.0 | ~800 | "Undetected" gateway reimpl |
| `discord_client_rest` | 0.1.1 | ~400 | REST companion |

These are by `UwUDev`, very early stage. May work as reference but risky to depend on.

### API Overview (v10)
- **REST:** `https://discord.com/api/v10/`
- **Gateway:** `wss://gateway.discord.gg/?v=10&encoding=json`
- **Voice:** Separate voice WebSocket + WebRTC

**Key endpoints:**
- `/users/@me` — current user
- `/users/@me/guilds` — list servers
- `/guilds/{id}/channels` — server channels
- `/channels/{id}/messages` — channel messages
- `/users/@me/channels` — DM channels
- `/users/@me/relationships` — friends

### Anti-Bot Measures
- Discord actively detects unofficial clients
- Client-side JavaScript challenges (CF Turnstile, custom challenges)
- Fingerprinting of client behavior
- Rate limiting and shadow-banning

### TOS Notice (MUST show to users)
> Using unofficial Discord clients violates Discord's Terms of Service.
> Your account may be suspended or terminated.
> By adding a Discord account to Poly, you acknowledge this risk.

### Approach Options (Decision at Phase 3.3)

1. **Direct API**: Highest risk, cleanest UX. Need to handle challenges.
2. **Webview Bridge**: Run Discord web in hidden webview, intercept events. Moderate risk.
3. **Matrix Bridge**: `mautrix-discord` bridges Discord to Matrix. Needs separate bridge server.
4. **Background Client**: Official Discord running, IPC/scraping. Heaviest.
5. **Minimal JS Runtime**: `boa` (Rust JS engine) to execute client challenges.

---

## 4. Microsoft Teams

### Overview
- **API:** Microsoft Graph REST API
- **Auth:** OAuth2 via Azure AD
- **Reference:** `ttyms` crate (terminal Teams client)
- **Risk Level:** MEDIUM (official API, but rate-limited)

### ttyms Reference Crate

**Version:** `ttyms = "0.1.4"` (released ~2026-02-27)  
**What it demonstrates:**
- Microsoft Graph API for Teams chat (1:1 + group)
- Teams & Channels browsing
- OAuth2 Device Code Flow with default Azure AD client ID
- Token refresh and storage
- Message CRUD with reactions
- Presence/status reading
- Scopes: ChatMessage.Read, ChatMessage.Send, Channel.ReadBasic.All, etc.

**Key takeaway:** Teams is actually quite accessible via official API. The `ttyms` crate proves it works in Rust.

### Microsoft Graph Endpoints

**Authentication:**
```
POST https://login.microsoftonline.com/common/oauth2/v2.0/devicecode
POST https://login.microsoftonline.com/common/oauth2/v2.0/token
```

**Teams (→ Poly Servers):**
```
GET /me/joinedTeams
GET /teams/{team-id}
GET /teams/{team-id}/channels
GET /teams/{team-id}/channels/{channel-id}/messages
POST /teams/{team-id}/channels/{channel-id}/messages
```

**Chats (→ Poly DMs/Groups):**
```
GET /me/chats
GET /chats/{chat-id}
GET /chats/{chat-id}/messages
POST /chats/{chat-id}/messages
```

**Users:**
```
GET /me
GET /users/{id}
GET /me/presence
```

### Chat Types → Poly Mapping
- `oneOnOne` chat → `DmChannel`
- `group` chat → `Group` (with Teams icon badge)
- `meeting` chat → `Group` (with Teams icon badge)

### Rate Limiting
- Per-app: varies by endpoint (typically 10,000/10min)
- Per-user: varies (typically 100-200/min for message endpoints)
- 429 response with Retry-After header
- Need exponential backoff with jitter

### Real-Time Events
Microsoft Graph supports change notifications (webhooks):
- Subscribe to `/chats/getAllMessages` for new messages
- Subscribe to `/communications/presences` for presence changes
- Webhook URL must be publicly accessible (or use polling fallback)
- For desktop/mobile: polling may be simpler (subscription needs a server)

### Voice/Video
- Microsoft Graph has limited calling APIs
- Full Teams calling requires Teams SDK (C++/.NET/JS)
- **Likely NOT feasible** via Graph API alone for pure voice/video
- Mark as known limitation — Teams calls require the official client

---

## 5. Summary & Readiness Assessment

| Backend | SDK Maturity | Effort Required | Risk | Phase |
|---|---|---|---|---|
| **Demo** | N/A (we build it) | Low | None | 2 |
| **Stoat** | None (build from API) | High | Medium | 3.1 |
| **Matrix** | Excellent (matrix-sdk) | Medium | Low | 3.2 |
| **Discord** | Pre-alpha only | Very High | **High** (TOS) | 3.3 |
| **Teams** | Reference (ttyms) | Medium-High | Medium | 3.4 |
