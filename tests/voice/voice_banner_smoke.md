# Voice Banner Smoke Tests — Phase K (K.4 + K.5 + K.6)

Markdown test harness for the voice UI integration. Executed by the test agent
via the `mcp__poly-web` MCP server (see CLAUDE.md "MCP Workflow").

All steps that depend on phases not yet shipped are marked `TODO(Phase-X)` and
must be run as sanity-checks only — they are expected to be skipped or fail
gracefully rather than hard-fail the harness.

---

## Prerequisites

Before running any scenario:

1. `mcp__poly-web__launch_app` — start `dx serve --platform web --fullstack`.
2. Poll `mcp__poly-web__get_last_build_status` every 5 s until `state != "Running"`.
   - FAIL if `state == "Failed"` (paste tail of `get_last_build_log`).
3. `mcp__poly-web__connect_cdp` — attach CDP to the running Chromium.
4. Ensure `test-stoat` (port 9101) and `test-discord` (port 9102) fixtures are
   running (started by `poly-test-runner`; check `cargo run -p poly-test-runner`).
5. Seed a test account for each backend if not already present in the app's
   `poly_kv` store (use the fixture's `/test/auth/token` endpoint).

---

## Scenario K.4 — Stoat voice channel: connect → banner → participants → disconnect

**Depends on:** Phase G (Stoat voice UI integration, not yet shipped).
Steps marked `TODO(Phase-G)` are expected to be `SKIP` until Phase G lands.

### Steps

1. **Navigate** to the test-stoat server's voice channel route:
   - Route pattern: `/<backend>/stoat/<instance_id>/<account_id>/channels/<voice_channel_id>`
   - Use a seeded voice channel from the test-stoat fixture seed data.
   - `mcp__poly-web__navigate_page` to the route.
   - `mcp__poly-web__take_screenshot` — assert the channel list is visible.

2. **Click Connect** on the voice channel button.
   - `mcp__poly-web__click` on the voice channel join button (selector: `.voice-channel-join-btn`
     or the channel row containing the voice channel icon).
   - `TODO(Phase-G)`: Until Phase G ships, this click may render a "coming soon" placeholder
     rather than initiating a real Stoat voice connection.

3. **Assert voice banner appears**.
   - `mcp__poly-web__wait_for` selector `.voice-banner` with timeout 5 s.
   - `mcp__poly-web__take_screenshot` — banner must be visible in the bottom bar.
   - `TODO(Phase-G)`: The banner may appear immediately with the pseudo-backend
     `TemporaryCall` path (pre-Phase-G behaviour) or via real Stoat transport (post-Phase-G).
     Either is acceptable here; what matters is that the banner renders.

4. **Assert participant list updates**.
   - `mcp__poly-web__wait_for` selector `.voice-participant` with timeout 5 s.
   - `mcp__poly-web__take_screenshot` — at least one participant tile must be visible
     (the local user's own tile is always present once connected).
   - `TODO(Phase-G)`: After Phase G, remote participants from the test-stoat fixture's
     seeded voice state should also appear.

5. **Click Disconnect**.
   - `mcp__poly-web__click` on the hang-up button in the voice banner (selector:
     `.voice-banner-disconnect` or the red phone icon).

6. **Assert banner clears**.
   - `mcp__poly-web__wait_for` selector `.voice-banner` to disappear (poll for absence).
   - `mcp__poly-web__take_screenshot` — no voice banner must be visible.
   - `mcp__poly-web__list_console_messages` — assert no `error`-level messages from the
     disconnect path.

**Pass criteria:** Steps 3 and 6 succeed (banner appears and clears). Steps 4 and the
participant-count assertion in step 4 are `TODO(Phase-G)` and may skip without failure.

---

## Scenario K.5 — Held-call swap: Discord channel → Stoat DM → swap back

**Depends on:** Phase G (Stoat DM voice, not yet shipped for real transport).
The held-call mechanism itself is shipped (Phases C+D). This scenario tests the swap
with a second Discord-style call if Stoat voice is not available.

### Steps

1. **Start Discord voice channel call** (first active call).
   - Navigate to a test-discord server voice channel route.
   - Click Connect (as in K.4 step 2, but for Discord backend).
   - Assert voice banner appears (`wait_for .voice-banner`).
   - Screenshot.

2. **Start a second call** (this triggers the held-call swap mechanism).
   - **If Phase G has shipped:** Navigate to a test-stoat DM and initiate a voice DM call.
   - **If Phase G has NOT shipped (current state):** Navigate to a second Discord voice channel
     on a different seeded server. This exercises the held-call swap without requiring Phase G.
   - Click Connect / initiate the call.

3. **Assert first call is now held**.
   - `mcp__poly-web__wait_for` selector `.voice-banner-held` or a "held" indicator
     in the voice banner (the banner should show the held call count or a swap button).
   - Screenshot — held-call indicator must be visible.

4. **Click swap** to return to the first call.
   - `mcp__poly-web__click` on the swap/resume button in the voice banner.
   - `mcp__poly-web__wait_for` selector `.voice-banner` to show the first call as active again.
   - Screenshot.

5. **Assert second call is now held** (swap completed).
   - The held indicator should now show the second call.

6. **Disconnect both calls**.
   - Disconnect the active call via the hang-up button.
   - Assert banner updates to show the held call as active.
   - Disconnect the remaining call.
   - Assert banner clears (`wait_for .voice-banner` disappears).

**Pass criteria:** Held-call indicator appears in step 3, swap completes in step 4,
both calls disconnect cleanly in step 6. No error-level console messages.

**Note on Stoat DM voice:** Until Phase G ships, step 2 uses a second Discord channel
as the swap target instead of a Stoat DM call. This is intentional — the held-call
swap mechanism is backend-agnostic and can be verified with two same-backend calls.

---

## Scenario K.6 — Teams stub: click DM call → pending overlay → "coming soon" toast

**Depends on:** Phase I (Teams stub, SHIPPED in change `urzwsrny`).
This scenario should work today without any missing phase dependencies.

### Steps

1. **Navigate** to a test-teams DM route.
   - Route pattern: `/<backend>/teams/<instance_id>/<account_id>/dms/<dm_id>`
   - Use a seeded DM from the test-teams fixture (`servers/test-teams/`, port 9103).
   - `mcp__poly-web__navigate_page` to the DM route.
   - Screenshot — the DM conversation view must be visible.

2. **Click the call button** in the DM toolbar (phone / video icon).
   - `mcp__poly-web__click` on the call button (selector: `.dm-call-btn` or the phone icon
     in the DM header).
   - `mcp__poly-web__take_screenshot` — the pending-call overlay
     (`/:backend/teams/:instance/:account/dms/:dm_id/call`) must render.

3. **Assert pending overlay appears**.
   - `mcp__poly-web__wait_for` selector `.direct-call-overlay` or the route
     `.../dms/:dm_id/call` with timeout 3 s.
   - Screenshot — the "calling…" UI must be visible (not the DM conversation view).

4. **Wait for the "coming soon" toast**.
   - `mcp__poly-web__wait_for` selector `.toast[data-key="voice-teams-coming-soon"]`
     OR a toast containing the text "Teams calls are coming soon" with timeout 35 s
     (pseudo-backend ring timeout is 30 s; allow 5 s margin).
   - Screenshot — the toast must be visible.
   - Verify the FTL key `voice-teams-coming-soon` is the one shown (matches
     `crates/core/src/i18n/baked_locales_en.rs` line 549).

5. **Assert no real voice connection attempted**.
   - `mcp__poly-web__list_console_messages` — must not contain any message from the
     Discord or Stoat voice connection paths (no "Joining voice channel", no "op 0 IDENTIFY",
     no "UDP send").
   - The `open_input_calls` count on the active `AudioBackend` must remain 0
     (verifiable via `evaluate_script` calling `window.__polyDebug?.audioBackendStats`
     if the dev-build exposes it, or by asserting no mic permission dialog appeared).

6. **Dismiss the pending overlay**.
   - Click the cancel/back button on the pending-call overlay.
   - `mcp__poly-web__wait_for` selector `.direct-call-overlay` to disappear.
   - Navigate back to the DM view — assert the DM conversation renders normally.

**Pass criteria:** Steps 3 (overlay appears), 4 (toast fires within 35 s with correct
FTL key), 5 (no real audio device opened), and 6 (overlay dismisses cleanly) all pass.
No error-level console messages throughout.

---

## Reporting

After running all applicable scenarios, respond with a table:

| Scenario | Step | Result | Notes |
|----------|------|--------|-------|
| K.4 Stoat connect | 3 — banner appears | PASS/FAIL/SKIP(Phase-G) | ... |
| K.4 Stoat connect | 4 — participants | PASS/FAIL/SKIP(Phase-G) | ... |
| K.4 Stoat connect | 6 — banner clears | PASS/FAIL/SKIP(Phase-G) | ... |
| K.5 Held-call swap | 3 — held indicator | PASS/FAIL | ... |
| K.5 Held-call swap | 4 — swap completes | PASS/FAIL | ... |
| K.5 Held-call swap | 6 — both disconnect | PASS/FAIL | ... |
| K.6 Teams stub | 3 — overlay appears | PASS/FAIL | ... |
| K.6 Teams stub | 4 — toast fires | PASS/FAIL | FTL key present? |
| K.6 Teams stub | 5 — no real connect | PASS/FAIL | ... |
| K.6 Teams stub | 6 — overlay dismisses | PASS/FAIL | ... |
