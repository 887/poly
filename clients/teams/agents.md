# poly-teams — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-teams` implements the `ClientBackend` trait for **Microsoft Teams** using the **Microsoft Graph API**.

## Implementation Phase

**Phase 3.4** — Last backend to implement. See [Phase 3 Plan](../../docs/phase-3-plan.md) section 3.4.

## Technology

- **API**: Microsoft Graph REST API (https://graph.microsoft.com/v1.0/)
- **Auth**: OAuth2 with Azure AD
  - Device Code Flow (for headless/terminal)
  - Authorization Code Flow with PKCE (for browser-based)
- **Reference Implementation**: `ttyms` crate — terminal Microsoft Teams client in Rust

## Research Notes (Phase 1)

### ttyms Reference
- Crate: `ttyms = "0.1.4"` (released ~2026-02-27, very new)
- Architecture: Microsoft Graph API over HTTPS
- Auth: OAuth2 Device Code Flow or PKCE browser flow
- Ships with a **default Azure AD client ID** (works out of the box)
- Features:
  - 1:1 and group chat (send/receive/edit/delete)
  - Teams & Channels browsing
  - Message reactions
  - Presence/status
  - Vim-style navigation (TUI)
- Token storage: OS credential manager (`keyring` crate)
- Sensitive data zeroized in memory
- Scopes: Minimal permissions, delegated (user context only)

### Microsoft Graph API Endpoints

**Teams & Channels** (Team = Poly Server):
- `GET /me/joinedTeams` — list teams
- `GET /teams/{team-id}/channels` — list channels in team
- `GET /teams/{team-id}/channels/{channel-id}/messages` — channel messages
- `POST /teams/{team-id}/channels/{channel-id}/messages` — send message

**Chat** (1:1 and Group):
- `GET /me/chats` — list all chats
- `GET /chats/{chat-id}/messages` — chat messages
- `POST /chats/{chat-id}/messages` — send message
- Chat types: `oneOnOne`, `group`, `meeting`

**Users & Presence**:
- `GET /me` — current user profile
- `GET /users/{id}` — user profile
- `GET /me/presence` — current presence
- `GET /communications/presences` — batch presence

**Subscriptions** (real-time-ish):
- `POST /subscriptions` — webhook subscriptions for change notifications
- Alternative: polling at intervals

### Teams → Poly Mapping

| Teams Concept | Poly Type |
|---|---|
| Team | `Server` |
| Channel (in Team) | `Channel` |
| 1:1 Chat | `DmChannel` |
| Group Chat | `Group` (displayed under DMs with Teams icon) |
| Meeting | Not mapped (stub only) |
| User | `User` |

### Auth Flow
1. Open browser → Azure AD login page
2. User authenticates with Microsoft account
3. Redirect back with auth code
4. Exchange for access token + refresh token
5. Store tokens securely (local SurrealKV, encrypted for backup)

### Rate Limiting
- Microsoft Graph has per-app and per-user throttling
- 429 responses with Retry-After header
- Need exponential backoff logic

## Dependencies

- `poly-client` — trait to implement
- `reqwest` — HTTP client for Graph API
- `oauth2` — OAuth2 flow handling
- `serde`, `serde_json` — API response parsing
- `tokio` — async runtime
- `url` — URL construction

## Module Structure

```
src/
├── lib.rs              # TeamsClient struct + ClientBackend impl
├── auth.rs             # OAuth2 (Device Code + PKCE)
├── graph/              # Microsoft Graph API client
│   ├── mod.rs
│   ├── teams.rs        # Teams + Channels
│   ├── chats.rs        # 1:1 and group chats
│   ├── messages.rs     # Message operations
│   ├── users.rs        # User profiles, presence
│   └── subscriptions.rs # Change notification subscriptions
├── types/              # Teams-specific type definitions
│   ├── mod.rs
│   └── ...             # Matching Graph API schemas
└── rate_limit.rs       # Rate limiting + retry logic
```

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.
