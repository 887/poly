# poly-teams

Microsoft Teams client for **Poly** (PolyGlot Messenger).

## Purpose

Implements the `ClientBackend` trait for Microsoft Teams using the Microsoft Graph REST API.

## Features

- OAuth2 authentication (Device Code Flow + PKCE browser flow)
- Teams displayed as servers with channels
- 1:1 chats as DMs
- Group chats as multi-user groups (displayed under DMs with Teams icon)
- Send, receive, edit, delete messages with reactions
- User presence and status
- Contact/people discovery

## Implementation

Built on the Microsoft Graph API (`graph.microsoft.com/v1.0/`). References the `ttyms` crate for auth flow and API patterns.

Ships with a default Azure AD client ID for out-of-the-box usage.

## License

MIT / Apache-2.0
