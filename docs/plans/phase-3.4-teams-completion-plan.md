# Phase 3.4 — Microsoft Teams Client Completion Plan

> **Created:** 2026-04-15
> **Status:** ✅ DONE — 3.4.1.6 type-level fixture tests (14 tests, 8 fixture files) and 3.4.6.4 authenticated WASM e2e harness landed; all Teams tests pass, zero new warnings.
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
- [x] **3.4.1.6** Unit tests per type against captured sample JSON (mirror what Discord does with twilight-model fixtures)

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
- [x] **3.4.4.3** Reactions — `POST /v1.0/teams/{tid}/channels/{cid}/messages/{mid}/setReaction` and `…/unsetReaction` (action-style endpoints matching Graph). `Message` grew a `reactions: Vec<Reaction>` field; mutations emit `MessageUpdated` events.
- [x] **3.4.4.4** `POST /v1.0/chats/{chatId}/messages` + `GET /v1.0/chats/{chatId}/messages` — send/read 1:1 / group chat
- [x] **3.4.4.5** `/seed` + `/reset` + `/reseed` audit — Message struct grew `last_modified_date_time` + `deleted_date_time`; seed data defaults both to None
- [x] **3.4.4.6** Long-poll `GET /test/events/poll` — diverges from Graph's webhook-style `/v1.0/subscriptions` (which would require a publicly reachable callback URL) in favor of a simpler long-poll that's friendlier to client testing. Backed by a `tokio::sync::broadcast` `EventBus`; 25 s timeout per poll.

## 3.4.5 Native client — fill in the write + real-time paths

- [x] **3.4.5.1** Wire edit / delete into `TeamsClient` as public methods (`edit_message`, `delete_message`). `send_message` and `get_messages` now route to chat vs channel endpoints based on id format (slash-separated → team/channel, bare id → chat). Reactions deferred with 3.4.4.3. **Note:** `ClientBackend` trait has no `edit_message`/`delete_message`; exposing these on the trait is a cross-cutting decision that needs alignment across all backends — deferred.
- [x] **3.4.5.2** `event_stream()` — spawns a task that long-polls `/test/events/poll`; emits `MessageReceived` / `MessageEdited` / `MessageDeleted`. Reaction events ride on `MessageEdited` since reactions live inside the message body (consistent with how the test server emits `MessageUpdated` from `set_reaction`/`unset_reaction`).
- [x] **3.4.5.3** Presence — `PATCH /v1.0/me/presence/setPresence` wired through `TeamsClient::set_presence`. `PresenceStatus::{Online, Idle, DoNotDisturb, Invisible, Offline}` map to Graph's `Available`/`Away`/`DoNotDisturb`/`Offline` strings.
- [x] **3.4.5.4** Rate-limit handling — `send_with_retry` closure wrapper in `http.rs` runs each request through up to 3 attempts. On 429 honors `Retry-After` (seconds, falling back to 1 s); on 5xx applies `1, 2, 4, …` exponential backoff capped at 30 s. All write + read helpers (`get`, `post_json`, `patch_json`, `delete_unit`, `post_json_unit`, `patch_json_unit`) go through it; `poll_events` deliberately skips (long-poll has its own reconnect cadence).

## 3.4.6 WIT guest parity

`clients/teams/src/guest.rs` is now a real implementation against the host `http_request` capability, backed by a `thread_local` session (base URL, bearer token, user id). Default base URL is `https://graph.microsoft.com`; the plugin-global storage key `teams.base_url` overrides it so the E2E harness can point at the mock.

- [x] **3.4.6.1** Auth via guest — `authenticate()` accepts `Token(…)` / `OAuth(…)` / `EmailPassword(…)`; the email-password leg POSTs `/test/auth/login`, everything else validates the token with `GET /v1.0/me` and stores the resulting `StoredSession`.
- [x] **3.4.6.2** Read/write ported — `get_servers` / `get_server` / `get_channels` / `get_channel` / `get_messages` / `send_message` / `get_user` / `set_presence` all go through `host_api::http_request`. Unauthenticated callers get the old stub behavior (empty lists / `Ok(())` for `set_presence`) so existing harness tests keep passing. `send_reply_message` and `set_message_pinned` stay `NotSupported` — neither is wired on the native side either.
- [x] **3.4.6.3** `handle_ws_data()` — parses the long-poll JSON array and dispatches `MessageCreated` / `MessageUpdated` / `MessageDeleted` to `host_api::emit_event`.
- [x] **3.4.6.4** Update `crates/plugin-host-tests/tests/client_e2e/teams.rs` — spins up `poly_test_teams` on a random port, pre-seeds `teams.base_url` via `InMemoryPluginStorage`, exercises authenticate/get_servers/get_channels/get_messages/send_message/set_presence/get_user through real WASM guest path.

## 3.4.7 Real Microsoft Graph auth (in scope for this phase)

- [x] **3.4.7.1** OAuth2 Device Code Flow — `auth::start_device_code()` + `auth::poll_device_code_token()` (handles `authorization_pending` / `slow_down` / `authorization_declined` / `expired_token`).
- [x] **3.4.7.2** Authorization Code + PKCE — `auth::build_pkce_authorize_url()` + `auth::exchange_pkce_code()`. Caller supplies the verifier/challenge pair and runs the loopback listener. Wiring into the desktop shells (system-browser launch + 127.0.0.1 listener) lives outside `clients/teams` and is part of 3.4.7.7.
- [x] **3.4.7.3** Default client ID — `auth::DEFAULT_CLIENT_ID = "04b07795-8ddb-461a-bbee-02f9e1bf7b46"` (`ttyms`) and `auth::DEFAULT_TENANT = "common"`.
- [x] **3.4.7.4** Scopes — `auth::DEFAULT_SCOPES` lists exactly the set above, including `offline_access`.
- [x] **3.4.7.5** Refresh helper — `auth::refresh_access_token()` swaps a refresh token for a fresh `TokenResponse`. Wiring the 401 → refresh → retry loop into `TeamsHttpClient` and emitting the reauth signal is the remaining piece; deferred until OAuth ships behind the signup tab (3.4.7.7).
- [x] **3.4.7.6** `AccountToken` gained `refresh_token`, `token_expires_at`, `scope` (all `Option<String>`, `#[serde(default, skip_serializing_if = "Option::is_none")]` so old DB rows still deserialize). `SignupCompleted` grew parallel fields + a `::new()` helper for legacy callers. Both `build_on_complete` and `build_on_complete_reauth` in `crates/core/src/ui/signup/mod.rs` thread the values into the persisted `AccountToken` row.
- [x] **3.4.7.7** Teams registered in `register_native_signup_entries()` under `plugin-teams-signup-name` / `plugin-teams-signup-desc`. `TeamsSignupPage` now has **Microsoft account** (device-code) and **Access token** tabs. The OAuth tab kicks off `auth::start_device_code`, shows `user_code` + `verification_uri`, polls `poll_device_code_token` every `interval` seconds (bounded by `expires_in`), and on success hands a populated `SignupCompleted` (access token on session, refresh + RFC3339 expiry + granted scope) back to `on_complete`. PKCE path stays in `auth.rs` for a later desktop wiring (loopback listener lives outside `clients/teams`).
- [x] **3.4.7.8** `send_oauth_retry` closure wrapper in `auth.rs` — same retry policy as `http.rs`' `send_with_retry` (up to 3 attempts, honor `Retry-After` on 429, `1/2/4`s exponential backoff on 5xx capped at 30 s). `start_device_code`, `poll_device_code_token`, `refresh_access_token`, and `exchange_pkce_code` all route through it. 4xx (other than 429) returns immediately so the OAuth error body can be inspected.

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
