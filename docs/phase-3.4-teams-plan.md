# Phase 3.4 Plan — Microsoft Teams Client

> **Created:** 2026-03-30
> **Status:** ⬜ Not Started
> **Crate:** `poly-teams`
> **Goal:** Teams workspaces as servers, channels as channels, group chats as DMs. Via Microsoft Graph API.
> **Auth:** OAuth2 Device Code Flow + PKCE browser flow

---

## 3.4.1 Research & Planning

- [ ] **3.4.1.1** Study `ttyms` source code in detail (reference implementation)
- [ ] **3.4.1.2** Document Microsoft Graph API endpoints for Teams (teams, channels, messages, chats, members, presence)
- [ ] **3.4.1.3** Document OAuth2 flow (Device Code for headless + PKCE for desktop browser)
- [ ] **3.4.1.4** Document Azure AD app registration (or reuse default client ID from `ttyms`)
- [ ] **3.4.1.5** Document Graph API rate limits and throttling behavior
- [ ] **3.4.1.6** Update `clients/teams/agents.md` with all findings

---

## 3.4.2 Core Teams Client

- [ ] **3.4.2.1** OAuth2 authentication (Device Code Flow + PKCE browser flow)
- [ ] **3.4.2.2** Token storage, refresh, and silent re-auth
- [ ] **3.4.2.3** Implement `ClientBackend` trait for `TeamsClient`
- [ ] **3.4.2.4** Teams-Teams → Poly servers (joined teams list via Graph `GET /me/joinedTeams`)
- [ ] **3.4.2.5** Channels within Teams → Poly channels (with category/tab info)
- [ ] **3.4.2.6** 1:1 chat → DMs (Graph `GET /me/chats` filtered by chat type `oneOnOne`)
- [ ] **3.4.2.7** Group chats → multi-user DMs (`group` chat type, Teams icon as source indicator)
- [ ] **3.4.2.8** Messages: send, receive, edit, delete, reactions (Graph `/messages` endpoints)
- [ ] **3.4.2.9** User profiles, presence, avatars (`/users/{id}`, `/users/{id}/presence`)
- [ ] **3.4.2.10** Contact/people list (`/me/people`, `/users` search)

---

## 3.4.3 Real-Time Events

- [ ] **3.4.3.1** Microsoft Graph subscriptions (webhooks) or change notifications
- [ ] **3.4.3.2** New message notifications
- [ ] **3.4.3.3** Presence changes
- [ ] **3.4.3.4** Typing indicators (if available via Graph API)

---

## 3.4.4 Voice/Video

- [ ] **3.4.4.1** Research Teams calling via Graph API (Communications API)
- [ ] **3.4.4.2** Evaluate feasibility (likely limited without official Teams client integration)
- [ ] **3.4.4.3** Implement if feasible; otherwise document as known limitation

---

## Completion Criteria

- [ ] Can log into Microsoft account via OAuth2
- [ ] Teams workspaces display as servers with channels
- [ ] Can send and receive messages in channels and DMs
- [ ] Group chats display correctly with Teams source indicator
- [ ] User presence shows correctly
- [ ] Notifications work for mentions and DMs
- [ ] Contact list is accessible
