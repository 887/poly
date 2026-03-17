# Desktop + Mobile Shell Regression Test Plan

> Created: 2026-03-16  
> Purpose: repeatable UI verification after shell, split-menu, member rail, drawer, or settings layout changes.

## When to run this plan

Run this checklist after changes to any of:
- `crates/core/src/ui/main_layout.rs`
- `crates/core/src/ui/split_shell.rs`
- `crates/core/src/ui/routes.rs`
- `crates/core/src/ui/settings/**`
- `crates/core/src/ui/account/settings/**`
- `crates/core/src/ui/account/server/settings/**`
- `crates/core/src/ui/account/common/chat_view.rs`
- `crates/core/assets/styling/layout.css`
- `crates/core/assets/styling/mobile-shell.css`
- `crates/core/assets/scripts/mobile_drawer_runtime.js`

## Mandatory build gates before visual testing

1. `cargo check --workspace`
2. `cargo cranky --workspace`
3. `cargo check -p poly-web --target wasm32-unknown-unknown`
4. `dx build --platform desktop` in `apps/desktop-devtools/`

Do not start visual verification until all four pass.

## Tooling workflow

### Desktop verification
Use the Desktop DevTools MCP, not ad-hoc manual launching.

Required sequence:
1. Start MCP server if needed.
2. Call `mcp_poly-desktop_launch_app`.
3. Poll `get_last_build_status` until `Succeeded`.
4. Call `mcp_poly-desktop_connect_cdp`.
5. Capture screenshots/snapshots during each major checkpoint.

**Inline-first rule:** use screenshot tools without a save path by default so the images are visible inline during verification. Save screenshot files only when you explicitly need an archival artifact.

### Mobile verification
Use the Web DevTools MCP with mobile emulation.

Recommended viewport preset:
- width `393`
- height `852`
- `mobile: true`
- `touch: true`
- `deviceScaleFactor: 2`

Use URL query `?mobile=1` when verifying forced mobile shell behavior.

## Test data setup

### Demo account routes
Use demo data for repeatable layout checks.

Known route family:
- Account root: `/demo/demo/demo-cat/...`
- DMs: `/demo/demo/demo-cat/dms`
- Server home: `/demo/demo/demo-cat/channels/server-gaming`
- Account settings: `/demo/demo/demo-cat/settings`
- Server settings: `/demo/demo/demo-cat/servers/server-gaming/settings`

### Poly signup route
- `/signup/poly`

### Global routes
- `/notifications`
- `/settings`
- `/settings/identity`
- `/search`

## Desktop regression checklist

Use a desktop-sized viewport first, then resize during runtime.

### Desktop viewport sequence
Check each relevant view at:
1. `1440x900`
2. `1100x900`
3. `700x900`

At each size verify:
- no main content is cut off
- no unexpected horizontal overflow
- left rails stay aligned
- right member/contact rail stays docked on the right, not stacked below
- header controls remain visible and clickable

### 1. Demo data toggle
- Open the app.
- Toggle demo data on.
- Verify demo account/server rails appear.
- Toggle demo data off and back on.
- Verify no broken blank layout or stale side rail remains.

### 2. DMs view
Route or navigate to:
- `/demo/demo/demo-cat/dms`

Verify:
- DM list renders fully
- content column is not clipped
- selecting a DM opens chat without stale right rail state from a previous route

### 3. DM chat + contact rail
From DMs, open a direct message such as Alice.

Verify:
- chat body fills available width
- member/contact toggle works
- right contact rail opens on the right
- closing the rail restores full-width chat
- resizing between desktop widths does not push the rail below chat

### 4. Server home
Route or navigate to:
- `/demo/demo/demo-cat/channels/server-gaming`

Verify:
- server channel list renders correctly
- chat or server-home content is not clipped
- left rails remain docked in the expected columns

### 5. Server channel + member rail
Open a real text channel from the server channel list.

Verify:
- channel switch loads correctly
- member rail toggle works
- right member rail opens as a dedicated right column
- right rail closes cleanly
- resizing between desktop widths keeps rail geometry correct

### 6. Notifications
Route or navigate to:
- `/notifications`

Verify:
- notifications list fills available content area
- filter controls/header are visible
- no left split-shell clipping at 700px width

### 7. Global settings
Route or navigate to:
- `/settings`
- `/settings/identity`

Verify:
- settings sidebar is fully visible at all desktop widths
- content pane is not cut off
- identity section renders fully and remains scrollable
- no overlap between sidebar and content

### 8. Account settings
Route or navigate to:
- `/demo/demo/demo-cat/settings`

Verify:
- account settings nav renders in the shared left split shell
- content is readable and scrollable
- no cutoff on narrow desktop widths

### 9. Server settings
Route or navigate to:
- `/demo/demo/demo-cat/servers/server-gaming/settings`

Verify:
- server settings nav renders in the shared left split shell
- content is readable and scrollable
- no cutoff on narrow desktop widths

### 10. Poly signup
Route or navigate to:
- `/signup/poly`

Verify:
- full-page signup renders without sidebar chrome
- layout remains centered and readable at all desktop widths
- resizing does not break form controls

## Mobile regression checklist

Use web MCP with `?mobile=1` and mobile emulation enabled.

### Mobile route set
Verify these routes:
- `/settings?mobile=1`
- `/demo/demo/demo-cat/settings?mobile=1`
- `/demo/demo/demo-cat/channels/server-gaming?mobile=1`
- `/demo/demo/demo-cat/dms?mobile=1`
- `/notifications?mobile=1`

### 1. Left drawer closed state
For each split-shell page verify:
- only primary content is visible by default
- favorites/account/channel rails are offscreen
- account bar does not leak onscreen below 640px

### 2. Left drawer open state
Open the drawer via button or left-edge swipe.

Verify:
- favorites rail is visible
- account/server rail is visible
- shared left panel is visible
- geometry is correct and not partially offscreen

### 3. Right wing behavior in chat
Open a DM chat and a server text channel.

Verify:
- member/contact rail is closed by default
- toggling it opens an overlay from the right
- chat remains the base content underneath
- right wing closes via toggle and on route navigation
- swipe from right edge can open/close where supported by the runtime

### 4. Route-change cleanup
With either drawer or right wing open, navigate to another route.

Verify:
- left drawer closes automatically
- right wing closes automatically
- the newly selected content is the only primary visible pane

## Suggested screenshot set

Capture at least:
- desktop settings wide
- desktop settings narrow
- desktop server channel with member rail open
- desktop DM with contact rail open
- mobile settings closed
- mobile settings drawer open
- mobile server chat with right wing open
- mobile DM chat with right wing open
- poly signup desktop
- identity settings desktop

## Failure notes template

For any failure, record:
- route
- viewport size
- whether demo mode was enabled
- expected behavior
- actual behavior
- inline screenshot attachment or saved filepath (only if you intentionally archived one)
- whether issue reproduces after reload/rebuild
