# poly-matrix — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-matrix` implements the `ClientBackend` trait for **Matrix** protocol using the `matrix-sdk` Rust crate.

## Implementation Phase

**Phase 3.2** — Second real backend. See [Phase 3 Plan](../../docs/phase-3-plan.md) section 3.2.

## Technology

- **SDK**: `matrix-sdk = "0.16.0"` (production-grade, powers Element X)
- **Protocol**: Matrix client-server API over HTTPS + sync
- **E2EE**: Olm/Megolm via `matrix-sdk-crypto` (Vodozemac implementation)
- **Storage**: `matrix-sdk-sqlite` for session/crypto state (or integrate with our SurrealKV)
- **Auth**: Username/password, SSO (OIDC), token-based
- **Federation**: Any Matrix homeserver (matrix.org default)

## Research Notes (Phase 1)

### Matrix Concepts → Poly Mapping

| Matrix Concept | Poly Type | Notes |
|---|---|---|
| Space | `Server` | A Space organizes rooms into a hierarchy |
| Room | `Channel` | Rooms are channels (text by default) |
| Space child rooms | Channels in categories | Spaces can nest rooms in sub-spaces (categories) |
| User | `User` | Matrix user ID: @user:homeserver.tld |
| DM (2-person room) | `DmChannel` | |
| Multi-person room | `Group` | Rooms with 3+ members that aren't in a Space |
| VoIP events | Voice/Video | m.call.* events for WebRTC signaling |

### "Fake Servers" Feature
For Matrix rooms NOT in any Space, Poly lets users create custom groupings:
- User creates a "fake server" (named group)
- Drags rooms into it, creating categories
- Stored locally in SurrealKV, not on the Matrix server
- Displayed exactly like regular servers in the sidebar

### matrix-sdk Architecture
- `matrix_sdk::Client` — main client object
- `Client::sync()` — sync loop for real-time updates  
- `Room` type — represents a room (joined, invited, left)
- `Room::messages()` — paginated message history
- `Room::send()` — send messages
- `Room::typing_notice()` — typing indicators
- `RoomListService` — high-level room list management
- `Encryption` — automatic E2EE handling

### Key matrix-sdk Features
- Automatic E2EE (opt-in per room)
- Cross-signing and device verification (QR code, emoji)
- Lazy-loading room members
- Push notification rules
- SSO / OIDC authentication
- WASM support (for web target)
- SQLite or IndexedDB (web) storage

### Public Server Directory
- matrix.org is the default/largest homeserver
- `matrix.to` links for room/user discovery
- Room directory API: `GET /_matrix/client/v3/publicRooms` per homeserver
- Can fetch public rooms from any federated server

## Dependencies

- `poly-client` — trait to implement
- `matrix-sdk` — official Matrix Rust SDK
- `matrix-sdk-sqlite` — storage backend (or custom SurrealKV adapter)
- `tokio` — async runtime

## Module Structure

```
src/
├── lib.rs              # MatrixClient struct + ClientBackend impl
├── auth.rs             # Login flows (password, SSO, token)
├── sync.rs             # Sync loop management, event mapping
├── rooms.rs            # Room → Channel/Server/DM mapping
├── spaces.rs           # Space → Server mapping + fake servers
├── messages.rs         # Message send/receive/history
├── users.rs            # User profiles, presence, friends
├── encryption.rs       # E2EE setup, device verification
├── voip.rs             # VoIP signaling for voice/video
└── directory.rs        # Public room/server directory browsing
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
