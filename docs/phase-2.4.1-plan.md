# Phase 2.4.1 Plan — Remaining Gaps from Phases 2.3 & 2.4

> **Status:** 🔲 Not Started  
> **Parent:** [Phase 2 Plan](phase-2-plan.md)  
> **Depends On:** [Phase 2.3](phase-2.3-plan.md) ✅, [Phase 2.4](phase-2.4-plan.md) ✅  
> **Priority:** Low — none of these block Phase 2.5 or Phase 3  
> **Last Updated:** 2026-03-01

---

## Overview

Small remaining items from phases 2.3 (backup server) and 2.4 (client crypto/sync)
that weren't completed during those phases. None are blockers — they're polish,
deployment helpers, and future-flagged features.

---

## 2.4.1.1 Backup Server Deployment Helpers

> **Crate:** `servers/backup-server/`

- [ ] **A** `docker-compose.yml` — single-service with volume mount for `./data`, env var template,
  expose port 8080. Reference `servers/backup-server/Dockerfile`.
- [ ] **B** `.env.example` — template documenting all `POLY_*` environment variables with
  sensible defaults and inline comments
- [ ] **C** README section on Docker deployment (build, run, env vars, volume mounts)

## 2.4.1.2 Backup Server Missing Integration Tests

> **File:** `servers/backup-server/tests/e2e_protocol_test.rs`

- [ ] **A** Integration test: rate limiting — exceed `POLY_RATE_LIMIT_MAX` failed auth
  attempts, verify 429 + `Retry-After` header
- [ ] **B** Integration test: `POLY_MAX_ACCOUNTS` enforcement — register N accounts,
  verify N+1 new pubkey gets 403 Forbidden

## 2.4.1.3 Electron Wrapper Setup

> **Location:** `apps/desktop-electron/`
> **From:** Phase 2 plan item 2.1.8

- [ ] **A** `apps/desktop-electron/electron/package.json` with Electron dependency
- [ ] **B** `apps/desktop-electron/electron/main.js` — loads WASM web build
- [ ] **C** Build script: compile poly-web target, then bundle with Electron
- [ ] **D** VSCode task + launch profile for Electron build

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

- [ ] `docker compose up` works from `servers/backup-server/` with `.env.example` copied to `.env`
- [ ] All E2E tests pass including new rate-limit + max-accounts tests
- [ ] Electron wrapper builds and launches the web app
- [ ] OS dark/light preference detection works with user override
- [ ] Decision registry in overall-plan.md is up to date

---

## Notes

These items can be done in any order and independently. The Electron wrapper (2.4.1.3)
is the most substantial item; the rest are < 1 hour each. Consider doing the decision
registry update (2.4.1.5) immediately since it's documentation-only and keeps the
plans accurate.
