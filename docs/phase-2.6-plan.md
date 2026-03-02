# Phase 2.6 Plan — Remaining UI Polish & Minor Gaps

> **Status:** ⬜ Not Started  
> **Parent:** [Phase 2 Plan](phase-2-plan.md)  
> **Depends On:** Phase 2.5 ✅  
> **Last Updated:** 2026-03-02

---

## Overview

This phase captures all remaining incomplete items from Phase 2 that are **non-blocking**
for the Poly Server test client (Phase 2.7) but should be addressed before Phase 3.
Items are gathered from the Phase 2, 2.2, and 2.5 audits.

---

## 2.6.1 Demo Client Enhancements

- [ ] **2.6.1.1** Fake event stream — `event_stream()` produces periodic `MessageReceived`, `PresenceChanged`, `TypingStarted` events
- [ ] **2.6.1.2** Wire event stream consumer in ClientManager: dispatch events to ChatData
- [ ] **2.6.1.3** Demo "typing" indicators in chat view ("Alice is typing...")
- [ ] **2.6.1.4** Add image attachments to some demo messages (bundled/placeholder URLs)
- [ ] **2.6.1.5** Add multi-line messages, code blocks, emoji-heavy messages to demo data
- [ ] **2.6.1.6** Add edited messages (`edited: true`) to demo data
- [ ] **2.6.1.7** Add reactions with varied counts to demo messages
- [ ] **2.6.1.8** Spread demo message timestamps across several days (trigger date separators)
- [ ] **2.6.1.9** Messages in more demo channels (not just #general)

---

## 2.6.2 Chat View Gaps

- [ ] **2.6.2.1** Scroll-up pagination — detect near-top, trigger `load_more_messages()` with `before` cursor
- [ ] **2.6.2.2** Typing indicator component — "User is typing..." bar above message input
- [ ] **2.6.2.3** File upload button wiring — open file picker, create pending attachment
- [ ] **2.6.2.4** Drag-and-drop: finish `ondrop` handler (parse file data, create `PendingFile`)
- [ ] **2.6.2.5** Emoji search — filter emoji by name when text entered in search input

---

## 2.6.3 User Sidebar Gaps

- [ ] **2.6.3.1** User profile popup on click (modal with avatar, name, status, mutual servers)
- [ ] **2.6.3.2** Role display on user entries (when backend provides roles)

---

## 2.6.4 Notifications View Gaps

- [ ] **2.6.4.1** Wire "Mark as Read" button onclick handler
- [ ] **2.6.4.2** "Mark All as Read" button at top of notification list
- [ ] **2.6.4.3** Filter notifications by backend/account

---

## 2.6.5 Server Sidebar Gaps

- [ ] **2.6.5.1** Server icon with account badge overlay (bottom-right: account avatar)
- [ ] **2.6.5.2** "Add Server to Favorites" action / browse available servers

---

## 2.6.6 Friends Panel Gaps

- [ ] **2.6.6.1** Populate account/backend filter dropdown with real data
- [ ] **2.6.6.2** Populate server filter dropdown with real data
- [ ] **2.6.6.3** Mutual servers display on friend cards
- [ ] **2.6.6.4** Friend request notifications in DMs view

---

## 2.6.7 Settings Gaps

- [ ] **2.6.7.1** Accounts section: per-account view (server browser, favorites, friend list)
- [ ] **2.6.7.2** Persist notification settings to storage
- [ ] **2.6.7.3** Persist voice/video settings to storage
- [ ] **2.6.7.4** Wire account bar to real session data (currently hardcoded "Demo User")

---

## 2.6.8 Channel List Polish

- [ ] **2.6.8.1** Category collapsing toggle (chevron click toggles collapsed state)
- [ ] **2.6.8.2** Category collapse state persistence

---

## 2.6.9 i18n Remaining Keys

- [ ] **2.6.9.1** Audit all untranslated strings and add missing keys to all 4 locales
- [ ] **2.6.9.2** Chat-specific keys from phase 2.5.13 that are still missing
- [ ] **2.6.9.3** User sidebar group headers i18n

---

## 2.6.10 Electron Wrapper

- [ ] **2.6.10.1** Set up Electron wrapper project (`apps/desktop-electron/`)
- [ ] **2.6.10.2** Build script: compile web target, bundle with Electron
- [ ] **2.6.10.3** Test on Linux

---

## 2.6.11 Mobile Layout (Deferred)

- [ ] **2.6.11.1** Mobile layout: 3 swipeable panels
- [ ] **2.6.11.2** Responsive breakpoints for tablet/phone
- [ ] **2.6.11.3** Touch-friendly interaction targets

---

## 2.6.12 Poly Server Minor TODOs

These are small fixes in the existing `poly-server` code before the full client integration:

- [ ] **2.6.12.1** Fix `DELETE /servers/{id}` cascade delete (channels, memberships, messages)
- [ ] **2.6.12.2** Wire `TypingStart` client→server broadcast to channel members
- [ ] **2.6.12.3** Wire `Heartbeat` to update `device.last_seen` in DB
- [ ] **2.6.12.4** Wire `VoiceSignal` relay to target peer
- [ ] **2.6.12.5** Emit missing `ServerEvent` variants: `TypingStart`, `PresenceUpdate`, `ServerMemberJoined/Left`, `ChannelCreated/Deleted`, `Ping`
- [ ] **2.6.12.6** Fix integration test URL mismatch (`/servers/{id}/invites` → `/servers/{id}/invite`, `/invites/{code}/use` → `/servers/join/{code}`)

---

## Completion Criteria

- [ ] Demo client event stream produces periodic fake events
- [ ] Typing indicators render in chat view
- [ ] Category collapsing works
- [ ] Emoji search filters correctly
- [ ] All poly-server minor TODOs resolved
- [ ] All integration tests pass for poly-server
- [ ] `cargo cranky --workspace` — zero warnings
