# Plan: Replace Teams long-poll with Microsoft Graph change-notification subscriptions

> Author: orchestrator, 2026-05-24 — split out of `plan-solid-audit-teams.md` D.3.
> Scope: `clients/teams/src/http.rs`, `clients/teams/src/is_backend.rs`,
> `crates/host-bridge/` (new webhook relay route), shell tunneling.

## Status: NOT STARTED — design only

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

## Phase A — Design + infra prerequisite

- [ ] **A.1** Decide infra model. Three options:
  - **(a) Self-hosted relay.** Customer runs a publicly accessible
    poly-server that mounts `/teams/webhook/{account_id}` and proxies
    notifications back to per-account event channels. Requires deployment
    + TLS cert + DNS.
  - **(b) Hosted relay.** Poly project runs a multi-tenant relay
    (`webhook.poly.app`); clients subscribe with that URL, relay fans out
    to authenticated client connections (WebSocket / SSE). Requires
    Poly-side operational commitment.
  - **(c) Tunnel-on-demand.** Local-dev users get a tunnel (ngrok-style)
    spawned by the desktop shell. Production users use (a) or (b).
- [ ] **A.2** Document the chosen model + rollout sequencing in this plan.

## Phase B — Subscription lifecycle

- [ ] **B.1** `TeamsHttpClient::create_subscription(resource, expiry, client_state)`
  → returns `SubscriptionId`.
- [ ] **B.2** `TeamsHttpClient::renew_subscription(id, new_expiry)`.
- [ ] **B.3** `TeamsHttpClient::delete_subscription(id)` — call on logout
  to avoid orphan subscriptions hitting a dead `notificationUrl` and
  triggering Microsoft's lifecycle-event "removed" message.
- [ ] **B.4** Renewal scheduler — fire at `expiry - 5min` with jitter.

## Phase C — Notification handler

- [ ] **C.1** Webhook handler in `crates/host-bridge/src/teams_webhook.rs`
  (or wherever the chosen relay lives). Validate the `validationToken`
  challenge synchronously on first POST.
- [ ] **C.2** HMAC-verify each notification against the stored
  `clientState`.
- [ ] **C.3** Map Graph notification payload → `ClientEvent` (reuse
  `teams_event_to_client` after schema normalization — production Graph
  payloads differ from the test-server's `TeamsEvent` shape).
- [ ] **C.4** Fan out to per-account event channels.

## Phase D — Encryption (rich notifications)

- [ ] **D.1** Generate per-tenant RSA keypair, store private key in OS
  keychain (Linux: secret-service; macOS: Keychain; Windows: DPAPI).
- [ ] **D.2** Encode public cert in subscription requests.
- [ ] **D.3** Decrypt incoming payloads (AES-256-CBC + RSA-OAEP-SHA256
  hybrid; spec at https://learn.microsoft.com/graph/webhooks-with-resource-data).

## Phase E — Fallback + transition

- [ ] **E.1** Keep the long-poll path live for the test-server backend
  (gated on `if base_url contains "/test/"`). Webhooks only fire against
  real Graph.
- [ ] **E.2** Migration KV flag `teams.config.<account>.use_webhooks`
  defaulting to `false` until Phase A.1 infrastructure ships.

---

## Acceptance

When phases A-C land (D and E may stage), update
`plan-solid-audit-teams.md` D.3 from `[~]` to `[x]` and bump the status
note.
