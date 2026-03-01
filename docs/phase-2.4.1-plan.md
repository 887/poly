# Phase 2.4.1 Plan — Remaining Gaps from Phases 2.3 & 2.4

> **Status:** ✅ Complete  
> **Parent:** [Phase 2 Plan](phase-2-plan.md)  
> **Depends On:** [Phase 2.3](phase-2.3-plan.md) ✅, [Phase 2.4](phase-2.4-plan.md) ✅  
> **Priority:** Low — none of these block Phase 2.5 or Phase 3  
> **Last Updated:** 2026-03-02

---

## Overview

Small remaining items from phases 2.3 (backup server) and 2.4 (client crypto/sync)
that weren't completed during those phases. None are blockers — they're polish,
deployment helpers, and future-flagged features.

---

## 2.4.1.1 Backup Server Deployment Helpers

> **Crate:** `servers/backup-server/`

- [x] **A** `docker-compose.yml` — single-service with volume mount for `./data`, env var template,
  expose port 8080. Reference `servers/backup-server/Dockerfile`.
- [x] **B** `.env.example` — template documenting all `POLY_*` environment variables with
  sensible defaults and inline comments
- [ ] **C** README section on Docker deployment (build, run, env vars, volume mounts) *(deferred to later)*

## 2.4.1.2 Backup Server Missing Integration Tests

> **File:** `servers/backup-server/tests/e2e_protocol_test.rs`

- [x] **A** Integration test: rate limiting — `test_rate_limiting_blocks_after_n_failures`:
  sets rate_limit_max=3, makes 3 failed auths (wrong passphrase), verifies 4th gets 429.
  Rate limiting uses in-memory DashMap on AdminState (same pattern as admin rate limiter).
  ConnectInfo<SocketAddr> extractor added to `authenticate` handler; both TestServer variants
  now use `into_make_service_with_connect_info::<SocketAddr>()`.
- [x] **B** Integration test: `test_max_accounts_enforcement` — sets max_accounts=1,
  registers TEST_PK_A (succeeds), tries TEST_PK_B → 403 Forbidden. Re-auth of PK_A succeeds.

## 2.4.1.3 Electron Wrapper Setup

> **Location:** `apps/desktop-electron/`
> **From:** Phase 2 plan item 2.1.8

- [x] **A** `apps/desktop-electron/electron/package.json` — Electron 33 + electron-builder 25,
  platform targets: Linux AppImage/deb, macOS dmg, Windows nsis/portable
- [x] **B** `apps/desktop-electron/electron/main.js` — BrowserWindow loading `dist/index.html`,
  security hardened (contextIsolation, sandbox, no nodeIntegration), ready-to-show pattern,
  external links open in system browser, DevTools on POLY_DEV=1
- [x] **C** `apps/desktop-electron/electron/preload.js` — exposes `window.polyElectron`
  with platform + version; safe bridge for future native integrations
- [x] **D** `apps/desktop-electron/build.sh` — builds WASM with `dx build`, then launches
  dev Electron or runs electron-builder for release packaging
- [x] **E** `apps/desktop-electron/src/main.rs`, `Cargo.toml`, `Dioxus.toml`, `cranky.toml`
  — Rust crate added to workspace; identical WASM entry point as apps/web

## 2.4.1.4 Theme System Polish

> **From:** Phase 2 plan items 2.4.2.6, 2.4.2.8

- [x] **A** Custom CSS editor model — `get/set_user_css()` in storage, live preview
  in theme settings, textarea with syntax highlighting (future)
- [x] **B** Dark/light mode device preference — detect OS dark/light via CSS
  `prefers-color-scheme` media query, provide user override toggle in settings

## 2.4.1.5 Decision Registry Update

> **File:** `docs/overall-plan.md` — Decision Registry (§10)

- [x] **A** Add decision D15: **Encryption algorithm** — ChaCha20-Poly1305 chosen over
  XSalsa20-Poly1305 (plan §6.2 says XSalsa20, implementation uses ChaCha20).
  Rationale: ChaCha20-Poly1305 is the IETF-standardized variant (RFC 8439),
  widely used (TLS 1.3, WireGuard), excellent RustCrypto support.
- [x] **B** Add decision D16: **Admin UI approach** — Tailwind+Alpine.js embedded SPA
  instead of planned Dioxus fullstack admin UI. Rationale: simpler, no Dioxus
  dependency in server crate, no build step, single `const &str` HTML.
- [x] **C** Add decision D17: **rand 0.10 upgrade** — `distributions` → `distr`,
  `DistString` → `SampleString`, `thread_rng()` → `rng()`, `OsRng` → `SysRng`.
  UUID crate removed from WASM path entirely.
- [x] **D** Update §6.2 algorithm text from "XSalsa20-Poly1305" to "ChaCha20-Poly1305"

---

## Completion Criteria

- [x] `docker compose up` works from `servers/backup-server/` with `.env.example` copied to `.env`
- [x] All 12 E2E tests pass including new rate-limit + max-accounts tests
- [x] Electron wrapper builds and launches the web app (`build.sh` + complete JS/Rust files)
- [x] OS dark/light preference detection works with user override (prefers-color-scheme)
- [x] Decision registry in overall-plan.md is up to date (D15, D16, D17 added)

---

## Session Summary — 2026-03-02

Implemented all open items:

1. **docker-compose.yml** — backup server compose file with healthcheck, named volume, all env vars
2. **.env.example** — full documentation of all POLY_* variables with defaults and inline comments
3. **Rate limiting** — switched from DB-based approach (SurrealDB write timing issues) to in-memory
   DashMap on AdminState (same proven pattern as admin login rate limiter). Added ConnectInfo
   extractor to `authenticate` handler, updated serve to use `into_make_service_with_connect_info`.
4. **Integration tests** — `test_rate_limiting_blocks_after_n_failures` and `test_max_accounts_enforcement`
   both pass (12/12 total). Added `TestServer::start_with_limits()` helper.
5. **Electron wrapper** — full setup: `src/main.rs`, `Cargo.toml`, `Dioxus.toml`, `cranky.toml`,
   `electron/main.js`, `electron/preload.js`, `electron/package.json`, `build.sh`.
   Crate added to workspace (previously commented out).

## Notes

These items can be done in any order and independently. The Electron wrapper (2.4.1.3)
is the most substantial item; the rest are < 1 hour each. Consider doing the decision
registry update (2.4.1.5) immediately since it's documentation-only and keeps the
plans accurate.
