# Discord Ban Incident Runbook

> Phase F.5 of `docs/plans/plan-discord-anti-ban.md`
>
> Last updated: 2026-05-21

This runbook covers what to do when a user reports that their Discord account
was banned or suspended after using Poly.  The goal is to collect enough data
to correlate the ban with a specific fingerprint mismatch, guardrail regression,
or Discord-side policy change, so we can fix the root cause before it hits more
users.

---

## Step 1 — Collect the incident timestamp

Ask the user for:

- The **exact time** (with timezone) when they received the ban notification.
- The **last action** they performed in Poly before the ban (e.g. "sent a
  message in #general", "joined a voice channel", "opened a DM").
- The **Poly version** (shown in Settings → About).
- The **OS** they were running on (Linux / macOS / Windows).

---

## Step 2 — Collect the build number in use

The Discord build number is logged at backend init:

```
grep "discord.http.2xx\|discord-anti-ban\|build_number" <log-file>
```

Or ask the user to open Settings → Discord → "Backend health" and share a
screenshot of the panel (F.2 health panel).  The build number appears as
"Build number in use: NNNNNN".

If the build number is lower than the current `LATEST_KNOWN_STABLE_BUILD`
constant in `clients/discord/src/build_info.rs`, that is the primary suspect —
we sent a stale fingerprint.

---

## Step 3 — Collect the last 100 telemetry events

The `discord-anti-ban` tracing target emits events for every guardrail trip
and HTTP error.  Collect the last 100 events before the incident timestamp:

```bash
# If the user has a log file:
grep "discord-anti-ban" <log-file> | tail -100

# If using the host-bridge KV trace sink (bisect-log pattern):
sqlite3 ~/.local/share/poly/storage.sqlite3 \
  "SELECT key, payload FROM poly_kv WHERE key LIKE 'bisect:%' ORDER BY key"
```

Look for:

| Event | Meaning |
|-------|---------|
| `discord.http.4xx.429` | Rate-limited — we were too fast |
| `discord.http.4xx.401` | Token expired or revoked mid-session |
| `discord.http.4xx.403` | Moderator action attempted without permission |
| `discord.gateway.invalid_session` | Discord invalidated our gateway session |
| `discord.guardrail.rate_guard.trip` | Token bucket was holding requests back |

A cluster of `4xx.403` events just before the ban timestamp suggests we
were hitting moderator-only endpoints as a non-moderator — confirm the
`PermissionGuard` was wired correctly for those routes.

---

## Step 4 — Collect a gateway IDENTIFY payload sample

The IDENTIFY payload (op 2) is logged at DEBUG level.  To capture it, set:

```bash
RUST_LOG=poly_discord=debug cargo run --...
```

Or grep for `"identify"` in the log.  Verify:

- `properties.os` matches the host OS (should be `"Linux"` / `"Mac OS X"` /
  `"Windows"`).
- `properties.browser` is `"Discord Client"`, not `"poly"` or `"DiscordBot"`.
- `properties.client_build_number` matches the build number from Step 2.
- The build number in IDENTIFY matches the `X-Super-Properties` HTTP header
  (they are derived from the same `SuperProperties` struct — any mismatch
  is a regression in Phase C wiring).

---

## Step 5 — Check the `X-Super-Properties` header

The `X-Super-Properties` header is base64-encoded JSON.  Decode a sample:

```bash
echo "<base64-value>" | base64 -d | python3 -m json.tool
```

Verify all required fields are present:

```json
{
  "os": "Linux",
  "browser": "Discord Client",
  "device": "",
  "system_locale": "en-US",
  "browser_user_agent": "Mozilla/5.0 ...",
  "browser_version": "130.0.0.0",
  "os_version": "",
  "referrer": "",
  "referring_domain": "",
  "referrer_current": "",
  "referring_domain_current": "",
  "release_channel": "stable",
  "client_build_number": 354133,
  "client_event_source": null
}
```

Red flags:
- `"browser": "poly"` or `"browser": "DiscordBot"` — Phase B regression.
- `client_build_number` below `LATEST_KNOWN_STABLE_BUILD` — scraper failure.
- `client_event_source` is the string `"null"` instead of JSON `null`.
- Any field missing entirely.

---

## Step 6 — Check for recent Discord build rotation

Discord rotates client builds roughly weekly.  If our scraper has not
refreshed in > 14 days AND the floor constant has not been bumped in > 30
days, the user sees a yellow "stale fingerprint" warning in the health panel.

Check:

```bash
# Is the floor constant recent enough?
grep "LATEST_KNOWN_STABLE_BUILD\|FLOOR_CONSTANT_BUMPED_AT" \
  clients/discord/src/build_info.rs

# When did the scraper last succeed?
grep "scraped fresh Discord build info" <log-file> | tail -5
```

If the scraper has been failing, investigate:
- Can we reach `https://discord.com/app`?
- Did Discord change the asset URL format (the regex `Build Number: NNN,
  Version Hash: XXXXXXX` is the target)?
- Is the `HttpClient` host-bridge route still reachable from the WASM context?

---

## Step 7 — Run the canary test

After fixing the suspected root cause, run the F.3 canary test against the
throwaway account to verify no 429s or 401s occur:

```bash
DISCORD_CANARY_TOKEN=<throwaway-token> \
DISCORD_CANARY_GUILD=<private-test-guild-id> \
DISCORD_CANARY_CHANNEL=<text-channel-id> \
cargo test -p poly-discord --test canary --features discord-canary -- --ignored
```

The canary output will confirm:

```
canary: PASSED — build_number=NNNNNN, 2xx=N, 429=0, 401=0, 5xx=0
```

---

## Step 8 — Post-incident record

After resolving the incident, add a row to the table below so future
engineers can cross-reference:

| Date | Build # in use | Root cause | Fix |
|------|---------------|------------|-----|
| (fill in) | (fill in) | (fill in) | (fill in) |

---

## Quick reference — grepping for `discord-anti-ban` events

All anti-ban telemetry emits to the `discord-anti-ban` tracing target.
In a standard tracing subscriber with `RUST_LOG=discord-anti-ban=warn`:

```
discord.http.4xx.429 on /api/v10/... — throttled by Discord; retry-after=5s
discord.http.4xx.401 — Unauthorized (token revoked or session expired)
discord.http.4xx.403 on /api/v10/guilds/.../members/... — permission denied ...
discord.http.5xx — server error HTTP 503
discord.gateway.identify.success — gateway READY received (build_number=354133)
discord.gateway.invalid_session — session invalidated by Discord; will re-identify
discord.guardrail.rate_guard.trip — outbound request held by token bucket
discord.guardrail.slow_mode.trip — channel ... is in slow mode; send blocked
discord.guardrail.permission.trip — kick on guild ... blocked (missing permission)
discord.guardrail.typing_cap.drop — typing re-trigger suppressed on ... (within 8s window)
discord build info is stale — consider updating Poly or refreshing the build number
```

Enable at `warn` level to see the high-signal events without noise; use `info`
to also capture `2xx` success counts and guardrail trip counts.
