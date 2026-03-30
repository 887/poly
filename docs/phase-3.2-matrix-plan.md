# Phase 3.2 Plan — Matrix Client

> **Created:** 2026-03-30
> **Status:** ⬜ Not Started
> **Crate:** `poly-matrix`
> **Goal:** Chat with Matrix homeservers. Spaces = servers, rooms = channels, E2EE supported. Uses `matrix-sdk`.
> **Depends on:** Phase 3.1 (WebRTC infrastructure built there is reused here)

---

## 3.2.1 Research & Planning

- [ ] **3.2.1.1** Deep-dive `matrix-sdk` (current stable version) API surface
- [ ] **3.2.1.2** Document Spaces → server mapping strategy (Spaces hierarchy, rooms not in Spaces)
- [ ] **3.2.1.3** Document room → channel mapping (name, topic, membership, permissions)
- [ ] **3.2.1.4** Document SSO login flow (browser redirect, callback handling)
- [ ] **3.2.1.5** Document E2EE setup (Olm/Megolm, cross-signing, key backup)
- [ ] **3.2.1.6** Document VoIP signaling (m.call events, group calls via MSC3401)
- [ ] **3.2.1.7** Research major public homeservers (matrix.org, various federated servers)
- [ ] **3.2.1.8** Update `clients/matrix/agents.md` with all findings

---

## 3.2.2 Core Matrix Client

- [ ] **3.2.2.1** Initialize `matrix-sdk` client with homeserver URL and state store
- [ ] **3.2.2.2** SSO login flow (open system browser, handle callback token)
- [ ] **3.2.2.3** Username/password login flow
- [ ] **3.2.2.4** Implement `ClientBackend` trait for `MatrixClient`
- [ ] **3.2.2.5** Map Matrix Spaces → Poly servers (hierarchy, icons, descriptions)
- [ ] **3.2.2.6** Map Matrix rooms → Poly channels (categories from Space children)
- [ ] **3.2.2.7** "Fake servers" — user-created room groupings for rooms not in any Space
- [ ] **3.2.2.8** DM rooms → direct messages (`m.direct` account data)
- [ ] **3.2.2.9** Multi-user rooms → group chats
- [ ] **3.2.2.10** Message send/receive (text, images, files, replies, reactions)
- [ ] **3.2.2.11** User profiles, presence, avatars (MSC1769 / homeserver profile)
- [ ] **3.2.2.12** Room membership list
- [ ] **3.2.2.13** Federation: join rooms on any homeserver (via `#room:server.tld` alias)

---

## 3.2.3 E2EE

- [ ] **3.2.3.1** Enable `matrix-sdk-crypto` in workspace
- [ ] **3.2.3.2** Device verification (QR code + emoji SAS)
- [ ] **3.2.3.3** Cross-signing setup (bootstrapping MSKs/SSKs/USKs)
- [ ] **3.2.3.4** Encrypted message send/receive (Megolm session management)
- [ ] **3.2.3.5** Key backup and recovery (SSSS / 4S)

---

## 3.2.4 Real-Time Sync

- [ ] **3.2.4.1** Sync loop (`matrix-sdk` built-in sliding sync or `/sync` long-poll)
- [ ] **3.2.4.2** Map sync timeline events → `ClientEvent` enum
- [ ] **3.2.4.3** Typing indicators (`m.typing` ephemeral events)
- [ ] **3.2.4.4** Read receipts (`m.read` account data)
- [ ] **3.2.4.5** Presence updates (if homeserver supports it)

---

## 3.2.5 Voice/Video (Matrix VoIP)

- [ ] **3.2.5.1** Matrix VoIP signaling (`m.call.invite`, `m.call.answer`, `m.call.hangup`)
- [ ] **3.2.5.2** Integrate with shared WebRTC infrastructure from Phase 3.1
- [ ] **3.2.5.3** 1:1 voice calls
- [ ] **3.2.5.4** 1:1 video calls
- [ ] **3.2.5.5** Group calls (MSC3401 / Element Call protocol, if supported by `matrix-sdk`)

---

## 3.2.6 Public Server Directory

- [ ] **3.2.6.1** Show `matrix.org` as default homeserver
- [ ] **3.2.6.2** Fetch/display major well-known public homeservers
- [ ] **3.2.6.3** Room directory browsing (public rooms on a homeserver via `GET /_matrix/client/v3/publicRooms`)

---

## Completion Criteria

- [ ] Can log into `matrix.org` and other homeservers (SSO + password)
- [ ] Spaces display as servers with categories
- [ ] Rooms display as channels
- [ ] E2EE works for 1:1 and group chats
- [ ] Voice and video calls work (using shared WebRTC from 3.1)
- [ ] Can create "fake servers" to group Matrix rooms not in Spaces
- [ ] Federation works (join rooms across homeservers via alias)
- [ ] Notifications work for DMs and mentions
