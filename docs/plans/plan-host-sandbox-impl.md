## Status: PLANNED — not started

# Plan: Host Sandbox Implementation

Cross-reference: `docs/plans/plan-client-version-override-and-sandbox.md` Phase I.

Client-settings CLI reference (including the `captcha-sandbox` mechanism): `docs/client-settings.md`.

The `poly-host-sandbox` crate currently ships a `StubSandbox` that returns
`Err(SandboxError::NotImplemented)` for every call. This plan covers the real
plumbing that makes the `host-cap::sandbox-browser` mechanism functional.

## Problem statement

Several backends (Discord, in particular) require the user to complete a
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
  `navigation_handler` / `with_url_handler` callbacks.
- **Desktop-Electron:** open a `BrowserWindow` with `webContents.on('will-navigate')`
  and `webContents.on('did-navigate')` listeners; send captured URL via IPC.
- **Web (apps/web):** open a popup window (`window.open`) and poll
  `popup.location.href` with `setInterval`; blocked by cross-origin policy for
  third-party domains, so a service-worker intercept or a server-side redirect
  shim may be required.

CDP-style navigation interception, cookie capture, and cancel UX are all
shell-specific and out of scope for the stub crate.

## Phases

Phase A — Wry implementation
- [ ] **A.1** Implement `open_browser_sandbox` in a `WrySandbox` struct.
- [ ] **A.2** Wire into `apps/desktop` host-cap registry.
- [ ] **A.3** Integration test: open example.com, capture redirect.

Phase B — Electron implementation
- [ ] **B.1** Implement `open_browser_sandbox` via Electron IPC bridge.
- [ ] **B.2** Wire into `apps/desktop-electron` host-cap registry.
- [ ] **B.3** Integration test: open example.com, capture redirect.

Phase C — Web popup implementation
- [ ] **C.1** Implement `open_browser_sandbox` via popup + polling for same-origin URLs.
- [ ] **C.2** Document cross-origin limitation; add server-side redirect shim option.
- [ ] **C.3** Wire into `apps/web` host-cap registry.

Phase D — UI integration (Phase F of client-version plan)
- [ ] **D.1** Remove DISABLED rendering for `captcha-sandbox` mechanism once
      at least one shell advertises `HostCap::SandboxBrowser`.
- [ ] **D.2** End-to-end test with Discord captcha mock.
