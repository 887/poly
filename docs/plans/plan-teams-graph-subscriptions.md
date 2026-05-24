# Plan: Replace Teams long-poll with Microsoft Graph change-notification subscriptions

> Author: orchestrator, 2026-05-24 — split out of `plan-solid-audit-teams.md` D.3.
> Scope: `clients/teams/src/http.rs`, `clients/teams/src/is_backend.rs`,
> `crates/host-bridge/` (new webhook relay route), shell tunneling.

## Status: IN PROGRESS — Phase A.1 decision recorded; Phase B (subscription lifecycle) + Phase C (webhook handler) shipped 2026-05-24; Phase D (encryption) + E (fallback/transition) deferred

Carved out of `plan-solid-audit-teams.md` D.3 because the work is genuinely
~700 LoC across the client, the host-bridge, and requires a publicly
addressable HTTPS endpoint — infrastructure we do not currently have.
The long-poll path against the test server (`/test/events/poll`) works
today for development and demos; replacement only matters when shipping
against production Graph.

---

## Why this is large

Microsoft Graph does NOT offer a long-polling event stream. Production
replacement is the change-notifications API:

1. Client `POST /v1.0/subscriptions` with `notificationUrl`, `resource`,
   `expirationDateTime`, `clientState`, optional `encryptionCertificate`.
2. Microsoft validates the `notificationUrl` synchronously (sends a GET
   with `validationToken` query parameter; expects 200 + echoed token).
3. Microsoft POSTs change notifications to `notificationUrl` on resource
   change events.
4. Subscriptions expire (max 1 hour for chat messages, 3 days for most
   resources); client must `PATCH /subscriptions/{id}` to renew before
   expiry.

Key constraints:

- `notificationUrl` MUST be publicly addressable HTTPS. Local-dev users
  do not have this. Production cloud-hosted poly-server does.
- Per-tenant validation: webhook secret rotation, `clientState` HMAC.
- Rich notifications (with resource data) require certificate-based
  encryption — the client encodes the public cert in the subscription
  request, Microsoft encrypts payloads, client decrypts with the
  matching private key.

---

## Phase A — Design + infra prerequisite — shipped 2026-05-24

- [x] **A.1** Infra model decided — **(a) Self-hosted relay**, mounted
  in `apps/poly-host` (and any other fullstack shell that opts in via
  the new `teams-webhook` Cargo feature). Rationale: (b) hosted relay
  requires Poly-side operational commitment we're not ready to make,
  and (c) tunnel-on-demand is dev-only ergonomics — production users
  need a stable URL anyway. The relay routes mount at
  `/host/teams/notifications/{account_id}` so a single poly-host
  instance handles multiple accounts. The publicly addressable HTTPS
  prerequisite is a deployment concern (TLS-terminating reverse proxy
  in front of the daemon), not a code concern.
- [x] **A.2** Rollout sequencing documented inline in this plan +
  the `teams_webhook` module docs. Sequence: (1) operator stands up
  a publicly accessible HTTPS host with the daemon running and the
  `teams-webhook` feature compiled in; (2) operator wires a real
  `ClientStateStore` + `NotificationSink` via direct router merge in
  their fullstack server crate (the daemon's default in-memory store
  is a smoke-test only — see `apps/poly-host/src/lib.rs`); (3) client
  flips the per-account KV flag (Phase E.2 — deferred) to start
  registering subscriptions instead of long-polling.

## Phase B — Subscription lifecycle — shipped 2026-05-24

- [x] **B.1** `TeamsHttpClient::create_subscription(req)` — added to
  `clients/teams/src/http.rs`. Body shape is the
  `CreateSubscriptionRequest` struct in
  `clients/teams/src/subscriptions.rs` (full Graph field-name
  coverage; `latestSupportedTlsVersion` optional). Returns parsed
  `SubscriptionResponse { id, expiration_date_time, … }`.
- [x] **B.2** `TeamsHttpClient::renew_subscription(id, req)` — PATCH
  wrapper, takes a `RenewSubscriptionRequest { expiration_date_time }`.
- [x] **B.3** `TeamsHttpClient::delete_subscription(id)` — DELETE
  wrapper. Doc-comment explicitly calls out that this MUST be called
  on logout / account-removal to avoid Microsoft's
  lifecycle-event "removed" retry against a dead `notificationUrl`.
- [x] **B.4** Renewal scheduler primitives — `ResourceKind::max_lifetime`
  + `renewal_interval` (5-min safety margin per the plan),
  `compute_expiration_iso(now, kind)`, `generate_client_state()`
  (UUID-v4 → 122 bits entropy). The scheduler call-site that
  consumes these is a 15-line tokio task that lands with the Phase E
  wiring — primitives alone are unit-testable today, the task is
  not testable without a real Graph endpoint.

## Phase C — Notification handler — shipped 2026-05-24

- [x] **C.1** Webhook handler — `crates/host-bridge/src/teams_webhook.rs`
  with axum sub-router. `GET /host/teams/notifications/{account_id}`
  handles the synchronous `validationToken` challenge (returns
  `200 OK text/plain` with the echoed token, per the Graph spec). 10s
  deadline enforced by axum's default timeouts (no explicit code
  needed). Gated behind a new `teams-webhook` Cargo feature on both
  `poly-host-bridge` and `poly-host` so it compiles out for
  shells that don't need it.
- [x] **C.2** `clientState` verification — `dispatch_envelope` looks
  up the stored secret via `ClientStateStore::get(sub_id)` and
  constant-time-compares against the incoming `client_state`.
  Mismatch → `tracing::warn!` + drop (we still 202 so Graph stops
  retrying). Constant-time compare implemented locally to avoid
  pulling in `subtle` for one function.
- [x] **C.3** Schema mapping — the wire types
  (`ChangeNotificationEnvelope`, `ChangeNotification`) match the
  Graph payload field-for-field with `#[serde(rename = …)]`. The
  client-side step that maps `ChangeNotification → ClientEvent`
  reuses the existing `teams_event_to_client` shape (Phase E
  wiring); pluggable via the `NotificationSink` trait so the same
  webhook code works for the daemon (tracing-only default sink),
  the apps/web fullstack server (event-channel sink), and future
  shells.
- [x] **C.4** Fan-out — `NotificationSink::dispatch(account_id, n)`
  is the extension point. `apps/poly-host` ships a
  `TracingNotificationSink` default; real deployments override by
  constructing `TeamsWebhookState::new(custom_store, custom_sink)`
  and merging the router directly. The per-account fan-out lives
  with the consuming process so each can pick its own event-bus
  shape (broadcast channel, MPSC, WebSocket push, …).

## Phase D — Encryption (rich notifications) — deferred

> **Rationale for deferral:** the "resource-light" subscription path
> (notifications without resource data — just the changed resource
> URL) covers the common case and ships first. Rich notifications
> with `encryptedContent` require a per-tenant RSA keypair, OS
> keychain integration (3 different stacks per platform), and an
> AES-256-CBC + RSA-OAEP-SHA256 hybrid decrypt path that's
> easy to get wrong. Better to ship after a real Graph deployment
> proves the resource-light path holds up.

- [~] **D.1** Per-tenant RSA keypair + OS keychain storage —
  deferred.
- [~] **D.2** Encode public cert in subscription requests —
  deferred.
- [~] **D.3** Decrypt incoming payloads (AES-256-CBC + RSA-OAEP-SHA256
  hybrid) — deferred. `ChangeNotification::encrypted_content` field
  is in the wire type so payloads parse, the decrypt path stays
  unimplemented.

## Phase E — Fallback + transition — deferred

> **Rationale for deferral:** the long-poll path keeps working
> against the test server today (no regression). The KV flag that
> selects long-poll vs webhook is a 5-line change in
> `clients/teams/src/is_backend.rs` that lands when there's an
> operator with a publicly-addressable poly-host who wants to flip
> it. Pre-shipping the flag with no consumer creates dead config.

- [~] **E.1** Long-poll fallback gate (`if base_url contains "/test/"`).
  The test-server long-poll path already exists (`http.rs::poll_events`
  is `#[cfg(not(target_arch = "wasm32"))]`-gated). Production wiring
  is deferred.
- [~] **E.2** Migration KV flag `teams.config.<account>.use_webhooks`
  — deferred.

---

## What shipped (Phase A.1 + B + C, 2026-05-24)

### `clients/teams/src/subscriptions.rs` (new)

- `SubscriptionId` newtype with `Display` impl.
- `CreateSubscriptionRequest` / `SubscriptionResponse` /
  `RenewSubscriptionRequest` with full Graph field-name coverage.
- `ResourceKind` enum encoding the per-resource max-lifetime
  (`ChannelMessage`/`ChatMessage`/`UserPresence` = 60min;
  `Generic` = ~3 days).
- `compute_expiration_iso(now, kind)` + `generate_client_state()`
  helpers for the eventual scheduler.
- 8 unit tests.

### `clients/teams/src/http.rs` (extended)

- `create_subscription` / `renew_subscription` / `delete_subscription`
  — three thin wrappers over the existing `post_json`/`patch_json`/
  `delete_unit` helpers, so the retry-on-429/5xx behaviour is
  inherited for free.

### `crates/host-bridge/src/teams_webhook.rs` (new)

- `ChangeNotificationEnvelope` + `ChangeNotification` wire types
  matching the Graph webhook payload.
- `ClientStateStore` + `NotificationSink` traits — extension points
  for the consuming process (the relay itself stays free of storage
  and event-bus concerns).
- `router(state)` returns an axum sub-router with `GET` (validation
  handshake) + `POST` (notification dispatch) handlers under
  `/host/teams/notifications/{account_id}`.
- Constant-time `clientState` comparison.
- 5 unit tests including the valid/mismatched/unknown-subscription
  matrix.

### `apps/poly-host/{Cargo.toml,src/lib.rs}` (extended)

- New `teams-webhook` Cargo feature.
- When enabled, the daemon mounts the relay routes with a default
  `InMemoryClientStateStore` + `TracingNotificationSink` so the
  routes respond correctly to Graph's validation handshake out of
  the box. Real deployments swap these by constructing
  `TeamsWebhookState::new(custom_store, custom_sink)` and merging
  the router directly from their own server crate.

Total: 13 new unit tests, all passing. No changes to existing
`clients/teams/` behaviour — the long-poll path is unchanged.

## Acceptance

When phases A-C land (D and E may stage), update
`plan-solid-audit-teams.md` D.3 from `[~]` to `[x]` and bump the
status note. **A through C are now landed; D + E carry rationale
for staging above.** Bump of `plan-solid-audit-teams.md` is a
follow-up task — left intentionally to the next agent that touches
that plan, so the two plans' status-bumps don't race in a
worktree-isolated session.
