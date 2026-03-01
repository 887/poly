# poly-client

Shared messenger client protocol for **Poly** (PolyGlot Messenger).

## Purpose

Defines the `ClientBackend` trait that all messenger backends must implement, plus shared data types for servers, channels, messages, users, and events.

This crate is the **contract** between `poly-core` (the UI/app logic) and the backend implementations (`poly-stoat`, `poly-matrix`, `poly-discord`, `poly-teams`, `poly-demo`).

## Key Types

- `ClientBackend` — trait for all backend operations (auth, servers, channels, messages, users, events)
- `Server` — a community/workspace (Discord guild, Stoat server, Matrix Space, Teams team)
- `Channel` — text/voice/video channel within a server
- `Message` — a chat message with content, author, timestamp, attachments
- `User` — user profile with name, avatar, presence
- `ClientEvent` — real-time event enum (new message, presence change, etc.)
- `BackendType` — enum identifying the backend (Stoat, Matrix, Discord, Teams, Demo)

## Design

- Backend-agnostic: no imports from any specific backend crate
- Minimal dependencies: serde, chrono, futures only
- All methods async
- Event-driven via `Stream<Item = ClientEvent>`

## License

MIT / Apache-2.0
