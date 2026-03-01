# poly-matrix

Matrix protocol client for **Poly** (PolyGlot Messenger).

## Purpose

Implements the `ClientBackend` trait for Matrix using the official `matrix-sdk` Rust crate (the same SDK that powers Element X).

## Features

- Username/password and SSO authentication
- Matrix Spaces displayed as servers (with room hierarchies as categories)
- Rooms displayed as channels
- End-to-end encryption (Olm/Megolm)
- Device verification (QR code, emoji)
- Voice and video calls (Matrix VoIP + WebRTC)
- Federation — works with any Matrix homeserver
- Public room directory browsing
- "Fake servers" — user-created local groupings for rooms not in Spaces
- DMs and multi-user group chats

## Key Dependency

- `matrix-sdk = "0.16.0"` — production-grade Matrix Rust SDK

## License

MIT / Apache-2.0
