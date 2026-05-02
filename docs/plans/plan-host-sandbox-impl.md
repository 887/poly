## Status: PLANNED — not started

# Plan: Host Sandbox Implementation

Cross-references:
- `docs/plans/plan-client-version-override-and-sandbox.md` Phase I
- `docs/client-settings.md` (the `captcha-sandbox` mechanism CLI surface)
- `crates/host-bridge/src/sandbox.rs` (current `StubSandbox`)

The `poly-host-sandbox` crate ships a `StubSandbox` that returns
`Err(SandboxError::NotImplemented)` for every call. This plan covers the real
plumbing that makes the `host-cap::sandbox-browser` mechanism functional across
all three shells (Wry desktop, Electron, web), then flips the UI from DISABLED
to live.

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

### Phase A — Wry (apps/desktop) implementation

- [ ] **A.1** Create `WrySandbox` struct in `apps/desktop/src/sandbox.rs`,
      implementing `poly_host_sandbox::Sandbox` trait. Holds a handle to the
      Wry event loop proxy + a `Mutex<HashMap<SandboxId, oneshot::Sender<Result<String, SandboxError>>>>`.
- [ ] **A.2** Implement `open_browser_sandbox(url, capture_pattern, cancel_label)`:
      send `EventLoopMessage::OpenSandbox` to the main thread → main thread
      builds a new `Window` + child `WebViewBuilder` with
      `with_navigation_handler(move |req| { if matches(req.url, pat) { resolve; close; return false; } true })`
      and `with_new_window_req_handler` to keep popup chains inside the sandbox.
- [ ] **A.3** Cookie isolation: pass `with_data_directory(temp_per_sandbox_dir)`
      so the sandbox `WebView` writes cookies to a tmpdir that gets purged on
      window close.
- [ ] **A.4** Cancel path: render a small native overlay (or just rely on
      window-close X) — `WindowEvent::CloseRequested` resolves the oneshot
      with `Err(SandboxError::UserCancelled)`.
- [ ] **A.5** Wire `WrySandbox` into `apps/desktop`'s host-cap registry; bump
      the host's advertised caps to include `HostCap::SandboxBrowser`.
- [ ] **A.6** Integration test (`apps/desktop/tests/sandbox_capture.rs`): spawn
      the Wry shell with a localhost test page that 302-redirects to
      `http://127.0.0.1:<port>/captured?token=abc`, capture pattern
      `*//captured*`, assert the resolved URL contains `token=abc` within 5s.

### Phase B — Electron (apps/desktop-electron) implementation

- [ ] **B.1** Add `ipcMain.handle('open-sandbox', async (_, opts) => {...})` in
      `apps/desktop-electron-web/electron/main.js`: create a
      `new BrowserWindow({ webPreferences: { partition: 'sandbox-' + opts.id, contextIsolation: true } })`,
      load `opts.url`, register `webContents.on('will-navigate', ...)` and
      `webContents.on('did-redirect-navigation', ...)` to detect the capture
      pattern.
- [ ] **B.2** Tear down on capture or cancel: `win.close()` then resolve the
      IPC promise. Also wire `win.on('closed', () => reject('UserCancelled'))`.
- [ ] **B.3** Native side: `ElectronSandbox` struct in
      `apps/desktop-electron/src/sandbox.rs` that uses the existing eval-bridge
      (HTTP on 9224) to invoke the IPC handler and await its JSON response.
- [ ] **B.4** Wire `ElectronSandbox` into the host-cap registry; bump caps.
- [ ] **B.5** Integration test mirroring A.6 but driving the Electron MCP
      (`mcp__poly-electron__launch_app` → trigger sandbox via host bridge →
      assert captured URL).

### Phase C — Web (apps/web) implementation, full path with redirect shim

User picked option (b): build the redirect shim now so Discord-on-web
actually works.

- [ ] **C.1** Add route `GET /sandbox/<id>?<captured-fragment>` to
      `crates/host-bridge/src/router.rs`. Handler: serves a tiny HTML page
      that runs `window.opener.postMessage({ type: 'sandbox-captured', id, url: location.href }, location.origin); window.close();`.
- [ ] **C.2** Implement `WebSandbox` in `apps/web/src/sandbox.rs` (compiles
      to WASM): on `open_browser_sandbox`, generate sandbox id, build URL
      where the OAuth callback target = `<host-origin>/sandbox/<id>?...`,
      `window.open(url, '_blank', 'popup,width=600,height=800')`, then
      register `addEventListener('message', ...)` filtered by `event.origin === location.origin && event.data.id === <id>` → resolve.
- [ ] **C.3** Cancel path: `setInterval(() => if (popup.closed) reject('UserCancelled'), 500)`,
      cleared on resolve.
- [ ] **C.4** Document the constraint: the OAuth provider MUST be configured
      with the shim URL as redirect target; backends that hardcode their own
      callback URL won't work on web (note this in `docs/client-settings.md`).
- [ ] **C.5** Wire `WebSandbox` into the apps/web host-cap registry; bump caps.
- [ ] **C.6** Integration test (`apps/web/tests/sandbox_capture_web.rs`):
      spawn `dx serve --platform web`, drive via `mcp__poly-web__*` tools, fake
      the OAuth provider with a localhost test server that 302s to the shim,
      assert captured URL.

### Phase D — UI integration (closes plan-client-version-override-and-sandbox.md Phase I)

- [ ] **D.1** Remove the DISABLED rendering for the `captcha-sandbox`
      mechanism row in `crates/core/src/ui/account/mechanism_picker.rs` (or
      wherever the gating lives — `grep -rn 'DISABLED.*sandbox' crates/core/`).
      Replace with live "Configure" button.
- [ ] **D.2** Wire the "Configure" button: opens a popover that lets the user
      verify the host advertises `HostCap::SandboxBrowser`, with a "Test" link
      that runs a no-op sandbox call (open `https://example.com`, capture
      pattern `*example.com*`) to verify end-to-end plumbing.
- [ ] **D.3** End-to-end Discord captcha test: configure a Discord account
      that triggers captcha-on-login, verify all three shells (Wry, Electron,
      web) successfully complete the captcha and persist the session token to
      KV.
- [ ] **D.4** Update `docs/client-settings.md` `captcha-sandbox` section: flip
      "currently DISABLED" wording to "live; supported on Wry / Electron /
      Web (Web requires OAuth provider redirects to host-bridge shim)".

---

## Acceptance criteria (DONE bar)

- All three shells advertise `HostCap::SandboxBrowser` in their capability
  manifest.
- The DISABLED gating in the mechanism picker is removed.
- A Discord login that triggers a captcha completes end-to-end on all three
  shells in a manual test.
- `docs/client-settings.md` reflects the live status of the mechanism.
- All four phases ticked, status header flipped to `## Status: ✅ DONE`.
