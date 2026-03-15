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
