# Memory: Mobile Push-Drawer Architecture Complete

*Stored: 2026-03-18T00:04:13.652184847+00:00*

---

# Mobile Push-Drawer Navigation — Architecture & Implementation (2026-03-18)

## Feature Summary

Poly now has a professional mobile push-drawer UI pattern where swiping from screen edges (or tapping burger buttons) pushes the center chat view aside to reveal left/right navigation panels. This replaces the old fixed-overlay model with a more modern, app-like experience.

## Implementation Location

### Core Files Modified
- `/crates/core/src/ui/split_shell.rs` — Shared split-container wrapper
- `/crates/core/src/ui/main_layout.rs` — Removed old floating button rendering
- `/crates/core/src/ui/account/common/chat_view.rs` — Right-wing detection
- `/crates/core/assets/styling/mobile-shell.css` — Mobile-only CSS rules
- `/crates/core/assets/scripts/mobile_drawer_runtime.js` — Touch tracking + state

### Key Components

**Left Wing (Navigation Drawer)**
- Slides in from left edge
- Contains server/account sidebar + channel list
- Controlled by `SplitMenuShell` burger button or edge swipe
- Pushes center stage ~250px right
- Close/open via toggle button or snap threshold

**Right Wing (Members Panel)**
- Slides in from right edge
- Contains server members, contacts, or tools
- Controlled by chat header burger button or edge swipe  
- Pushes chat ~354px left
- Replaces that space showing member list

**Center Stage (Chat Content)**
- Always the primary view
- Responds to left/right offsets via CSS left property
- Z-index 470 (above side panels when closed)
- Smooth 220ms transitions

## UX Features

### Visual Polish
- **Burger buttons**: Plain text icons (☰), no border/background, opacity 0.8 hover to 1.0
- **Title row**: Single-line layout "#channel  🧪 Demo" with burgers at edges
- **Depth gradient**: Inset box-shadow on side panels showing "behind content" effect
- **Smooth animations**: 220ms ease transitions on all stage/panel movements
- **Z-index stacking**: Side panels (z:1) → content (z:2) → stage (z:470)

### Touch Gestures
- **Edge detection**: <24px from left or >window.innerWidth-24px from right to start drag
- **Drag calculation**: Progress = (touchX - startX) / revealDistance
- **Snap threshold**: 20% progress triggers open/close
- **Vertical scroll protection**: Must have horizontal movement >8px + pass ratio check
- **Momentum**: No inertia, snaps to final state on touchend

### State Management
- **CSS Custom Properties**:
  - `--poly-mobile-left-progress: 0-1` (0=closed, 1=open)
  - `--poly-mobile-right-progress: 0-1` (0=closed, 1=open)
  - `--poly-mobile-left-offset-px: 0 to ~250px` (stage push)
  - `--poly-mobile-right-offset-px: 0 to ~-354px` (chat push)
  - `--poly-mobile-right-panel-offset-px: -354px to 0px` (panel reveal)

- **CSS Classes**:
  - `.poly-mobile-runtime-active` (mobile UX mode active)
  - `.poly-mobile-left-wing-open` (left drawer open)
  - `.poly-mobile-right-wing-open` (right drawer open)
  - `.poly-mobile-left-wing-dragging` (left drag in progress)
  - `.poly-mobile-right-wing-dragging` (right drag in progress)

## CSS Architecture

### Mobile Breakpoint
- Triggered when `window.innerWidth <= 640` OR `.poly-force-mobile` class
- Applies cascading CSS inside `.poly-app.poly-mobile-runtime-active` selector
- Does NOT affect desktop layout (>640px)

### Key Rules
```css
/* Stage positioning */
.poly-split-content {
    position: relative;
    left: var(--poly-mobile-left-offset-px);
    transition: left 0.22s ease, box-shadow 0.22s ease;
}

/* Right panel reveal */
.chat-side-column {
    position: fixed;
    right: var(--poly-mobile-right-panel-offset-px);
    transition: right 0.22s ease, box-shadow 0.22s ease;
}

/* Depth gradient */
.poly-mobile-right-wing-open .chat-side-column {
    box-shadow: inset 8px 0 24px rgb(0 0 0 / 32%), 
                inset 16px 0 32px rgb(0 0 0 / 16%);
}
```

## JavaScript Runtime

### Touch Event Handlers (350+ lines)
Located in `/crates/core/assets/scripts/mobile_drawer_runtime.js`

**touchstart**
- Detects edge proximity (left/right edge zones)
- Initializes tracking object with side, startX, startY, reveal distance
- Returns early if touch is not at edge

**touchmove**
- Calculates drag distance and direction
- Confirms horizontal movement (filters vertical scroll)
- Updates CSS vars to animate stages/panels
- Sets dragging class to remove transitions

**touchend/touchcancel**
- Reads final progress value
- Snaps open if >20%, closed otherwise
- Removes dragging classes, triggering animations
- Calls toggle/open/close handler functions

### State Functions
- `setLeftProgress(root, progress)`: Updates left stage offset + class
- `setRightProgress(root, progress)`: Updates right panel + chat offset + classes
- `applyStageTransforms(root)`: Writes inline left styles to elements
- `window.__polySetMobileDrawerOpen(open)`: Toggle left drawer
- `window.__polySetMobileRightWingOpen(open)`: Toggle right drawer

## Browser Verification (2026-03-18)

### Mobile View (393×852 CSS px)
✅ Closed state: Clean single-pane chat view
✅ Left wing open: Stage pushed +250px right, left drawer visible
✅ Right wing open: Chat pushed -354px left, members panel visible
✅ Drag gestures: Respond correctly to edge swipes
✅ Snap threshold: 20% progress opens/closes reliably
✅ Visual polish: Gradient shadow on member panel when open
✅ No overlaps: Correct z-index layering
✅ Smooth animations: 220ms transitions work

### Desktop View (1920×1080 CSS px)
✅ Four-pane layout completely unchanged
✅ Side panels (z:450) visible on right
✅ No mobile-specific styling applies
✅ Normal fixed right panel, static layout

## Testing Checklist

**Before shipping to production:**
- [ ] Drag from left edge → opens left drawer
- [ ] Drag from right edge → opens right drawer  
- [ ] Swipe 20%+ → snap open
- [ ] Swipe <20% → snap closed
- [ ] Click burger buttons → open/close
- [ ] Opening left drawer → closes right drawer (mutually exclusive)
- [ ] Gradient shadow visible when right drawer open
- [ ] Title row single-line, no text wrap
- [ ] Desktop layout unchanged (test on 1920×1080)
- [ ] Mobile (393×852) renders cleanly
- [ ] iPad/tablet (split view) renders correctly
- [ ] Chat messages readable in all states

## Future Enhancements

1. **Right-wing icon**: Consider changing from ☰ to 👥 or ⚙ to better indicate "members"
2. **Gesture speed**: Could add momentum flinging on touchend (currently snaps only)
3. **Haptic feedback**: Vibration on snap-open on supported devices
4. **Swipe velocity**: Track speed to change threshold or animate faster
5. **Landscape mode**: Handle width >640px on mobile devices (may need orientation handling)
