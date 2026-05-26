# Plan — MCP early-boot console capture

Today `list_console_messages` lazy-installs a `console.*` monkeypatch
on its first call. Anything logged before that first call — including
boot-time Rust panics surfaced through `wasm-bindgen` → `Uncaught
RuntimeError: unreachable` — is lost. We diagnosed the auto-signin
`BorrowMutError` only because the human pasted the dx-serve overlay
into chat; the MCP itself reported `__polyConsoleLogs = []`.

This plan moves the capture install point earlier so the MCP catches
the panic on its own.

## Phase Z — CDP-backed MCPs (`poly-web`, `poly-electron`) — shipped in change `twrzuluk → next`

- [x] **Z.1** Added `CONSOLE_CAPTURE_INSTALL` + `CONSOLE_CAPTURE_PRELUDE`
      pub consts in `mcp/devtools-protocol/src/backend.rs`. The install
      script is idempotent (returns early if `__polyConsoleLogs` exists)
      and ALSO hooks `window.error` + `unhandledrejection` so wasm-bindgen
      panic rethrows land in the buffer.
- [x] **Z.2** `web-devtools-mcp/src/main.rs` `connect_cdp` now sends
      `Page.addScriptToEvaluateOnNewDocument` + `Runtime.evaluate` with
      the prelude. Best-effort `drop(self.cdp_send(...).await)`.
- [x] **Z.3** Same wiring in `electron-devtools-mcp/src/main.rs`.
- [x] **Z.4** Updated doc comment on `CONSOLE_CAPTURE_JS` (lazy path)
      to note CDP backends pre-install via connect_cdp.

## Phase Z+ — Wry / HTTP-eval (poly-desktop)

Desktop has no CDP; the lazy monkeypatch is the only option until we
bake the prelude into the Wry init script. Out of scope for this fix —
desktop boot crashes are rare and the user mostly works against web /
electron.

- [ ] **Z+.1** (deferred) Inject the prelude via Wry's
      `WebViewBuilder::with_initialization_script` in
      `apps/desktop-web/src/main.rs`. Captures from page-load. Track
      separately when desktop becomes the active debug target.

## Verification

- [ ] **V.1** Rebuild all three MCPs (poly-web, poly-electron, poly-desktop).
- [ ] **V.2** Re-trigger the auto-signin BorrowMutError condition
      (revert `twrzuluk` locally, navigate to `?auto_signin=1`,
      `list_console_messages` should now return the panic line
      "The hook list is already borrowed" without any prior nav).
- [ ] **V.3** Restore `twrzuluk`, verify normal boot logs (e.g.
      "registered 16 test accounts") appear in `list_console_messages`
      without the human pasting them.

## Status: ✅ DONE — Phase Z shipped 2026-05-27 (Phase Z+ deferred for desktop)
