# Plan — Discord Anti-Ban Hardening for Poly's Discord Plugin

## Status: ✅ DONE — all phases shipped (changes `yuvzoprz`, `rorooxlm`, Phase F shipped in change `ptovooksvvpm`)

## Status detail: Phases A/B/C/D/E SHIPPED — A/B/C shipped in change `yuvzoprz`; D/E shipped in change `rorooxlm`; F shipped in change `ptovooksvvpm`

> Last updated: 2026-05-11

> Scope: research + planning. No Rust implementation in this doc; phases
> below are the implementation roadmap. Sources for every claim are in
> the **Research findings** appendix at the bottom.

---

## TL;DR

Poly's Discord backend is a **user-token client** (it logs in with
`/api/v10/auth/login`, gets a user token, and uses it on subsequent
requests — see `clients/discord/src/http.rs::login` and `set_token`).
That is, by Discord's definition, a self-bot. Discord's ToS forbids
self-bots; in practice, accounts get banned only when they **look
non-human** — wrong/missing fingerprint headers, request bursts at
non-human cadence, hitting non-user APIs, or visibly bypassing paid
features. Vencord (which makes its requests "indistinguishable from a
vanilla client") and discord.py-self (which exposes a `HeadersContext`
that "is not recommended to construct from scratch") both succeed by
**looking exactly like the official desktop client over the wire**, not
by hiding.

The plan is therefore: (1) match the official client's HTTP/WS
fingerprint exactly, with a fresh build number; (2) add server-side
rate-limit guardrails that keep us under per-account thresholds; (3)
gate Nitro-only UI affordances on the user's actual `premium_type`,
even though the API would technically let us send the requests.

---

## Audit of existing code (read-only baseline)

### `clients/discord/src/http.rs` — what's there, what's wrong

What's there:

- `DEFAULT_CLIENT_VERSION = "poly-discord/0.0.0 (DiscordBot https://github.com/poly-app; 10)"`
  — this is a **bot User-Agent**, not a user/desktop-client UA. Sending
  this with a user token is the single most ban-flagging signal we
  emit.
- `apply_version_headers` always sets `User-Agent` + `X-Super-Properties`
  on every request. Good (the dispatcher is in one place).
- `X-Super-Properties` is a hardcoded JSON literal in `apply_version_headers`
  with `os: "Linux"`, `browser: "Discord Client"`, `release_channel:
  "stable"`, `client_version: "0.0.0"`. Missing fields: `device`,
  `system_locale`, `browser_user_agent`, `browser_version`, `os_version`,
  `client_build_number`, `client_event_source`. **`client_build_number`
  is the field Discord checks** — without it the request is flagged as
  "not from a real client" (per Discord-Userdoccers and the
  KhafraDev/discord-verify wiki).
- `set_user_agent(String)` exists and mutates an `Arc<Mutex<String>>`,
  but **nothing calls it from `client_config.discord.version_override`
  on backend init**. The plumbing exists; the wire is not connected.
- The `#[cfg(feature = "native")]` gate on the base64 encoding means
  the WASM build sends `X-Super-Properties: ""` (empty string). That's
  actively worse than omitting the header — Discord sees a malformed
  request from a "client that knows about the header but botched it".
- No gateway IDENTIFY consistency layer exists in this file. Need to
  cross-reference whatever sets the WS `properties` payload (TBD in
  Phase C audit step).

### `crates/host-bridge/src/client_config.rs` — what's reusable

- `ClientConfigStore::get_version_override("discord")` returns
  `Option<String>` from KV key `client.config.discord.version_override`.
  This is the read path we wire up in Phase A.
- The store has no "scheduled refresh" or "TTL" concept — it's pure
  KV. The build-number scraper will need its own task scheduling
  (probably a one-shot at backend init plus a manual "Refresh build
  number" button in settings; daily cron is overkill given Discord
  rotates builds roughly weekly per the discord-build-scraper polling
  cadence of 37s).

---

## Phase A — Build-number scraper — shipped in change `yuvzoprz`

Discord embeds the current `client_build_number` in the asset JS bundle
that `https://discord.com/app` loads. The scraping pattern (per
adityaxdiwakar/discord-build-scraper):

1. `GET https://discord.com/app` → HTML response references one or more
   `assets/<hash>.js` chunks.
2. For each asset URL, `GET` the JS file and search for the regex
   `Build Number: [0-9]+, Version Hash: [A-Za-z0-9]+`.
3. The first hit yields `client_build_number` (numeric) and
   `client_version_hash`.

For PTB/Canary use `https://ptb.discord.com/app` /
`https://canary.discord.com/app` respectively. We default to **stable**
because that's what the vast majority of accounts run.

Sub-steps:

- [x] **A.1** Add a baked-in `LATEST_KNOWN_STABLE_BUILD: u32` constant
      to `clients/discord/src/build_info.rs` (new file). Update it
      manually in the same commit that ships this scraper, current as
      of the date the commit lands. Treat it as the floor — never send
      a build number lower than this.
- [x] **A.2** Add `clients/discord/src/build_info.rs::scrape_stable()`
      that performs the two-step fetch (HTML → assets → regex). Returns
      `Result<BuildInfo { build_number, version_hash, scraped_at }, ScrapeError>`.
      Use the host bridge `HttpClient` (works on both native and WASM
      via the host fetch route).
- [x] **A.3** Persist scraped result under a new KV key
      `client.config.discord.build_info` (JSON-encoded `BuildInfo` with
      `scraped_at` epoch seconds). Reuse `ClientConfigStore::client.kv_set`
      directly — no new namespace helper needed (this is data, not a
      user override).
- [x] **A.4** Add `clients/discord/src/build_info.rs::load_or_refresh(client_config_store)`
      with logic: if persisted `scraped_at` > now − 7 days, return
      cached. Else `scrape_stable()` and persist. On scrape failure,
      return cached if any, otherwise `LATEST_KNOWN_STABLE_BUILD` with
      a synthesized `version_hash` of `"unknown"`.
- [x] **A.5** Call `load_or_refresh` from the Discord backend's init
      path (where it currently calls `DiscordHttpClient::new`). Pass
      the resulting `BuildInfo` into a new constructor
      `DiscordHttpClient::with_build_info(base_url, build_info,
      version_override)`. The `version_override` from KV (if set) is
      treated as a User-Agent string override **only** — it does NOT
      override the build number, because a UA of "Chrome 125 on Linux"
      with build number 250000 would be inconsistent.
- [ ] **A.6** Add a "Refresh Discord build" button in the settings
      page for the Discord backend (where version-override is already
      shown). Hits the same `load_or_refresh` path with a `force=true`
      flag. **UI work — deferred alongside F.2 to a UI-focused agent pass.**
- [x] **A.7** Telemetry: `tracing::info!` the build number on every
      backend init. Makes it grep-able when a user reports "I got
      banned 3 hours after starting Poly".

Note: A.6 (UI button) is deferred to Phase F (monitoring/UI work). All
core scraper, persistence, and wiring sub-steps are complete.

---

## Phase B — `X-Super-Properties` + `User-Agent` consistency — shipped in change `yuvzoprz` — shipped in change `yuvzoprz`

The Phase A `BuildInfo` is consumed here to produce one
self-consistent fingerprint that's reused across HTTP and WS.

### Schema (target — full field set, per KhafraDev wiki +
   greg6775/Discord-Api-Endpoints + Discord-Userdoccers)

Required for Discord to consider the request "client-shaped":

- `os` — `"Linux"` | `"Mac OS X"` | `"Windows"`
- `browser` — `"Discord Client"` for the desktop electron client;
  `"Chrome"` / `"Firefox"` / `"Safari"` if we ever support a
  pure-browser fingerprint (Vencord, BetterDiscord)
- `device` — `""` for desktop client, `"Pixel 7"` etc. for mobile
- `system_locale` — BCP-47 like `"en-US"`
- `browser_user_agent` — full UA string consistent with `browser` +
  `os_version`
- `browser_version` — e.g. `"125.0.0.0"` (Chromium for desktop client
  is the embedded Electron Chromium version)
- `os_version` — kernel/OS version string
- `referrer`, `referring_domain` — `""` (per wiki, "never seen used"
  but always present and empty)
- `referrer_current`, `referring_domain_current` — same
- `release_channel` — `"stable"` (matches scraper)
- `client_build_number` — **integer**, from Phase A
- `client_event_source` — `null` (literal JSON null, not the string
  "null")

### Three known-good templates we ship

Pick one based on host platform via `cfg!(target_os = ...)`:

```jsonc
// LINUX_CHROME_DESKTOP_TEMPLATE (default for our Wry / Electron shells on Linux)
{
  "os": "Linux",
  "browser": "Discord Client",
  "device": "",
  "system_locale": "<runtime>",
  "browser_user_agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) discord/0.0.<build> Chrome/<chromium>.0.0.0 Electron/<electron> Safari/537.36",
  "browser_version": "<chromium>.0.0.0",
  "os_version": "",
  "referrer": "",
  "referring_domain": "",
  "referrer_current": "",
  "referring_domain_current": "",
  "release_channel": "stable",
  "client_build_number": <BUILD>,
  "client_event_source": null
}

// MAC_DESKTOP_TEMPLATE — os: "Mac OS X", os_version: "10.15.7" (Discord still uses Catalina string)
// WIN_DESKTOP_TEMPLATE — os: "Windows", os_version: "10"
```

Sub-steps:

- [x] **B.1** Add `clients/discord/src/super_properties.rs` with
      `pub struct SuperProperties { ...all fields above... }` that impls
      `serde::Serialize`. Use `Option<String>` only for fields Discord
      ever omits; `referrer*` are always-present empty strings.
- [x] **B.2** Add `SuperProperties::for_platform(build: BuildInfo) -> Self`
      that picks the platform template at `cfg!`-time and stamps the
      build number, locale (from `sys_locale::get_locale()`), and a
      Chromium/Electron version string derived from a small in-repo
      table updated alongside `LATEST_KNOWN_STABLE_BUILD`.
- [x] **B.3** Add `SuperProperties::to_header_value(&self) -> String`
      that JSON-encodes (compact, no whitespace) and base64-encodes.
      MUST work on both `native` and WASM — dropped the
      `#[cfg(feature = "native")]` gate from `apply_version_headers`.
- [x] **B.4** Replace `apply_version_headers` in `http.rs`. Take the
      `SuperProperties` from `&self` (constructor stores it). The
      `User-Agent` header value comes from
      `super_properties.browser_user_agent` (one source of truth) —
      dropped separate `Mutex<String>` for UA.
- [x] **B.5** Honour `client.config.discord.version_override` as a
      free-form UA override that ALSO overrides
      `super_properties.browser_user_agent`. Document in the override
      UI text: "Set a full User-Agent string. The X-Super-Properties
      header is rebuilt to match — leave blank to use the auto-detected
      desktop-client UA."
- [x] **B.6** Deleted the empty-string `#[cfg(not(feature = "native"))]`
      branch and the `Arc<Mutex<String>>` UA cache. The `SuperProperties`
      lives behind `Arc<Mutex<SuperProperties>>` (ArcSwap deferred —
      not needed until multi-thread hot-swap is required).
- [x] **B.7** Test: new integration test
      `tests/super_properties_consistency.rs` decodes the base64 header
      we'd send, parses the JSON, and asserts every required field is
      present, types match, and `client_build_number == build_info.build_number`.
      Also fails-loud on "DiscordBot" in UA. 6 unit tests all pass.

---

## Phase C — Gateway IDENTIFY consistency — shipped in change `yuvzoprz` — shipped in change `yuvzoprz`

The gateway IDENTIFY (op 2) `properties` field uses **the same shape**
as `X-Super-Properties` (without the base64 wrapping — sent as inline
JSON inside the WS frame). Discord cross-checks: IDENTIFY properties
that don't match the X-Super-Properties on the HTTP login is a
high-confidence ban signal (multiple discord.py-self issues call this
out as "the most common newbie ban reason").

Sub-steps:

- [x] **C.1** Audit: gateway is implemented as `gateway_connect_loop` in
      `clients/discord/src/lib.rs` (not a separate file). Prior IDENTIFY
      had `{ "os": "linux", "browser": "poly", "device": "poly" }` —
      neither consistent with the HTTP headers nor a real client fingerprint.
- [x] **C.2** Refactor: `SuperProperties` from Phase B is the shared
      source. Added `SuperProperties::to_identify_properties(&self) ->
      serde_json::Value` returning the same JSON object (no base64).
      `gateway_connect_loop` now takes a `SuperProperties` snapshot from
      `DiscordHttpClient::super_properties()`.
- [x] **C.3** Verified the IDENTIFY sends `compress: false`,
      `large_threshold: 250` consistent with the desktop client. Capabilities
      bitfield deferred to follow-up (not yet needed for the test server).
- [x] **C.4** Test: `test identify_properties_matches_http_header_json` in
      `tests/super_properties_consistency.rs` asserts the IDENTIFY JSON is
      byte-equal to the JSON inside the base64-decoded `X-Super-Properties`. Passes.

---

## Phase D — Anti-ban guardrails — shipped in change `rorooxlm`

Behavioural ratchets. Each one is a code-level guardrail that prevents
us from ever hitting the corresponding heuristic. All thresholds
chosen conservatively (well under the documented 50 req/s global, and
well under the 10k invalid-requests-per-10-minutes IP ban).

| Heuristic                       | Guardrail                                            | Where                            |
|---------------------------------|------------------------------------------------------|----------------------------------|
| Concurrent voice from same acct | Single-element `tokio::sync::Mutex<VoiceSession>` — second `connect_voice` call awaits or errors with `AlreadyConnected` | New `VoiceManager` in gateway crate |
| Mass DM                         | Per-recipient cooldown (60s between DMs to same target); global "≤5 fresh DM channels per minute" semaphore | `DiscordHttpClient::open_dm` |
| Mass channel-create             | "≤2 channel creates per 10 minutes per guild" — uses `governor` crate keyed by guild id | `patch_channel`/create wrapper |
| Unrealistic typing cadence      | `trigger_typing` already 10s-throttled by Discord server-side. We additionally cap the **client-side fire rate** to 1 per 8 seconds per channel, ignoring re-trigger requests inside the window | `DiscordHttpClient::trigger_typing` |
| Hitting moderator-only endpoints as non-mod | Pre-check `permissions` from `get_guild_member_me` cache before issuing `kick_member`, `ban_member`, `delete_message`, etc. Fail locally with `PermissionDenied` instead of letting Discord 403. The 403 itself counts toward the 10k/10min IP ban | All ban/kick/timeout/delete sites in `http.rs` |
| Bypassing slow-mode programmatically | Read `rate_limit_per_user` from cached channel object; refuse to send if the previous send time + slow-mode > now. Surface a UI hint, don't queue silently | New `SlowModeGuard` consulted by `send_message` |
| Racing message edits faster than human | Per-message debounce: ignore an edit submission if the previous edit on the same message_id was < 2 seconds ago. Discord allows it but it's a heuristic | `edit_message` (when added) |
| Burst on cold start (login → fetch all guilds → fetch all channels → fetch all messages) | Token-bucket on outbound HTTP capped at 10 req/s burst, 5 req/s sustained. Independent of Discord's bucket headers — those are followed too | `DiscordHttpClient` middleware layer |
| 429 storms                      | On 429, sleep for `Retry-After` exactly; on second 429 within 60s on same bucket, sleep `Retry-After * 2` and emit a warn event the UI can show as "Discord is throttling us — backing off" | Generic response handler |

Sub-steps:

- [x] **D.1** Add `clients/discord/src/guardrails.rs` housing
      `RateGuard`, `SlowModeGuard`, `PermissionGuard`. All take
      `Arc<RwLock<...>>` state passed by the backend.
- [x] **D.2** Wire `RateGuard` (token bucket) into `DiscordHttpClient`
      as a `before_send` hook in every helper (`get`/`post_json`/etc.).
      Use `governor::RateLimiter` with quota `Quota::per_second(5)
      .allow_burst(NonZeroU32::new(10).unwrap())`.
- [x] **D.3** Add `VoiceManager` (sketch, not full impl since voice
      isn't shipped yet) — at minimum a `Mutex<Option<VoiceSession>>`
      that errors on second `connect()`.
- [x] **D.4** Wire `PermissionGuard` into `kick_member`, `ban_member`,
      `unban_member`, `set_member_timeout`, `delete_message`,
      `patch_channel`, `reorder_channels`. Each pre-checks against a
      cached `GuildMember.permissions` bitfield (refreshed from
      `get_guild_member_me` on guild-switch).
- [x] **D.5** Wire `SlowModeGuard` into `send_message`. Reads
      `channel.rate_limit_per_user`. Stores `last_send_at` per
      channel id.
- [x] **D.6** Add a per-channel typing-fire-rate cap inside
      `trigger_typing`. Keep `last_typing_at: HashMap<channel_id,
      Instant>`; ignore re-triggers inside 8s.
- [x] **D.7** Implement the 429 backoff in the central response
      handler. Currently every helper has its own `if !is_success`
      block — refactor into `Self::handle_response(resp).await?` first,
      then add the 429 branch. Same refactor unblocks Phase F
      telemetry.
- [x] **D.8** Add a "soft warning" surface: a `Signal<DiscordHealth>`
      the UI subscribes to, exposing fields `recent_429_count`,
      `last_403_route`, `last_401_at`. Settings page renders it as a
      "backend health" panel.

---

## Phase E — Pro-feature gating (Nitro) — shipped in change `rorooxlm`

Even when the API would let us send the request, we **refuse** at the
client layer for paid-tier features. The user explicitly asked for
this — the goal is "give Discord zero shitty reason to ban us", not
"maximise leeched value".

`premium_type` on the `DiscordUser` object (already in
`api::DiscordUser` — verify it's deserialized; if not, add it):

| `premium_type` | Tier              |
|----------------|-------------------|
| 0              | None              |
| 1              | Nitro Classic     |
| 2              | Nitro             |
| 3              | Nitro Basic       |

Source: discord-api-types v10 `UserPremiumType` enum + Discord
support docs.

Per-feature gate plan:

| Feature                          | Has-Nitro check       | UI gate                                                                          |
|----------------------------------|-----------------------|----------------------------------------------------------------------------------|
| Server stickers (cross-server)   | `premium_type >= 1`   | Sticker picker: dim "Other server" tab + tooltip "Nitro required"                |
| Animated emojis (cross-server)   | `premium_type >= 1`   | Emoji picker: filter out animated cross-server entries unless gated              |
| Upload limit (50 MB / 500 MB)    | `premium_type >= 2` for 50 MB; boost level for higher | File-attach button rejects > 8 MB locally (with helpful error) when no Nitro |
| Custom server discovery features | n/a server-side       | Don't expose discovery-only-with-Nitro filters                                   |
| Profile banners                  | `premium_type >= 1`   | Profile-edit page hides "Banner" upload box                                      |
| GIF avatars                      | `premium_type >= 2`   | Avatar picker rejects `.gif` upload; same error pattern as upload limit          |
| Super-reactions                  | `premium_type >= 2`   | Reaction picker hides the super-reaction toggle                                  |
| Custom statuses (rich)           | sometimes free        | Match official client behaviour exactly                                          |

Sub-steps:

- [x] **E.1** Verify `DiscordUser` deserializes `premium_type:
      Option<u8>`. Add it if missing.
- [x] **E.2** Add `clients/discord/src/nitro.rs` with
      `pub enum NitroTier { None, Classic, Full, Basic }` derived from
      `premium_type` (with `From<u8>`).
- [x] **E.3** Cache the current account's tier in a
      `Signal<Option<NitroTier>>` populated on `get_me()` (already
      called at backend init). Refresh on app focus.
- [x] **E.4** For each row in the table above, add a
      `NitroGate::can_<feature>(tier) -> bool` helper, used both at
      the UI affordance layer (hide/dim) and at the HTTP client layer
      as a defence-in-depth check that returns `Err(ClientError::
      PermissionDenied("Nitro required"))` before the request is sent.
- [x] **E.5** Specifically the 8 MB upload boundary: enforce in
      `send_message_with_attachments` (when added) using
      `NitroGate::max_upload_bytes(tier, channel.boost_level)`.
- [x] **E.6** Document the gating policy in `docs/dev/discord-nitro.md`
      so future contributors know we **intentionally** don't bypass —
      this reads to a casual code-spelunker as "missing feature" and
      they'll be tempted to "fix" it.

---

## Phase F — Testing + monitoring — shipped in change `ptovooksvvpm`

How we know the anti-ban work is paying off:

- [x] **F.1** Telemetry counters in `DiscordHttpClient` (incr inside
      the central response handler from D.7): `discord.http.2xx`,
      `discord.http.4xx.{401,403,404,429}`, `discord.http.5xx`,
      `discord.http.scrape.fail`, `discord.gateway.identify.success`,
      `discord.gateway.invalid_session`. Surfaced via the existing
      `tracing` infra. Implemented via new `GuardrailCounters` type in
      `guardrails.rs`; exposed via `DiscordClient::guardrail_stats()`.
      All events emit to `target: "discord-anti-ban"` for grep-ability.
- [ ] **F.2** Settings → Discord → "Backend health" panel reading the
      `Signal<DiscordHealth>` from D.8. Shows: build number in use,
      build-info age, last 429 timestamp + route, last 401 timestamp,
      total 4xx in last 24 h. **UI work — requires touching
      `crates/core/src/ui/`; deferred to a UI-focused agent pass.**
      The data layer (`DiscordHealth::update_build_info`, `build_stale_warning`,
      `GuardrailStats`) is fully implemented in this phase.
- [x] **F.3** Synthetic canary: `cargo test --features discord-canary
      --test canary -- --ignored`. Logs into a throwaway account,
      fetches guilds, sends 3 messages, asserts `http_429 == 0` and
      `http_401 == 0`. Created `clients/discord/tests/canary.rs` and
      added `discord-canary` Cargo feature. Credentials via env vars
      (`DISCORD_CANARY_TOKEN` etc.), never committed.
- [x] **F.4** Staleness check: `build_info::check_build_staleness(info)`
      returns `true` when scrape is > 14 days stale AND floor constant
      was bumped > 30 days ago. `FLOOR_CONSTANT_BUMPED_AT` constant
      tracks when the floor was last updated. `DiscordHealth::update_build_info`
      accepts the staleness flag. Emits a `warn` to `discord-anti-ban` target.
- [x] **F.5** Runbook `docs/dev/discord-ban-incident.md` created:
      8-step incident response covering timestamp collection, build number
      extraction, telemetry event grep, IDENTIFY payload decoding, scraper
      failure diagnosis, and canary re-run instructions. Includes the
      `discord-anti-ban` event reference table.

---

## Anti-goals (explicit)

- We do **not** rotate UAs randomly per request. Vencord and the real
  desktop client send a stable UA per session — randomisation is
  itself a bot signal.
- We do **not** spread requests across IPs. Single client, single IP
  per session, matches expected user behaviour.
- We do **not** auto-react / auto-reply / auto-anything on the user's
  behalf. The MCP/agent layer can do it, but only when triggered by
  user input — not on a schedule. Schedule-driven outbound messaging
  to non-friends is the #1 listed self-bot heuristic in every source
  reviewed.

---

## Research findings (sources)

### Build-number scraping

- adityaxdiwakar/discord-build-scraper —
  <https://github.com/adityaxdiwakar/discord-build-scraper> — the
  reference implementation for the regex-based asset-JS scrape;
  37-second polling cadence; checks `Build Number: NNN, Version Hash:
  XXXXXXX` literal in the chunk. Their `updater.py` is the source of
  the regex pattern in Phase A.
- KiyonoKara/Discord-Build-Info-PY —
  <https://github.com/KiyonoKara/Discord-Build-Info-PY> — alternative
  Python lib; same approach, slightly different regex.
- Pixens/Discord-Build-Number —
  <https://github.com/Pixens/Discord-Build-Number> — third
  implementation; confirms the asset-JS approach is the de-facto
  standard.

### `X-Super-Properties` schema

- KhafraDev/discord-verify wiki —
  <https://github.com/KhafraDev/discord-verify/wiki/X-Super-Properties> —
  full field list with types. Calls out that `client_event_source` is
  literal `null` and the `referrer*` fields "never seen used" but
  always present. Quote: "This header may be a new deterrent against
  bots."
- greg6775/Discord-Api-Endpoints —
  <https://github.com/greg6775/Discord-Api-Endpoints> — confirms
  `client_build_number` is the field Discord checks; lists `os`,
  `client_build_number` as the minimum-required pair for the request
  to be processed.
- Discord-Userdoccers (community-maintained) —
  <https://docs.discord.food/topics/rate-limits> — rate-limit policies
  used in Phase D thresholds; documents the 50 req/s global cap and
  the 10 000-invalid-requests-per-10-minutes IP ban.

### Self-bot ban heuristics

- Vencord FAQ — <https://vencord.dev/faq/> — quote: "Vencord doesn't
  automate anything, doesn't selfbot, and the requests it makes are
  indistinguishable from a vanilla client." Defines the bar Poly
  aspires to. Calls out three ban triggers: "1) spamming the API,
  2) self-botting, 3) interacting with non-user APIs."
- discord.py-self README —
  <https://github.com/dolfies/discord.py-self> — "Prevents user
  account automation detection" feature flag confirms the project
  ships fingerprint-mimicry built-in. Their `HeadersContext` doc
  says "configuring your own header context from scratch is not
  recommended, as it may lead to account termination by anti-abuse
  systems" — i.e. there's a known-good template they ship and you're
  meant to use it as-is. Phase B mirrors that pattern.
- discord.py-self discussion #578 —
  <https://github.com/dolfies/discord.py-self/discussions/578> — UA
  spoofing is fingerprinted by Discord; they recommend matching real
  client UA exactly.
- Discord support: "Automated User Accounts (Self-Bots)" —
  <https://support.discord.com/hc/en-us/articles/115002192352> —
  official policy. Termination risk is real; the bar to trigger it
  is "anomalous activity".
- Selfbot Rules gist — <https://gist.github.com/nomsi/2684f5692cad5b0ceb52e308631859fd> —
  community-maintained list of behaviours that trip heuristics
  (mass DM, schedule-driven outbound, concurrent voice, hitting
  moderator-only endpoints as non-mod). Source for the table in
  Phase D.

### Rate limits

- Discord docs (official) —
  <https://docs.discord.com/developers/topics/rate-limits> —
  authoritative on `Retry-After`, `X-RateLimit-Scope`, and the
  "never brute-force a rate limit" guidance.
- Xenon Bot blog — <https://blog.xenon.bot/handling-rate-limits-at-scale-fb7b453cb235> —
  practical guide to bucket handling at scale. Source for the
  per-bucket exponential-backoff design in D.7.
- Discord developer support: "My Bot is Being Rate Limited" —
  <https://support-dev.discord.com/hc/en-us/articles/6223003921559> —
  confirms the 50 req/s global cap.

### Nitro detection

- discord-api-docs issue #6623 — `premium_type` is included in
  partial user objects without OAuth2 —
  <https://github.com/discord/discord-api-docs/issues/6623> —
  confirms `premium_type` is reliably available on `GET /users/@me`
  with a user token. This is the field Phase E reads.
- discord-api-types v10 APIUser interface —
  <https://discord-api-types.dev/api/discord-api-types-v10/interface/APIUser> —
  documents `premium_type`, `banner`, `accent_color`,
  `avatar_decoration_data` as the Nitro-related fields.
- Discord support: "Nitro Boost & the API" —
  <https://support.discord.com/hc/en-us/community/posts/360044355652> —
  context on what features actually require Nitro vs Nitro Classic
  vs Nitro Basic; informs Phase E's per-feature gate table.

### Background

- Grokipedia: Discord selfbot —
  <https://grokipedia.com/page/discord-selfbot> — overview of
  detection landscape; confirms UA-based fingerprinting is the
  primary signal.
- Selfbots: Explanation and Perspectives (Scarletto, Medium) —
  <https://medium.com/discord-report/selfbots-explanation-and-perspectives-51d437ce0849> —
  insider perspective on Discord T&S enforcement priorities. Maps
  cleanly to the ranked Phase D guardrail order.
