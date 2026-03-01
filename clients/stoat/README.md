# poly-stoat

Stoat (formerly Revolt) messenger client for **Poly** (PolyGlot Messenger).

## Purpose

Implements the `ClientBackend` trait for Stoat/Revolt messenger. Supports both the official server and self-hosted instances.

## Features

- Email/password authentication
- Server browsing with categories and channels
- Text messaging (send, receive, edit, delete)
- Voice and video calling (WebRTC via Vortex)
- Real-time events via WebSocket
- Friend management and DMs
- Group chats
- Self-hosted instance support (configurable base URL)

## Implementation

Built from scratch using the Stoat REST API + WebSocket protocol. No existing Rust SDK — this is a custom implementation.

- API docs: https://developers.stoat.chat
- REST API for CRUD operations
- WebSocket (Bonfire) for real-time events
- WebRTC (Vortex) for voice/video

## License

MIT / Apache-2.0
