# voice-ui-smoke

CDP-driven UI smoke test for the voice-banner connect/disconnect flow
(K.4 of `docs/plans/plan-voice-video-calls.md`).

## What it does

1. Connects to a running Chromium via `--remote-debugging-port` (CDP).
2. Navigates to a voice-channel URL on the local `apps/web` dev server.
3. Clicks `.btn-voice-join`, asserts `.voice-banner` appears.
4. Asserts the participant avatars container has the local user.
5. Clicks `.voice-ctrl-btn.disconnect`, asserts `.voice-banner` is gone.

## Why a Rust CDP binary, not Playwright

The repo already drives Chromium via raw CDP (`tokio-tungstenite`) in
`mcp/web-devtools-mcp/src/main.rs`. A Rust workspace member fits the
existing tooling shape (`tools/discord-voice-smoke/`,
`tools/stoat-voice-smoke/`) and avoids pulling in a Node/Playwright
toolchain. Decision rationale in
`docs/plans/plan-voice-ui-playwright.md`.

## Skip-by-default

Without `RUN_VOICE_UI_SMOKE=1` the binary exits 0 immediately — the
compile-only path that runs in CI and from `TEST_HARNESS.md`.

## To actually run

```bash
# 1. In one terminal, run apps/web on port 3000.
cd apps/web
dx serve --platform web --fullstack \
  @client --no-default-features --features "dev-plugins,web" \
  @server --platform server --no-default-features --features "dev-plugins,server"

# 2. In another terminal, launch Chromium pointed at the app
#    with synthetic mic/camera and a dedicated profile.
chromium --remote-debugging-port=9222 \
  --user-data-dir=/tmp/poly-voice-ui-smoke-profile \
  --use-fake-ui-for-media-stream \
  --use-fake-device-for-media-stream \
  http://127.0.0.1:3000

# 3. Sign in / add a test-stoat account, navigate to the voice
#    channel once by hand to confirm the URL, then copy it.

# 4. Run the smoke against that URL.
RUN_VOICE_UI_SMOKE=1 \
POLY_VOICE_UI_URL="http://127.0.0.1:3000/<your-route>/CHVOICE001" \
  cargo run -p poly-voice-ui-smoke
```

If `POLY_VOICE_UI_URL` is not set, the binary logs a SKIP message and
exits 0. This keeps `cargo run -p poly-voice-ui-smoke` safe to call
from automation that hasn't set up the prerequisites.

## Env vars

- `RUN_VOICE_UI_SMOKE` — must be `1` to do anything beyond compile.
- `POLY_VOICE_UI_URL` — full URL of a voice channel route to test.
- `POLY_CDP_PORT` — Chromium remote debugging port (default `9222`,
  matches `poly-web-devtools-mcp`).
- `RUST_LOG` — log filter (default `voice_ui_smoke=info`).

## Selectors the test depends on

| Element | Selector | Defined in |
|---------|----------|------------|
| Connect button | `.btn-voice-join` | `crates/core/src/ui/account/common/voice_view.rs` |
| Voice banner root | `.voice-banner` | `crates/core/src/ui/voice_banner.rs` |
| Participant avatars container | `.voice-banner-avatars .voice-banner-avatar` | same |
| Disconnect button | `.voice-ctrl-btn.disconnect` | same |

If any of these classes are renamed, update both this README and
`src/main.rs`.
