# Phase 3 Plan — Client Implementations

> **Status:** ⬜ Not Started  
> **Target Start:** After Phase 2 completion  
> **Parent:** [Overall Plan](overall-plan.md)  
> **Depends On:** [Phase 2](phase-2-plan.md)

---

## 3.0 Pre-Implementation

- [ ] **3.0.1** Update all crate dependencies to latest stable versions
- [ ] **3.0.2** Review and update `last-crate-update-date`
- [ ] **3.0.3** Verify demo client still works as expected after any Dioxus/SurrealDB updates
- [ ] **3.0.4** Review overall plan for any changes needed based on Phase 2 learnings

> **NOTE:** The Poly-Server Test Client (formerly 3.0.5–3.0.8) has been moved to
> [Phase 2.7](phase-2.7-plan.md). It is now completed as part of Phase 2 to validate
> the `ClientBackend` integration path before external protocol work begins.

---

## 3.1 Stoat (Revolt) Client + Voice/Video Infrastructure

> **Crate:** `poly-stoat`  
> **Goal:** Full chat + voice + video with Stoat servers. WebRTC infrastructure built here is reused by all other backends.

### 3.1.1 Research & Planning
- [ ] **3.1.1.1** Deep-dive Stoat REST API (developer docs at `developers.stoat.chat`)
- [ ] **3.1.1.2** Document all REST endpoints needed: auth, servers, channels, messages, users, voice
- [ ] **3.1.1.3** Document WebSocket event protocol
- [ ] **3.1.1.4** Document Stoat auth flow (email/password, OAuth if available)
- [ ] **3.1.1.5** Document Stoat voice/video protocol (WebRTC specifics)
- [ ] **3.1.1.6** Test against official Stoat server and a self-hosted instance
- [ ] **3.1.1.7** Update `clients/stoat/agents.md` with all findings

### 3.1.2 Core Stoat Client
- [ ] **3.1.2.1** HTTP client setup (reqwest or similar) with base URL configuration
- [ ] **3.1.2.2** Authentication (email/password login, token storage)
- [ ] **3.1.2.3** Implement `ClientBackend` trait for `StoatClient`
- [ ] **3.1.2.4** Server list retrieval
- [ ] **3.1.2.5** Channel list per server (with categories)
- [ ] **3.1.2.6** Message retrieval (paginated)
- [ ] **3.1.2.7** Send messages (text, with attachments)
- [ ] **3.1.2.8** User profiles and presence
- [ ] **3.1.2.9** Friend list and friend requests
- [ ] **3.1.2.10** Group DMs / multi-user chats
- [ ] **3.1.2.11** Self-hosted instance support (configurable base URL, API version detection)

### 3.1.3 Real-Time Events
- [ ] **3.1.3.1** WebSocket connection management (connect, reconnect, heartbeat)
- [ ] **3.1.3.2** Message events (new, edit, delete)
- [ ] **3.1.3.3** Presence updates
- [ ] **3.1.3.4** Typing indicators
- [ ] **3.1.3.5** Notification events
- [ ] **3.1.3.6** Channel/server update events
- [ ] **3.1.3.7** Map Stoat events → `ClientEvent` enum

### 3.1.4 WebRTC Voice Infrastructure (SHARED)
- [ ] **3.1.4.1** Set up `webrtc = "0.17.1"` in workspace
- [ ] **3.1.4.2** ICE candidate gathering (STUN/TURN)
- [ ] **3.1.4.3** DTLS handshake
- [ ] **3.1.4.4** Audio capture (platform-specific: ALSA/PulseAudio on Linux, CoreAudio on Mac, etc.)
- [ ] **3.1.4.5** Audio encoding/decoding (Opus codec)
- [ ] **3.1.4.6** RTP/SRTP audio streaming
- [ ] **3.1.4.7** Voice channel join/leave
- [ ] **3.1.4.8** Mute/unmute, deafen controls
- [ ] **3.1.4.9** Voice activity detection (VAD)
- [ ] **3.1.4.10** Platform bridge for mobile (iOS/Android mic access)

### 3.1.5 WebRTC Video Infrastructure (SHARED)
- [ ] **3.1.5.1** Camera capture (platform-specific bridges)
- [ ] **3.1.5.2** Video encoding (VP8/VP9/H264)
- [ ] **3.1.5.3** Video decoding and rendering
- [ ] **3.1.5.4** Screen sharing capture
- [ ] **3.1.5.5** Video channel join/leave
- [ ] **3.1.5.6** Camera on/off controls
- [ ] **3.1.5.7** Platform bridge for mobile (iOS/Android camera access)

### 3.1.6 Integration Testing
- [ ] **3.1.6.1** Test auth flow with real Stoat server
- [ ] **3.1.6.2** Test message send/receive in real channels
- [ ] **3.1.6.3** Test voice call with another Stoat user
- [ ] **3.1.6.4** Test video call with another Stoat user
- [ ] **3.1.6.5** Test with self-hosted instance
- [ ] **3.1.6.6** Test adding Stoat server to favorites in Poly UI
- [ ] **3.1.6.7** Test friend management through Poly

### 3.1 Completion Criteria
- [ ] Can log into a Stoat account through Poly settings
- [ ] Can view Stoat servers and add them to favorites
- [ ] Can browse channels within a Stoat server
- [ ] Can send and receive text messages in real-time
- [ ] Can make voice calls to other Stoat users
- [ ] Can make video calls to other Stoat users
- [ ] Friend list displays correctly with search
- [ ] Notifications work for DMs and mentions
- [ ] Self-hosted instances work with custom base URL

---

## 3.2 Matrix Client

> **Crate:** `poly-matrix`  
> **Goal:** Chat with Matrix homeservers. Spaces = servers, rooms = channels, E2EE supported.

### 3.2.1 Research & Planning
- [ ] **3.2.1.1** Deep-dive `matrix-sdk` 0.16.0 API
- [ ] **3.2.1.2** Document Spaces → server mapping strategy
- [ ] **3.2.1.3** Document room → channel mapping
- [ ] **3.2.1.4** Document SSO login flow
- [ ] **3.2.1.5** Document E2EE setup (Olm/Megolm, cross-signing)
- [ ] **3.2.1.6** Document VoIP / voice/video signaling
- [ ] **3.2.1.7** Research public homeserver directory (list of major federated servers)
- [ ] **3.2.1.8** Update `clients/matrix/agents.md`

### 3.2.2 Core Matrix Client
- [ ] **3.2.2.1** Initialize `matrix-sdk` client with homeserver URL
- [ ] **3.2.2.2** SSO login flow (open browser, handle callback)
- [ ] **3.2.2.3** Username/password login flow
- [ ] **3.2.2.4** Implement `ClientBackend` trait for `MatrixClient`
- [ ] **3.2.2.5** Map Matrix Spaces → Poly servers
- [ ] **3.2.2.6** Map Matrix rooms → Poly channels
- [ ] **3.2.2.7** "Fake servers" — user-created room groupings for rooms not in Spaces
- [ ] **3.2.2.8** DM rooms → direct messages
- [ ] **3.2.2.9** Multi-user rooms → group chats
- [ ] **3.2.2.10** Message send/receive (text, images, files)
- [ ] **3.2.2.11** User profiles, presence, avatars
- [ ] **3.2.2.12** Room membership list
- [ ] **3.2.2.13** Federation: join rooms on any homeserver

### 3.2.3 E2EE
- [ ] **3.2.3.1** Enable matrix-sdk-crypto
- [ ] **3.2.3.2** Device verification (QR code, emoji)
- [ ] **3.2.3.3** Cross-signing setup
- [ ] **3.2.3.4** Encrypted message send/receive
- [ ] **3.2.3.5** Key backup and recovery

### 3.2.4 Real-Time Sync
- [ ] **3.2.4.1** Sync loop (matrix-sdk built-in)
- [ ] **3.2.4.2** Map sync events → ClientEvent enum
- [ ] **3.2.4.3** Typing indicators
- [ ] **3.2.4.4** Read receipts
- [ ] **3.2.4.5** Presence updates

### 3.2.5 Voice/Video (Matrix VoIP)
- [ ] **3.2.5.1** Matrix VoIP signaling (m.call events)
- [ ] **3.2.5.2** Integrate with shared WebRTC infrastructure from 3.1
- [ ] **3.2.5.3** 1:1 voice calls
- [ ] **3.2.5.4** 1:1 video calls
- [ ] **3.2.5.5** Group calls (if supported by matrix-sdk)

### 3.2.6 Public Server Directory
- [ ] **3.2.6.1** Show matrix.org as default homeserver
- [ ] **3.2.6.2** Fetch/display major public homeservers
- [ ] **3.2.6.3** Room directory browsing (public rooms on a homeserver)

### 3.2 Completion Criteria
- [ ] Can log into matrix.org and other homeservers
- [ ] Spaces display as servers with categories
- [ ] Rooms display as channels
- [ ] E2EE works for 1:1 and group chats
- [ ] Voice and video calls work
- [ ] Can create "fake servers" to group Matrix rooms
- [ ] Federation works (join rooms across homeservers)

---

## 3.3 Discord Client

> **Crate:** `poly-discord`  
> **Goal:** View and interact with Discord servers/channels/DMs. Approach TBD.

### 3.3.1 Research & Approach Decision
- [ ] **3.3.1.1** Re-evaluate Discord client landscape (new crates? policy changes?)
- [ ] **3.3.1.2** Test `discord_client_gateway` / `discord_client_rest` crates (if still maintained)
- [ ] **3.3.1.3** Evaluate bridge approach (Matrix bridge via mautrix-discord)
- [ ] **3.3.1.4** Evaluate webview approach (hidden webview running Discord web)
- [ ] **3.3.1.5** Evaluate hybrid approach (background official client + data extraction)
- [ ] **3.3.1.6** **DECISION: Choose implementation approach** (document in agents.md)
- [ ] **3.3.1.7** Document TOS implications and user warnings needed

### 3.3.2 Implementation (approach-dependent)
- [ ] **3.3.2.1** Auth flow (token, OAuth, or webview-based)
- [ ] **3.3.2.2** Implement `ClientBackend` trait for `DiscordClient`
- [ ] **3.3.2.3** Server (guild) retrieval
- [ ] **3.3.2.4** Channel list with categories
- [ ] **3.3.2.5** Message send/receive
- [ ] **3.3.2.6** User profiles, presence, avatars
- [ ] **3.3.2.7** DMs and group DMs (up to ~10 users)
- [ ] **3.3.2.8** Friend list and friend requests
- [ ] **3.3.2.9** Server icons and channel info
- [ ] **3.3.2.10** Self-hosted Discord API support (custom base URL)

### 3.3.3 Real-Time Events
- [ ] **3.3.3.1** Gateway WebSocket connection (or bridge events)
- [ ] **3.3.3.2** Message events
- [ ] **3.3.3.3** Presence updates
- [ ] **3.3.3.4** Typing indicators

### 3.3.4 Voice/Video
- [ ] **3.3.4.1** Discord voice gateway integration
- [ ] **3.3.4.2** Voice channel join/leave
- [ ] **3.3.4.3** Video/screen share

### 3.3 Completion Criteria
- [ ] Can view Discord servers and channels
- [ ] Can send and receive messages
- [ ] DMs and group DMs work
- [ ] Voice channels work (stretch)
- [ ] User is warned about TOS implications

---

## 3.4 Microsoft Teams Client

> **Crate:** `poly-teams`  
> **Goal:** Teams workspaces as servers, channels, group chats as DMs. Via Microsoft Graph API.

### 3.4.1 Research & Planning
- [ ] **3.4.1.1** Study `ttyms` source code in detail
- [ ] **3.4.1.2** Document Microsoft Graph API endpoints for Teams
- [ ] **3.4.1.3** Document OAuth2 flow (Device Code + PKCE)
- [ ] **3.4.1.4** Document Azure AD app registration (or use default client ID from ttyms)
- [ ] **3.4.1.5** Document API rate limits and throttling
- [ ] **3.4.1.6** Update `clients/teams/agents.md`

### 3.4.2 Core Teams Client
- [ ] **3.4.2.1** OAuth2 authentication (Device Code Flow + PKCE browser flow)
- [ ] **3.4.2.2** Token storage and refresh
- [ ] **3.4.2.3** Implement `ClientBackend` trait for `TeamsClient`
- [ ] **3.4.2.4** Teams-Teams → Poly servers
- [ ] **3.4.2.5** Channels within Teams → Poly channels
- [ ] **3.4.2.6** 1:1 chat → DMs
- [ ] **3.4.2.7** Group chats → multi-user DMs (Teams icon as source)
- [ ] **3.4.2.8** Messages: send, receive, edit, delete, reactions
- [ ] **3.4.2.9** User profiles, presence, avatars
- [ ] **3.4.2.10** Contact/people list

### 3.4.3 Real-Time Events
- [ ] **3.4.3.1** Microsoft Graph subscriptions (webhooks) or polling
- [ ] **3.4.3.2** New message notifications
- [ ] **3.4.3.3** Presence changes
- [ ] **3.4.3.4** Typing indicators (if available via Graph)

### 3.4.4 Voice/Video
- [ ] **3.4.4.1** Research Teams calling via Graph API
- [ ] **3.4.4.2** Evaluate feasibility (may be limited without Teams client)
- [ ] **3.4.4.3** Implement if feasible, otherwise mark as limitation

### 3.4 Completion Criteria
- [ ] Can log into Microsoft account via OAuth
- [ ] Teams workspaces display as servers with channels
- [ ] Can send and receive messages in channels and DMs
- [ ] Group chats display correctly with Teams icon
- [ ] User presence shows correctly
- [ ] Notifications work for mentions and DMs

---

## Phase 3 Overall Completion Criteria

- [ ] At least 2 backends fully working (Stoat + Matrix minimum)
- [ ] All backends implement `ClientBackend` trait
- [ ] Feature flags work — can build with any subset of backends
- [ ] Voice/video works for at least Stoat + Matrix
- [ ] Multi-account per backend works
- [ ] Cross-backend favorites sidebar works correctly
- [ ] Cross-backend DM/friends view works correctly
- [ ] Cross-backend notification aggregation works
- [ ] All backends respect encrypted backup sync
