# Phase 3.4 — Microsoft Teams Client Completion Plan

> **Created:** 2026-04-15
> **Status:** 🟡 α (native backend working against mock server; not wired into signup picker)
> **Crate:** `poly-teams`
> **Supersedes:** `docs/archive/phases/phase-3.4-teams-plan.md` (that plan assumed greenfield; this one picks up from where the α implementation actually stands)
> **Goal:** Bring Teams to Discord-level parity — typed API layer, EmailPassword test flow, signup-picker entry, WIT guest on par with native.

---

## Current state (from repo audit, 2026-04-15)

`clients/teams/` is **not** a stub — it has a full `ClientBackend` impl against the mock server. Gaps vs. `clients/discord/`:

| Area | Discord | Teams |
|------|---------|-------|
| Typed API layer | `twilight-model` workspace dep | Custom `api.rs` (70 LOC, subset of Graph) |
| Test server auth | `/api/v10/auth/login` (EmailPassword) + `/test/auth/token` | Only `/test/auth/token` (Bearer) |
| Signup picker entry | Registered in `register_native_signup_entries()` | **MISSING** |
| Test accounts | Koala / Kangaroo (EmailPassword) | Sheep / Walrus (Token) |
| Settings page | Registered | Registered ✓ |
| Locales | Full FTL strings | 2-line stub |
| Real OAuth2 | N/A (chat-server auth) | Not implemented (Device Code + PKCE TBD) |
| WIT guest | Partial | Stub returning errors |

**Decision (from user, 2026-04-15):** We roll our own types under `clients/teams/src/types/` rather than pulling in `graph-rs-sdk` or `microsoft-graph-rs`. The Graph surface we actually touch is small, and Graph's official SDK is heavyweight/enterprise-leaning. Keep the custom types, grow them as features land.

---

## 3.4.1 Typed API layer (parity with twilight-model in Discord)

Lift `clients/teams/src/api.rs` into a proper `types/` module so message / team / channel / chat / user types live in one place and http handlers deserialize straight into them.

- [x] **3.4.1.1** Split `src/api.rs` into `src/types/{mod.rs,user.rs,team.rs,channel.rs,message.rs,chat.rs}`
- [x] **3.4.1.2** Match Graph v1.0 field names (`id`, `displayName`, `userPrincipalName`, `chatType`, `messageType`, etc.) with `#[serde(rename)]` where Rust style diverges
- [x] **3.4.1.3** Add `ODataResponse<T> { value: Vec<T>, @odata.nextLink: Option<String> }` for list pagination — every Graph list endpoint wraps in this
- [x] **3.4.1.4** Add `GraphError { error: { code, message } }` shape and a `From<GraphError> for ClientError` mapper
- [x] **3.4.1.5** Port `http.rs` to parse into typed structs, not `serde_json::Value`
- [ ] **3.4.1.6** Unit tests per type against captured sample JSON (mirror what Discord does with twilight-model fixtures)

## 3.4.2 Wire Teams into signup picker — **N/A, matches Discord**

Discord is NOT registered in `register_native_signup_entries()` either — both are test-account-only plugins (visible as "Add Test Account" entries rather than a manual signup page). Keep parity: no signup entry for Teams.

- [x] **3.4.2.1** ~~Register in signup picker~~ → N/A; Teams matches Discord pattern (test-account-only)
- [x] **3.4.2.2** ~~Extend locales for signup panel~~ → N/A for now; the signup panel isn't reachable without a picker entry. Revisit when 3.4.7 lands a real OAuth tab.
- [x] **3.4.2.3** ~~Feature-gate signup entry behind `dev-plugins`~~ → N/A; test accounts already gated via `dev-plugins` on `register_native_test_accounts`

## 3.4.3 EmailPassword test flow (parity with Discord)

- [x] **3.4.3.1** `servers/test-teams/src/routes.rs` — add `POST /test/auth/login { login, password } → { token, user_id }` that validates against the seeded Sheep/Walrus accounts and returns a Bearer token. Uses the same `state.auth.create_token` path `/test/auth/token` issues.
- [x] **3.4.3.2** `clients/teams/src/lib.rs` — accept `AuthCredentials::EmailPassword { email, password }`. On that variant: POST `/test/auth/login`, receive token, continue as Bearer flow. Token flow stays as-is.
- [x] **3.4.3.3** `clients/teams/src/signup.rs` — swap Sheep/Walrus `TestAccountEntry` to EmailPassword (mirror what Discord did for Koala/Kangaroo in phase 2.5)
- [ ] **3.4.3.4** ~~Signup panel tabs~~ → deferred with 3.4.2. Signup panel isn't reachable until Teams is registered in the signup picker (which in turn waits for 3.4.7 OAuth to give the manual form a reason to exist).

## 3.4.4 Extend test-teams to match test-discord's surface

`servers/test-teams/src/routes.rs` currently covers list/read. Fill in the write side so UI flows don't half-work.

- [x] **3.4.4.1** `PATCH /v1.0/teams/{tid}/channels/{cid}/messages/{mid}` — edit message (author-only, rejects if `deletedDateTime` set)
- [x] **3.4.4.2** `DELETE /v1.0/teams/{tid}/channels/{cid}/messages/{mid}` — soft-delete (sets `deletedDateTime`, clears `body.content`, row stays)
- [ ] **3.4.4.3** Reactions — deferred; `setReaction`/`unsetReaction` pair with event-stream (3.4.5.2), handle together
- [x] **3.4.4.4** `POST /v1.0/chats/{chatId}/messages` + `GET /v1.0/chats/{chatId}/messages` — send/read 1:1 / group chat
- [x] **3.4.4.5** `/seed` + `/reset` + `/reseed` audit — Message struct grew `last_modified_date_time` + `deleted_date_time`; seed data defaults both to None
- [ ] **3.4.4.6** `GET /v1.0/subscriptions` mock — deferred; pairs with 3.4.5.2 event-stream

## 3.4.5 Native client — fill in the write + real-time paths

- [x] **3.4.5.1** Wire edit / delete into `TeamsClient` as public methods (`edit_message`, `delete_message`). `send_message` and `get_messages` now route to chat vs channel endpoints based on id format (slash-separated → team/channel, bare id → chat). Reactions deferred with 3.4.4.3. **Note:** `ClientBackend` trait has no `edit_message`/`delete_message`; exposing these on the trait is a cross-cutting decision that needs alignment across all backends — deferred.
- [ ] **3.4.5.2** `event_stream()` — poll the mock subscription / long-poll endpoint and emit `MessageReceived`, `MessageEdited`, `MessageDeleted`, `ReactionAdded`
- [ ] **3.4.5.3** Presence — stub `set_presence` against `PATCH /v1.0/me/presence/setPresence` in the mock
- [ ] **3.4.5.4** Rate-limit handling — on 429 from Graph, honor `Retry-After`; no-op against the mock but keep the wiring so real Graph calls work

## 3.4.6 WIT guest parity

Teams `guest.rs` is still a stub returning errors. Once the native write paths above land, port them.

- [ ] **3.4.6.1** Auth via guest — `host_api::http_request()` to `/test/auth/login` (or accept pre-issued token)
- [ ] **3.4.6.2** Port list / read / send / edit / delete / react to the guest
- [ ] **3.4.6.3** `handle_ws_data()` — parse the poll-response frame and call `emit-event`
- [ ] **3.4.6.4** Update `crates/plugin-host-tests/tests/client_e2e/teams.rs` — flip the 10 "stub returns error" assertions to real behavior checks

## 3.4.7 Real Microsoft Graph auth (in scope for this phase)

- [ ] **3.4.7.1** OAuth2 Device Code Flow against `login.microsoftonline.com/common/oauth2/v2.0/devicecode` (headless / terminal)
- [ ] **3.4.7.2** Authorization Code + PKCE against `/oauth2/v2.0/authorize` → `/oauth2/v2.0/token` — open system browser from the desktop shells, loopback-redirect to a one-shot localhost listener
- [ ] **3.4.7.3** Azure AD app registration — reuse the `ttyms` default client ID (`04b07795-8ddb-461a-bbee-02f9e1bf7b46` or whatever it ships with) for day one; capture the value in `clients/teams/src/auth.rs` as a const, switch to a Poly-owned registration later
- [ ] **3.4.7.4** Scopes (minimal, delegated): `User.Read`, `Team.ReadBasic.All`, `Channel.ReadBasic.All`, `ChannelMessage.Read.All`, `ChannelMessage.Send`, `Chat.Read`, `Chat.ReadWrite`, `Presence.Read`, `offline_access` (for refresh tokens)
- [ ] **3.4.7.5** Refresh-token rotation + silent re-auth — store refresh-token alongside access-token in `AccountToken`; when a 401 comes back from Graph, refresh once before surfacing the error; wire the reauth-needed signal into the per-account reauth UI (phase 2.5 landed that surface)
- [ ] **3.4.7.6** Token storage via the existing `AccountToken` / host-bridge KV, encrypted for backup — extend the record to include `refresh_token`, `expires_at`, `scope`
- [ ] **3.4.7.7** Teams signup panel — add a "Microsoft account (real)" tab alongside the test-server tab, showing the device-code URL + user-code when that flow kicks off, or kicking off the browser for the PKCE flow
- [ ] **3.4.7.8** Rate-limit + throttling handling — honor `Retry-After` from Graph, exponential backoff on 5xx

---

## Completion criteria for this phase

- [ ] Teams shows up in the signup picker (behind `dev-plugins`)
- [ ] Sheep / Walrus log in via email+password against `test-teams`
- [ ] Real Microsoft accounts log in via Device Code Flow (terminal-friendly) and PKCE (desktop browser)
- [ ] Refresh-token silent re-auth works; 401 surfaces reauth prompt only when refresh also fails
- [ ] Channel + 1:1 + group chat list, read, send, edit, delete, react all work end-to-end in the UI
- [ ] Event stream delivers new messages without a manual refresh
- [ ] WIT guest E2E tests cover the same operations the native client does
- [ ] `cargo check` clean across all shells with `--all-features`

## Out of scope / explicitly deferred

- Meetings, calling, video (Communications API)
- Presence webhooks (polling is fine for now)
- Any Azure AD admin-consent flows (user-consent only)
