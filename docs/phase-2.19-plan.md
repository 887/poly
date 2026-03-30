# Phase 2.19 Plan — Route-backed Fullscreen Profile & Media Viewer

> **Created:** 2026-03-20  
> **Status:** In Progress (incremental, interruption-safe)

## Goal

Implement Discord-like fullscreen UX for profile and message images with route/back-button semantics, Escape dismissal, toolbar actions, and multi-image navigation. Also align demo data with real DM behavior (DMs can exist for non-friends).

## Rules for this phase

- Work strictly **item-by-item** (no giant oneshot).
- After each item:
  1. compile/lint/WASM checks,
  2. poly-web visual verification,
  3. Memory MCP checkpoint note.
- Keep route-driven behavior so browser back works naturally.

## Checklist

### 0) Profile fullscreen Escape close
- [x] Add Escape key handling for fullscreen profile modal.
- [x] Verify host + WASM + cranky.
- [x] Commit separately.

**Done commit:** `5acc0ff` (`core/ui: close profile modal on Escape`)

---

### 1) Fullscreen media viewer route scaffold (single image)
- [x] Add route shape for fullscreen message media viewer (account-scoped).
- [x] Add shared viewer state model (image list, active index, origin context).
- [x] Ensure route open/close integrates with browser back.

**Done:** `DmMediaViewerRoute` + `ServerMediaViewerRoute` in `routes.rs`; `MessageMediaViewerOverlay` component in `crates/core/src/ui/account/common/media_viewer.rs`.

### 2) Hook message image click -> viewer route
- [x] Wire image attachments in chat messages to open viewer route.
- [x] Ensure this works from server channels, DMs, and group DMs.

**Done:** `AttachmentsView` in `chat_view.rs` navigates to viewer route on image click.

### 3) Viewer UI v1 (single image)
- [x] Backdrop + centered media with clean controls.
- [x] Escape to close viewer.
- [x] Toolbar: zoom, next/prev placeholders, download, open in browser.
- [x] Overflow menu (`...`) for secondary actions.

**Done:** Full toolbar with zoom in/out, download, open-in-browser; Escape via JS keydown listener; backdrop click closes.

### 4) Multi-image support
- [ ] Support multiple images per message.
- [ ] Left/right edge navigation controls.
- [ ] Bottom thumbnail strip with active preview.
- [ ] Keyboard left/right navigation.

**Note:** Explicitly deferred — code comment in `media_viewer.rs` marks this as a follow-up. Single-image viewer is complete.

### 5) Viewer route semantics + robust navigation
- [x] Deep-link route opens correct media.
- [x] Back button returns to exact previous chat context.
- [x] Close button and Escape use same state transition path.

**Done:** Both routes render `ChatView` + overlay together; all close paths call `nav.go_back()`.

### 6) Demo data + DM parity
- [ ] Add non-friend DM examples in demo data.
- [ ] Ensure DM list and friends list can diverge naturally.
- [ ] Add demo messages containing multiple images for viewer testing.

### 7) Verification + docs closeout
- [ ] `cargo check -p poly-core`
- [ ] `cargo check -p poly-web --target wasm32-unknown-unknown`
- [ ] `cargo cranky -p poly-core`
- [ ] Poly-web visual pass with inline screenshots for each checklist item.
- [ ] Append session notes and outcomes.

## Session Notes

### 2026-03-20
- Phase initialized.
- Completed item 0 first (per user instruction): Escape closes profile modal.
- Added initial route scaffold: `Route::MediaViewerRoute`
  (`/:backend/:instance_id/:account_id/media/:channel_id/:message_id/:attachment_index`)
  plus `sync_route_to_app_state` wiring and placeholder route component.
- Next item: wire image attachment clicks to navigate into the media route.
