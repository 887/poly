# Phase 3 Plan — Client Implementations

> **Created:** (original)
> **Updated:** 2026-03-30 — split into per-client sub-plans
> **Status:** 🟡 In Progress (Stoat)
> **Depends On:** Phase 2

---

## Overview

Phase 3 implements the four live messenger backends. Each client has its own plan doc:

| Client | Crate | Plan | Status |
|--------|-------|------|--------|
| **Stoat (Revolt)** | `poly-stoat` | [phase-3.1-stoat-plan.md](phase-3.1-stoat-plan.md) | 🟡 In Progress |
| **Matrix** | `poly-matrix` | [phase-3.2-matrix-plan.md](phase-3.2-matrix-plan.md) | ⬜ Not Started |
| **Discord** | `poly-discord` | [phase-3.3-discord-plan.md](phase-3.3-discord-plan.md) | ⬜ Not Started |
| **Microsoft Teams** | `poly-teams` | [phase-3.4-teams-plan.md](phase-3.4-teams-plan.md) | ⬜ Not Started |

WebRTC voice/video infrastructure is built in Phase 3.1 (Stoat) and reused by all subsequent clients.

---

## 3.0 Pre-Implementation

- [x] **3.0.1** Update all crate dependencies to latest stable versions
- [x] **3.0.2** Review and update `last-crate-update-date`
- [x] **3.0.3** Verify demo client still works as expected
- [x] **3.0.4** Review overall plan for any changes needed based on Phase 2 learnings

> **NOTE:** The Poly-Server Test Client (formerly 3.0.5–3.0.8) was moved to
> [Phase 2.7](phase-2.7-plan.md) and is complete.

---

## Phase 3 Overall Completion Criteria

- [ ] At least 2 backends fully working (Stoat + Matrix minimum)
- [ ] All backends implement `ClientBackend` trait
- [ ] Feature flags work — can build with any subset of backends
- [ ] Voice/video works for at least Stoat + Matrix
- [ ] Multi-account per backend works
- [ ] Cross-backend favorites sidebar works correctly
- [ ] Cross-backend DM/friends view works correctly
- [ ] Cross-backend notification aggregation works
- [ ] All backends respect encrypted backup sync
