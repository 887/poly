# Poly — Root Agent Instructions

> **READ THIS FILE FIRST** before any work on this project.  
> **Last Updated:** 2026-02-28

---

## Project Identity

- **Name:** Poly (PolyGlot Messenger)
- **Type:** Cross-platform, multi-backend messenger client
- **Language:** Rust (latest stable)
- **UI Framework:** Dioxus 0.7.3
- **Database:** SurrealDB 3.0.x (SurrealKV backend EVERYWHERE — desktop + mobile)
- **License:** MIT / Apache-2.0 dual license

---

## MANDATORY RULES — READ EVERY SESSION

### 1. Version Constraints (NON-NEGOTIABLE)

| Dependency | Version | Documentation Source |
|---|---|---|
| **Rust** | Latest stable (`rustup update stable`) | https://doc.rust-lang.org/stable/ |
| **Dioxus** | **0.7.3** | https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.3 and https://dioxuslabs.com/learn/0.7/ |
| **SurrealDB** | **3.0.x** | https://surrealdb.com/3.0 — ONLY use 3.0 documentation. Do NOT reference 2.x or 1.x docs. |
| **Tokio** | Latest (multi-threaded runtime, implied by Dioxus) | |

**DO NOT** use outdated API patterns from older Dioxus (0.4, 0.5, 0.6) or SurrealDB (1.x, 2.x) versions.

### 2. Crate Update Policy

- **Always use latest stable versions** of all Rust crates
- Check `last-crate-update-date` in the workspace root at the START of every session
- If the date is **older than 3 months**, run `cargo update` and update all workspace dependencies
- After updating, update the date in `last-crate-update-date`
- When adding a new crate, use the latest version from crates.io

### 3. Hot Reload — CRITICAL

- **poly-core** is the shared library crate where most development happens
- It **MUST** support Dioxus subsecond hot-reload via `dx serve --hotpatch`
- Use `subsecond::call()` patterns where needed
- **Test hot-reload after any structural changes to poly-core**
- If hot-reload breaks, fixing it is the #1 priority above all other work

### 4. Workspace Structure

- **Cargo workspace** with `[workspace.dependencies]` for shared dependency versions
- Each crate has its own `agents.md` (agent instructions) and `README.md`
- **Read the crate's `agents.md` before working on that crate**
- **Update the crate's `agents.md`** when you make architectural decisions or learn something important
- Use agent.md and README.md files as **eidetic memory** — document everything

### 5. Feature Flags

Messenger backends are feature-flagged in `poly-core`:
- `stoat` — Stoat (Revolt) client
- `matrix` — Matrix client
- `discord` — Discord client
- `teams` — Microsoft Teams client
- `demo` — Demo/mock client for testing

Someone should be able to build Poly with only `discord + teams` or any other combination.

### 6. i18n — ALL strings through translations

- **Every user-facing string** must go through the i18n system
- Use `.ftl` (Project Fluent) files in `locales/`
- Never hardcode English strings in UI components
- Languages: English (default), German, French, Spanish
- Fallback: user locale → `en`

### 7. Code Quality

- Run `cargo cranky --workspace` — zero warnings/errors policy (uses `cranky.toml` in each crate)
  - `cranky` is a `cargo clippy` wrapper that reads `cranky.toml` for denied/warned lints
  - Every crate and the workspace root has a `cranky.toml` denying: `warnings`, `unsafe_code`, `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::indexing_slicing`, `clippy::print_stdout`, `clippy::print_stderr`
  - Install once: `cargo install cranky`
- Run `cargo check --workspace` — verify all crates compile
- Run `cargo fmt --all` — consistent formatting
- Write doc comments on all public items
- Write `// TODO(phase-X.Y.Z):` comments referencing the plan item number
- Add `// DECISION(DX):` comments referencing decision numbers from overall-plan.md

### 7a. ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression attribute to source code.

This applies to **all** Clippy lints in every `cranky.toml`:
- `warnings = true` — all compiler warnings are errors
- `unsafe_code`
- `clippy::unwrap_used`
- `clippy::expect_used`
- `clippy::panic`
- `clippy::indexing_slicing`
- `clippy::print_stdout` / `clippy::print_stderr`

When `cargo cranky` reports a lint violation, **FIX THE CODE**. Never suppress it with an allow attribute.

**The ONLY exception** — inside `#[cfg(test)]` modules:
- `#[allow(clippy::unwrap_used)]` is permitted for test assertions (e.g. `result.unwrap()`)
- `#[allow(clippy::expect_used)]` is permitted for test setup (e.g. `val.expect("test context")`)
- No other allows are permitted even in tests

Rationale: these rules exist to prevent real bugs. Suppressing them hides the problem. Smaller models
try to `#[allow(...)]` their way out of lint errors — this is explicitly prohibited in this project.

### 8. Documentation Protocol

When making architectural decisions:
1. Document in the relevant crate's `agents.md`
2. If project-wide, also add to `docs/overall-plan.md` Decision Registry
3. Update the relevant phase plan checklist
4. Write code comments explaining non-obvious choices

### 9. Session Management

At the START of each coding session:
1. Read this `agents.md`
2. Read `last-crate-update-date` — update crates if >3 months old
3. Read the relevant phase plan to know what to work on next
4. Read the `agents.md` of the crate(s) you'll work on
5. Check `docs/overall-plan.md` for any open decisions

At the END of each session:
1. Run `cargo cranky --workspace` — fix ALL lint errors before committing
2. Run `cargo fmt --all` — format all code
3. Update phase plan checkboxes for completed items
4. Update relevant `agents.md` files with new learnings
5. Write a brief session summary in the phase plan (append to bottom)

### 10. Platform Targets

| App | Renderer | Entry Point |
|---|---|---|
| Desktop Wry | System webview (Wry) | `apps/desktop/` |
| Desktop Blitz | WGPU native (experimental) | `apps/desktop-blitz/` |
| Desktop Electron | WASM in Electron shell | `apps/desktop-electron/` |
| Android | Dioxus mobile | `apps/android/` |
| iOS | Dioxus mobile | `apps/ios/` |
| Web | Dioxus fullstack + Axum | `apps/web/` |
| Backup Server | Axum + Dioxus fullstack | `servers/backup-server/` |

### 11. Database Engine

**SurrealKV everywhere.** No RocksDB. No SQLite. No divergence between platforms.
- Feature: `kv-surrealkv` on the `surrealdb` crate
- If SurrealKV fails to compile on a mobile target, that's a P0 blocker to resolve

### 12. Research Resources

When implementing messenger backends, consult:
- **Stoat (Revolt):** `developers.stoat.chat` API docs, Revolt backend source (GitHub)
- **Matrix:** `matrix-sdk` docs on docs.rs, Matrix spec at spec.matrix.org
- **Discord:** Research carefully — TOS prohibits unofficial clients. Check for new developments.
- **Teams:** `ttyms` crate source code, Microsoft Graph API docs
- **WebRTC:** `webrtc` crate docs, look up platform-specific native bindings for camera/mic
- **Voice/Video on mobile:** Research Flutter packages with native bindings that could help, also native Rust bindings

### 13. Theme System

- CSS custom properties for all colors
- 3 built-in presets: neutral-dark (default), purple (Discord), red (Stoat)
- Full per-color customization + custom CSS editor
- Theme import/export (share CSS files)
- Dark mode default, light mode optional, follow-device-preference option

### 14. Security

- Local DB: account tokens may be stored unencrypted (acceptable)
- Backup server: ALL data encrypted BEFORE leaving device — NEVER send plaintext
- Identity: Ed25519 keypair, X25519 derived, BIP39 mnemonic recovery phrase
- Backup auth: PoW challenge + server-wide passphrase + long session tokens with device tracking

### 15. Git Workflow — NEVER COMMIT OR PUSH WITHOUT USER REVIEW

- **NEVER** run `git commit` or `git push` without the user explicitly reviewing and approving the changes first
- You MAY use `git add` / `git stage` and the staging area freely — this helps with diffs and tracking
- You MAY use `git diff`, `git log`, `git show`, `git stash`, `git checkout` to inspect history, compare versions, or recover older code
- Before committing: tell the user what changed and wait for their go-ahead
- Exception: if the user explicitly says "commit this" or "commit and push", then proceed
- Never force-push (`git push --force`) under any circumstances without explicit user consent

---

## File Map

| File | Purpose |
|---|---|
| `agents.md` (this file) | Root agent instructions — read first |
| `last-crate-update-date` | When crates were last updated |
| `docs/overall-plan.md` | Comprehensive project plan + decisions |
| `docs/phase-1-plan.md` | Phase 1 checklist (planning) |
| `docs/phase-2-plan.md` | Phase 2 checklist (structure + UI) |
| `docs/phase-3-plan.md` | Phase 3 checklist (client implementations) |
| `docs/research/` | Technology research notes |
| `crates/*/agents.md` | Per-crate agent instructions |
| `crates/*/README.md` | Per-crate documentation |
| `apps/*/agents.md` | Per-app agent instructions |
