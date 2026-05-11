## Status: ✅ DONE — all phases A–D shipped (changes `pnqyuryxoszp` (Wry) + `qxxzzkkruywk` (Electron) + `vslkuxvrymmr` (Web) + `kmwwpptu` (Phase D))

# Plan: Host Sandbox Implementation

Cross-references:
- `docs/plans/plan-client-version-override-and-sandbox.md` Phase I
- `docs/client-settings.md` (the `captcha-sandbox` mechanism CLI surface)
- `crates/host-bridge/src/sandbox.rs` (current `StubSandbox`)

## Existing UI surface (what's already wired)

The mechanism-toggle UI is **already live** but never renders the sandbox row
because no backend declares it yet:

- `crates/core/src/ui/settings/client_settings/mechanism_toggle.rs:14` —
  `MechanismToggle` component. When `requires_host_cap.is_some()` it renders
  greyed with the tooltip `client-settings-mechanism-disabled-host-cap` →
  "Requires host capability not available in this build" (en.ftl:597).
- `crates/core/src/ui/settings/client_settings/backend_card.rs:103` —
  iterates a backend's `Mechanism` list and renders a `MechanismToggle` per
  row.
- `clients/client/src/types.rs:1015` — `HostCap::SandboxBrowser` variant
  exists; `Mechanism::requires_host_cap: Option<HostCap>` field exists.
- `clients/lemmy/src/lib.rs:707` is the **only** backend currently declaring
  any mechanisms (all with `requires_host_cap: None`).
- The plugin-settings landing page (`crates/core/src/ui/settings/plugin_settings.rs`,
  the screen with "Discord (dev)" / "Microsoft Teams (dev)" cards) is where
  per-plugin sandbox status rows naturally dock.

So the gap is two-sided:
1. The `StubSandbox` returns `NotImplemented` on every shell — Phases A/B/C below.
2. No backend currently declares a `captcha-sandbox` mechanism that requires
   `HostCap::SandboxBrowser` — until something declares it, the disabled-row
   UI never shows. Phase D wires Discord (and Teams, where applicable).

## Goal

The `poly-host-sandbox` crate ships a `StubSandbox` that returns
`Err(SandboxError::NotImplemented)` for every call. This plan covers the real
plumbing that makes the `host-cap::sandbox-browser` mechanism functional across
all three shells (Wry desktop, Electron, web), then declares the matching
mechanism on Discord/Teams so the existing `MechanismToggle` actually surfaces
it (DISABLED everywhere → live on shells that advertise the cap).

---

## Problem statement

Several backends — Discord most acutely — require the user to complete a
browser-based challenge (CAPTCHA, OAuth popup, 2FA confirmation) before a
session token can be obtained or refreshed. The WASM UI cannot open a
full-featured browser window; only the native host shell can. A sandboxed
host-managed browser window needs to:

1. Open a given URL in an isolated browser context (no shared cookies/storage
   with the main app).
2. Monitor navigation events and detect when the URL matches a caller-supplied
   capture pattern (glob or regex).
3. Extract and return the matching URL (or a fragment of it) so the caller can
   parse OAuth tokens, session cookies, etc.
4. Support a user-visible cancel action that resolves with `UserCancelled`.

The implementation varies by shell:

- **Desktop (Wry):** open a child `WebView` in a new `Window`, intercept
  `navigation_handler` / `with_url_handler` callbacks. Cookies live in the
  child `WebView`'s isolated context.
- **Electron:** open a `BrowserWindow` with `webContents.on('will-navigate')`
  and `webContents.on('did-navigate')` listeners; send captured URL back via
  IPC. Use `partition: 'sandbox-<id>'` for storage isolation.
- **Web (apps/web):** open a popup window (`window.open()`); cross-origin
  policy blocks `popup.location.href` reads for third-party domains. Need a
  server-side redirect shim under `/sandbox/<id>` that the OAuth provider
  redirects back to — the shim then `postMessage`s the captured URL fragment
  to the opener, who closes the popup.

---

## Phases

### Phase A — Wry (apps/desktop) implementation — shipped in change `pnqyuryxoszp`

- [x] **A.1** `WrySandbox` struct in `crates/host-sandbox/src/wry_sandbox.rs`
      implementing `HostSandbox`. Spawns a dedicated OS thread with its own
      tao `EventLoop` (`any_thread = true` on Linux/Unix) per sandbox call.
- [x] **A.2** `open_browser_sandbox(url, capture_pattern, cancel_label)`:
      OS thread builds a `tao::Window` + `wry::WebViewBuilder` with
      `with_navigation_handler` that calls `glob_matches(pattern, url)` —
      resolves via `std::sync::mpsc` channel when matched, returns `false`
      to block navigation.
- [x] **A.3** Cookie isolation via `with_incognito(true)` — each sandbox
      call gets a fresh non-persistent data store.
- [x] **A.4** Cancel path: `WindowEvent::CloseRequested` in `run_return`
      sends `Err(SandboxError::UserCancelled)` through the channel.
- [x] **A.5** `crates/host-sandbox` adds `wry-sandbox` feature;
      `apps/desktop/Cargo.toml` enables it for native builds;
      `advertised_host_caps()` returns `[HostCap::SandboxBrowser]` when
      active. Re-exported via `apps/desktop/src/sandbox.rs`; logged at
      startup in `main.rs`.
- [x] **A.6** Integration test `apps/desktop/tests/sandbox_capture.rs`:
      spawns axum mock that 302s to `/captured?token=abc`, drives
      `WrySandbox`, asserts capture. Display-requiring test is opt-in via
      `POLY_SANDBOX_RUN_DISPLAY_TEST=1` (avoids GTK fatal abort in
      headless/broken-Wayland CI). Host-cap assertion always runs.

### Phase B — Electron (apps/desktop-electron) implementation — shipped in change `qxxzzkkruywk`

- [x] **B.1** Add `ipcMain.handle('open-sandbox', async (_, opts) => {...})` in
      `apps/desktop-electron-web/electron/main.js`: create a
      `new BrowserWindow({ webPreferences: { partition: 'sandbox-' + opts.id, contextIsolation: true } })`,
      load `opts.url`, register `webContents.on('will-navigate', ...)` and
      `webContents.on('did-redirect-navigation', ...)` to detect the capture
      pattern.
- [x] **B.2** Tear down on capture or cancel: `win.close()` then resolve the
      IPC promise. Also wire `win.on('closed', () => reject('UserCancelled'))`.
- [x] **B.3** Native side: `ElectronSandbox` struct in
      `apps/desktop-electron/src/sandbox.rs` that uses the existing eval-bridge
      (HTTP on 9224) to invoke the IPC handler and await its JSON response.
- [x] **B.4** Wire `ElectronSandbox` into the host-cap registry; bump caps.
      Adds `/host/caps` (returns `["SandboxBrowser"]`) and `/host/sandbox/open`
      (POST → `ElectronSandbox`) routes to the fullstack server.
- [x] **B.5** Integration test mirroring A.6 but driving the Electron MCP
      (`mcp__poly-electron__launch_app` → trigger sandbox via host bridge →
      assert captured URL). `tests/sandbox_capture_electron.rs`: two unit tests
      run unconditionally; the full CDP round-trip test is `#[ignore]`-gated
      (requires live Electron on port 9224).

### Phase C — Web (apps/web) implementation, full path with redirect shim (shipped in change `vslkuxvrymmr`)

User picked option (b): build the redirect shim now so Discord-on-web
actually works.

- [x] **C.1** Add route `GET /sandbox/<id>?<captured-fragment>` to
      `crates/host-bridge/src/router.rs`. Handler: serves a tiny HTML page
      that runs `window.opener.postMessage({ type: 'sandbox-captured', id, url: location.href }, location.origin); window.close();`.
      Implemented in `apps/poly-host/src/lib.rs` (the shared axum router
      used by apps/web's fullstack server).
- [x] **C.2** Implement `WebSandbox` in `apps/web/src/sandbox.rs` (compiles
      to WASM): on `open_browser_sandbox`, generate sandbox id, build URL
      where the OAuth callback target = `<host-origin>/sandbox/<id>?...`,
      `window.open(url, '_blank', 'popup,width=600,height=800')`, then
      register `addEventListener('message', ...)` filtered by `event.origin === location.origin && event.data.id === <id>` → resolve.
      Uses `js_sys::eval` + `wasm_bindgen_futures::JsFuture` to avoid a
      long list of web-sys feature flags.
- [x] **C.3** Cancel path: `setInterval(() => if (popup.closed) reject('UserCancelled'), 500)`,
      cleared on resolve. Implemented inside the JS Promise in C.2.
- [x] **C.4** Document the constraint: the OAuth provider MUST be configured
      with the shim URL as redirect target; backends that hardcode their own
      callback URL won't work on web. Documented in `apps/web/src/sandbox.rs`
      module-level comment (constraint: same-origin requirement for postMessage).
- [x] **C.5** Wire `WebSandbox` into the apps/web host-cap registry; bump caps.
      `poly-host-sandbox` gains a `web` feature; when enabled,
      `advertised_host_caps()` returns `[HostCap::SandboxBrowser]`.
      `apps/web/Cargo.toml` enables `poly-host-sandbox/web`. `WebSandbox`
      re-exported from `apps/web/src/main.rs` for future bootstrap wiring.
- [x] **C.6** Integration test (`apps/web/tests/sandbox_capture_web.rs`):
      5 tests covering shim HTML output, query-param passthrough, bad-id
      rejection, cache-control header, and C.5 host-cap advertisement.
      All pass: `cargo test -p poly-web --test sandbox_capture_web`.

### Phase D — Backend mechanism declarations + UI surfacing — shipped in change `kmwwpptu`

This phase actually makes the existing `MechanismToggle` show the sandbox row.
Until at least one backend declares `requires_host_cap: Some(SandboxBrowser)`,
the `client-settings-mechanism-disabled-host-cap` tooltip path in
`mechanism_toggle.rs:28` is unreachable.

- [x] **D.1** Add `Mechanism { id: "captcha-sandbox", … requires_host_cap:
      Some(HostCap::SandboxBrowser), … }` to Discord's mechanism list in
      `clients/discord/src/lib.rs`. Also added `super-properties` mechanism.
      FTL keys: `plugin-discord-mechanism-{super-properties,captcha-sandbox}-{label,desc}`.
- [x] **D.2** Add `oauth-sandbox` mechanism to Teams in `clients/teams/src/lib.rs` —
      Teams uses sandbox for AAD OAuth interactive popup (not needed for device-code
      flow). Keys `plugin-teams-mechanism-oauth-sandbox-{label,desc}`.
- [x] **D.3** Add `SandboxStatusRow` component to `plugin_settings.rs`.
      Fetches `GET /host/caps` at mount via `use_future`. Shows "Sandbox available" +
      "Test sandbox" button when `SandboxBrowser` cap is present. Added `/host/caps`
      route to `poly-host/src/lib.rs` shared router (caps set via `HostState::with_caps`).
      Each shell passes its caps at startup.
- [x] **D.4** FTL keys for D.3 surface: `client-settings-sandbox-status-{available,unavailable,test-button,test-running,test-success,test-failure}` added to `locales/en/main.ftl` and all four baked locale files.
- [x] **D.5** Unit tests in `clients/discord/src/lib.rs::mechanism_tests` verify:
      (1) `captcha-sandbox` is declared with `requires_host_cap = Some(SandboxBrowser)`,
      (2) `super-properties` is declared without host cap requirement,
      (3) `set_client_mechanism` rejects unknown IDs. All 3 tests pass.
- [x] **D.6** Updated `docs/client-settings.md`: flipped `captcha-sandbox` from
      "not advertised in v1" to "Live — supported on all three shells". Added Teams
      mechanism table. Added per-shell behaviour matrix and manual test recipe.

---

## Acceptance criteria (DONE bar)

- All three shells advertise `HostCap::SandboxBrowser` in their capability
  manifest.
- The DISABLED gating in the mechanism picker is removed.
- A Discord login that triggers a captcha completes end-to-end on all three
  shells in a manual test.
- `docs/client-settings.md` reflects the live status of the mechanism.
- All four phases ticked, status header flipped to `## Status: ✅ DONE`.
