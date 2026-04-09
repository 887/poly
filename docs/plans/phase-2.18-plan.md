# Phase 2.18 Plan — Poly Web Mobile Test Mode

> **Created:** 2026-03-15  
> **Status:** In Progress

## Goal

Create a dedicated Poly web mobile-testing flow that lets developers:

1. force the shared app into a mobile UI mode,
2. render it inside a phone-sized Chromium viewport,
3. visually verify the responsive/mobile layout with the web devtools MCP.

## Scope

- `poly-core`
  - Add a **force-mobile UI mode** for web builds.
  - Make the shared shell usable on narrow/mobile screens.
  - Keep chat as the default full-screen content.
  - Move server/menu chrome into a left-side swipe drawer instead of a top rail.
- `poly-web-devtools-mcp`
  - Support mobile-like Chromium emulation instead of resize-only viewport changes.
- Docs / workflow
  - Document how to launch and verify the mobile web mode.

## Non-Goals

- Native Android/iOS gesture work
- Full mobile-only route set
- PWA packaging
- Touch-specific platform APIs beyond browser emulation

## Deliverables

- [x] Force-mobile UI mode exposed through Poly web runtime (`?mobile=1`)
- [x] Responsive/mobile shell styles for sidebar rails, channel list, chat body, and footer
- [x] Web devtools MCP support for mobile emulation settings
- [x] VS Code / docs workflow for launching and testing the mobile web mode
- [x] Visual verification screenshots / confirmation using web devtools MCP
- [x] Lint / check / WASM verification passing

## Implementation Plan

### 1. Runtime mobile mode
- Detect `?mobile=1` in the web runtime
- Add a root CSS class to force mobile layout even outside a naturally small viewport
- Default right-side chat/member rails closed in force-mobile mode
- Auto-close the left mobile drawer on route changes so channel/DM/settings content becomes the only visible page again

### 2. Shared responsive layout
- Keep the chat route full-width when mobile mode is active
- Move favorites rail, account/server rail, and channel list into a fixed left-side drawer
- Support button + swipe opening from the left edge and close via backdrop/button/swipe
- Keep the drawer open until navigation occurs, matching native messenger expectations
- Make the chat header, search, side column, and composer fit a narrow viewport

### 3. Web devtools mobile testing
- Extend the web MCP viewport path to support true mobile emulation settings
- Document a phone preset workflow (e.g. 393×852)
- Use MCP launch + viewport + visual inspection as the standard verification flow

### 4. Verification checklist
- [x] App launches successfully in web MCP
- [x] `?mobile=1` applies the mobile shell
- [x] Default mobile state shows chat-only content at full width
- [x] Left menu drawer opens with favorites/account/channel panes on screen
- [x] DM chat remains readable in narrow viewport
- [x] Drawer visual states captured via screenshots
- [x] No WASM crash overlay appears

## Session Notes

### 2026-03-15
- Initial plan created.
- Implementation started for a force-mobile web mode and MCP-driven mobile viewport testing.
- Reworked the first mobile-shell attempt after UX review: favorites/account rails no longer become a horizontal top bar.
- Implemented a left-side swipe drawer model so chat stays full-screen by default and server/menu chrome slides in from the left.
- Added route-change auto-close behavior in `MainLayout` and captured verification screenshots:
  - `devtools-screenshots/web-mobile-drawer-closed-2026-03-15.png`
  - `devtools-screenshots/web-mobile-drawer-open-2026-03-15-fixed.png`

### 2026-03-16
- Replaced the repeated left split-pane shells with a shared `SplitMenuShell` wrapper used by:
  - DM/server route shells
  - app settings
  - global search
  - account settings
  - server settings
- Added the matching shared `RightWingShell` wrapper for chat-side member/contact/utility rails so the
  desktop right column and mobile right-wing overlay share one structural entry point.
- Moved the `MainLayout` browser runtime scripts out of inline Rust strings into checked files:
  - `crates/core/assets/scripts/mobile_drawer_runtime.js`
  - `crates/core/assets/scripts/drag_bridge_runtime.js`
- Added an explicit native renderer stub path in `MainLayout` so the browser-only split-shell runtime
  is no longer assumed to exist for non-WASM renderers.
- Fixed the under-640 account/server rail leak: the account bar now stays fully offscreen until the
  left drawer opens, and both the left drawer closed/open states were re-verified in web MCP.
- Reworked the chat-side member/contact/utility rail into a true mobile right-side wing controlled by
  `.poly-mobile-right-wing-open` instead of stacking below the chat body.
- Mobile route changes now auto-close both wings:
  - left drawer closes in `MainLayout`
  - right member/contact wing closes in `MainLayout` + `ChatView` sync logic
- Verified live in poly-web with runtime viewport resizing on 2026-03-16:
  - desktop `/settings` at 1200×900
  - desktop `/settings` at 700×900
  - desktop `/search` at 700×900
  - mobile `/settings` at 372×1268 (drawer closed + open)
  - mobile `/demo/demo/demo-cat/settings` at 372×1268 (drawer closed + open geometry)
  - mobile `/demo/demo/demo-cat/channels/server-gaming` at 372×1268 (right member wing closed by default, opens as overlay, closes on navigation)
  - mobile `/demo/demo/demo-cat/dms/dm-user-alice` at 372×1268 (right contact wing opens as overlay, clears on navigation)
- Verification screenshots captured:
  - `devtools-screenshots/web-settings-shared-split-2026-03-16.png`
  - `devtools-screenshots/web-mobile-account-settings-closed-2026-03-16.png`
  - `devtools-screenshots/web-mobile-account-settings-open-2026-03-16.png`
  - `devtools-screenshots/web-mobile-server-members-open-2026-03-16.png`

### 2026-03-17
- Added `docs/desktop-mobile-shell-regression-test-plan.md` as the rerunnable verification plan for:
  - desktop DMs / DM contact rail
  - desktop server channel / member rail
  - desktop notifications
  - desktop global settings + identity
  - desktop account settings
  - desktop server settings
  - desktop Poly signup
  - desktop demo-data disable + re-enable
  - web desktop-width resize checks
  - web forced-mobile drawer + right-wing checks
- Verified live in **desktop-devtools** after rebuilding `apps/desktop-devtools` and launching the built
  binary as a background process:
  - DM shell screenshot: `devtools-screenshots/desktop-dms-shell-2026-03-16.png`
  - DM chat: `devtools-screenshots/desktop-dm-alice-2026-03-16.png`
  - DM contact rail open: `devtools-screenshots/desktop-dm-alice-contact-open-2026-03-16.png`
  - server chat: `devtools-screenshots/desktop-server-gaming-2026-03-16.png`
  - server member rail open: `devtools-screenshots/desktop-server-gaming-members-open-2026-03-16.png`
  - notifications: `devtools-screenshots/desktop-notifications-2026-03-16.png`
  - settings: `devtools-screenshots/desktop-settings-2026-03-16.png`
  - identity settings: `devtools-screenshots/desktop-settings-identity-2026-03-16.png`
  - account settings: `devtools-screenshots/desktop-account-settings-2026-03-16.png`
  - server settings: `devtools-screenshots/desktop-server-settings-2026-03-16.png`
  - Poly signup: `devtools-screenshots/desktop-poly-signup-2026-03-16.png`
  - demo toggle restored: `devtools-screenshots/desktop-demo-toggle-restored-2026-03-16.png`
- Re-verified live in **poly-web** after a fresh rebuild with runtime viewport resizing:
  - desktop settings wide: `devtools-screenshots/web-desktop-settings-wide-2026-03-16.png`
  - desktop search narrow: `devtools-screenshots/web-desktop-search-narrow-2026-03-16.png`
  - mobile settings closed/open:
    - `devtools-screenshots/web-mobile-settings-closed-2026-03-16.png`
    - `devtools-screenshots/web-mobile-settings-open-2026-03-16.png`
  - mobile account settings closed/open:
    - `devtools-screenshots/web-mobile-account-settings-closed-2026-03-16-rerun.png`
    - `devtools-screenshots/web-mobile-account-settings-open-2026-03-16-rerun.png`
  - mobile server right wing open: `devtools-screenshots/web-mobile-server-right-wing-open-2026-03-16-rerun.png`
  - mobile notifications after route-change cleanup: `devtools-screenshots/web-mobile-notifications-after-route-change-2026-03-16.png`
  - mobile DM right wing open: `devtools-screenshots/web-mobile-dm-right-wing-open-2026-03-16-rerun.png`
- Validation status:
  - `cargo check --workspace` ✅
  - `cargo check -p poly-web --target wasm32-unknown-unknown` ✅
  - `cargo cranky --workspace` ✅
