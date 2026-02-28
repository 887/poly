# poly-stoat — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-stoat` implements the `ClientBackend` trait for **Stoat** (formerly Revolt) messenger. Supports both the official Stoat server and self-hosted instances.

## Implementation Phase

**Phase 3.1** — First real backend to implement. See [Phase 3 Plan](../../docs/phase-3-plan.md) section 3.1.

## Technology

- **Protocol**: REST API + WebSocket for real-time events
- **API Documentation**: https://developers.stoat.chat
- **Auth**: Email/password login → session token
- **Self-hosted**: Configurable base URL (different Stoat/Revolt instances)
- **Voice/Video**: WebRTC-based (Stoat's Vortex voice server)

## Research Notes (Phase 1)

### API Overview
- Stoat (Revolt) rebranded in 2025. API docs at `developers.stoat.chat`.
- The backend is written in Rust, but there is NO official Rust client SDK.
- Existing Rust crates (`revolt-rs`, `rive`) are unmaintained (2+ years old).
- We are building this client from scratch using the REST/WebSocket API.

### Key API Areas
- **Auth**: `POST /auth/session/login` — email/password → token
- **Servers**: `GET /servers/{id}`, server members, roles
- **Channels**: `GET /channels/{id}`, messages, typing indicators
- **Messages**: `GET/POST/PATCH/DELETE` on channel messages
- **Users**: `GET /users/{id}`, relationships (friends)
- **WebSocket**: `wss://ws.stoat.chat` — Bonfire real-time protocol
- **Voice**: Vortex voice server (WebRTC with SDP exchange)

### Type Mapping
| Stoat Concept | Poly Type |
|---|---|
| Server | `Server` |
| Channel (Text/Voice) | `Channel` |
| Category | `Category` |
| User | `User` |
| Group (DM with multiple users) | `Group` |
| Direct Message | `DmChannel` |

### No Existing Rust SDK
Must build from scratch:
1. HTTP client (reqwest) for REST API
2. WebSocket client (tokio-tungstenite) for real-time events
3. Type definitions matching Stoat API schemas
4. Auth flow management
5. WebRTC integration for voice/video (Vortex protocol)

## Dependencies

- `poly-client` — trait to implement
- `reqwest` — HTTP client
- `tokio-tungstenite` — WebSocket
- `serde`, `serde_json` — API type (de)serialization
- `url` — base URL management
- `webrtc` — voice/video (Phase 3.1)

## Module Structure

```
src/
├── lib.rs           # StoatClient struct + ClientBackend impl
├── api/             # REST API client
│   ├── mod.rs
│   ├── auth.rs      # Login, session management
│   ├── servers.rs   # Server operations
│   ├── channels.rs  # Channel operations
│   ├── messages.rs  # Message CRUD
│   ├── users.rs     # User profiles, friends
│   └── voice.rs     # Voice/video signaling
├── ws/              # WebSocket event handling
│   ├── mod.rs
│   ├── connection.rs # Connection management, reconnect
│   └── events.rs    # Event parsing, mapping to ClientEvent
├── types/           # Stoat-specific type definitions
│   ├── mod.rs
│   └── ...          # Matching Stoat API schemas
└── voice/           # WebRTC voice/video
    ├── mod.rs
    └── vortex.rs    # Vortex voice protocol
```
