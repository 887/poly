# TEST_HARNESS — Poly Project

Run this file as a subagent task. Execute every step in order and report
pass/fail for each. Stop on first hard failure (compilation error, panic).
Warnings are acceptable unless they are new compared to the previous run.

---

## 1. Rust type-check (all crates)

```bash
cargo check --workspace 2>&1
```

Expected: `Finished` with zero errors.

---

## 2. Clippy (poly-core + all native plugins + chat-mcp)

```bash
cargo clippy \
  -p poly-core \
  -p poly-client \
  -p poly-demo \
  -p poly-stoat \
  -p poly-matrix \
  -p poly-discord \
  -p poly-teams \
  -p poly-lemmy \
  -p poly-hackernews \
  -p poly-github \
  -p poly-server-client \
  -p poly-chat-mcp \
  -- -D warnings 2>&1
```

Expected: zero errors. Pre-existing warnings in unrelated crates are ignored.

---

## 3. WASM build check

```bash
cd apps/web && dx build --platform web 2>&1 | tail -5
```

Expected: `Client build completed successfully!`

---

## 4. Unit tests

`cargo test --workspace` does not work because the repo mixes native and WASM targets
(dependency conflicts). Run each testable crate individually instead:

```bash
cargo test \
  -p poly-core \
  -p poly-client \
  -p poly-demo \
  -p poly-stoat \
  -p poly-matrix \
  -p poly-discord \
  -p poly-teams \
  -p poly-lemmy \
  -p poly-hackernews \
  -p poly-github \
  -p poly-server-client \
  -p poly-plugin-host 2>&1
cargo test -p poly-plugin-loader-tests --tests 2>&1
cargo test -p poly-chat-mcp --test mcp_integration 2>&1
cargo test -p poly-chat-mcp --lib 2>&1
```

Expected: all tests pass. Report any failures with test name + stderr.

---

## 5. poly-web MCP smoke-test — UI changes only

> Skip this section if no `.rs`, `.css`, or `.html` files changed.
> This step uses the **poly-web** MCP server (custom Rust binary at
> `mcp/web-devtools-mcp/`), NOT Playwright or `chrome-devtools-mcp`.
> If the poly-web MCP is not loaded in the current session, report
> SKIP — do not substitute any other browser MCP.

Workflow:

1. `mcp__poly-web__launch_app` — starts `dx serve --platform web` + Chromium.
   Non-blocking; returns immediately.
2. Poll `mcp__poly-web__get_last_build_status` every 5–10s until
   `state != "Running"`. Report FAIL if `state == "Failed"` and include
   the tail of `mcp__poly-web__get_last_build_log`.
3. `mcp__poly-web__connect_cdp` — attach to the running Chromium.
4. `mcp__poly-web__take_screenshot` of the root route and each
   top-level UI surface affected by the change.
5. `mcp__poly-web__list_console_messages` — fail on any `error`-level
   console messages that are new compared to a clean baseline.
6. For each modified component / route, exercise the golden path
   (click, type, navigate) and re-screenshot.
7. `mcp__poly-web__kill_app` when done.

Pass criteria: build succeeds, no error-level console messages, all
screenshots render (no blank / 0x0 / crash overlays), every exercised
interaction responds as expected.

---

## 6. Persona e2e mock smoke

> Run after any change that touches `mcp/chat-mcp/src/persona/`, `crates/core/src/ui/agent/persona/`,
> or `tests/e2e/persona-multi-agent.sh`. Safe to run on all other changes too — it skips
> cleanly when the script or scenario is absent.

```bash
if [ ! -f tests/e2e/persona-multi-agent.sh ]; then
  echo "SKIP — tests/e2e/persona-multi-agent.sh not present"
  exit 0
fi

# Primary scenario (Phase E.3 of plan-persona-e2e-multi-agent.md — not yet shipped).
# Falls back to two-personas-handoff which is present post Phases A-C.
# Decision: mcp-to-ui-live-update doesn't exist yet; fallback documented here per Phase S design choice.
SCENARIO="mcp-to-ui-live-update"
FALLBACK="two-personas-handoff"
SCRIPT="tests/e2e/persona-multi-agent.sh"

if bash "$SCRIPT" --list-scenarios 2>/dev/null | grep -q "^${SCENARIO}$"; then
  timeout 300 bash "$SCRIPT" --scenario "$SCENARIO" --mode mock-claude
  RC=$?
else
  echo "INFO — scenario '${SCENARIO}' not found (Phase E.3 not yet shipped); running fallback '${FALLBACK}'"
  timeout 300 bash "$SCRIPT" --scenario "$FALLBACK" --mode mock-claude
  RC=$?
fi

if [ $RC -eq 124 ]; then
  echo "FAIL — persona e2e smoke exceeded 5-minute budget (timeout 300s)"
  exit 1
fi
exit $RC
```

Pass criteria: script exits 0 within 300 seconds. A "SKIP" line in stdout is an
acceptable pass for branches that don't yet have the e2e plan landed.

---

## 7. Discord voice transport CLI smoke (Phase K.2)

> Skip-by-default — requires real Discord credentials and a live voice channel.
> Opt-in: set `RUN_VOICE_SMOKE=1` plus the env vars listed in the binary's doc.
> See `docs/plans/plan-voice-video-calls.md` Phase K.2 and `tools/discord-voice-smoke/`.

```bash
if [ "${RUN_VOICE_SMOKE:-0}" != "1" ]; then
  echo "SKIP — RUN_VOICE_SMOKE not set (requires real Discord credentials)"
  exit 0
fi

# Required env vars: DISCORD_TOKEN, DISCORD_GUILD_ID, DISCORD_VOICE_CHANNEL_ID
: "${DISCORD_TOKEN:?DISCORD_TOKEN must be set for voice smoke test}"
: "${DISCORD_GUILD_ID:?DISCORD_GUILD_ID must be set}"
: "${DISCORD_VOICE_CHANNEL_ID:?DISCORD_VOICE_CHANNEL_ID must be set}"

cargo run -p discord-voice-smoke 2>&1
```

Pass criteria: binary exits 0, prints "Smoke test PASSED", `open_input_calls >= 1`,
`open_output_calls >= 1`. Incoming sample count may be 0 if the channel is empty —
that is acceptable (decode loop verified by the UDP receive path even with no participants).

---

## 8. Stoat voice transport CLI smoke (Phase K.3)

> Skip-by-default — opt-in via `RUN_STOAT_VOICE_SMOKE=1`.
> Spins up the local `test-stoat` mock server on a random port, authenticates,
> connects voice, waits 2s for the mock raccoon participant to arrive, then asserts
> and disconnects. No external credentials or real audio hardware required.
> See `docs/plans/plan-voice-video-calls.md` Phase K.3 and `tools/stoat-voice-smoke/`.

```bash
if [ "${RUN_STOAT_VOICE_SMOKE:-0}" != "1" ]; then
  echo "SKIP — RUN_STOAT_VOICE_SMOKE not set"
  exit 0
fi

RUN_STOAT_VOICE_SMOKE=1 cargo run -p poly-stoat-voice-smoke 2>&1
```

Pass criteria: binary exits 0 and prints "Smoke test PASSED". Assertions:
- `open_input_calls >= 1` (encode loop started — mic backend opened)
- `open_output_calls >= 1` (decode loop started — speaker backend opened)
- `participant_join_events >= 1` (VoiceUserJoined received for synthetic raccoon participant)

---

## Reporting

After running all applicable steps, respond with a table:

| Step | Result | Notes |
|------|--------|-------|
| 1. cargo check | PASS/FAIL | ... |
| 2. clippy | PASS/FAIL | ... |
| 3. WASM build | PASS/FAIL | ... |
| 4. unit tests | PASS/FAIL | N tests passed |
| 5. poly-web MCP | PASS/SKIP/FAIL | ... |
| 6. persona e2e smoke | PASS/SKIP/FAIL | ... |
| 7. Discord voice smoke | PASS/SKIP/FAIL | SKIP if RUN_VOICE_SMOKE not set |
| 8. Stoat voice smoke | PASS/SKIP/FAIL | SKIP if RUN_STOAT_VOICE_SMOKE not set |
