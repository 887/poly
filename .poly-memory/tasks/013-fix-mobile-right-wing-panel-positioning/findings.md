# Research Findings — Task: Fix mobile right-wing panel positioning

*Auto-updated by poly-memory-mcp. Add findings via CLI or MCP tool.*

---


## Finding 2026-03-17T23:49:43Z

# Right Wing Panel Issue — Root Cause (2026-03-18)

## Problem
Members panel (`.chat-side-column`) is **always visible ON-SCREEN** overlapping chat messages, even when it should be closed/hidden.

## Root Cause
CSS has hardcoded `right: 0` instead of responding to the `--poly-mobile-right-progress` runtime state.

Current CSS (WRONG):
```css
.chat-side-column {
    position: fixed;
    right: 0;        /* <- HARDCODED: always on-screen */
    z-index: 1;
    width: var(--poly-mobile-right-reveal-px);
}
```

## Expected Behavior
- **CLOSED** (progress=0): panel OFF-SCREEN to the right (not visible)
- **OPEN** (progress=1): panel ON-SCREEN fully visible
- When opening, `.chat-main-column` pushes left via `left: var(--poly-mobile-right-offset-px)`

## Fix Required
Add a new CSS variable for the side panel's right positioning:

```css
.chat-side-column {
    right: var(--poly-mobile-right-panel-offset-px);  /* responsive to state */
    ...
}
```

Runtime should set (in `setRightProgress()`):
```javascript
const revealPx = rightRevealPx(root);
const panelOffset = -1 * (1 - progress) * revealPx;  // negative to push off-screen
root.style.setProperty('--poly-mobile-right-panel-offset-px', `${panelOffset}px`);
```

Result:
- progress=1: offset = `-1 * 0 * 320 = 0px` → `right: 0` ✓ on-screen
- progress=0: offset = `-1 * 1 * 320 = -320px` → `right: -320px` ✓ off-screen

## Browser Evidence
- Computed style: `left: 39.3125px` (why is left being used at all?)
- z-index correct (1, behind content at z-470)
- Members panel visually overlaps chat messages  
- Clicking right-wing toggle doesn't move it

---


## Finding 2026-03-17T23:59:59Z

# Mobile UX Issues to Fix (2026-03-18)

## Issues Identified

1. **Left Burger Button**: Has border + background styling (looks like button). Should be plain icon.
   - Current: border, background, box-shadow, border-radius
   - Need: Remove all styling, just show plain ☰ symbol

2. **Chat Title Row**: Wrapping to 2 rows due to padding/layout
   - Currently: "# general  🧪" on top row, left/right burgers below
   - Issue: Flex-wrap and align-items: flex-start causing wrap
   - Need: Keep title and demo label on same line, burgers positioned absolutely

3. **Right Burger Button**: Missing styles, needs:
   - Plain styling (like left, no border)
   - Different icon (use 👥 or ⚙ instead of ☰ to indicate server members)

4. **Right Wing Panel**: Missing gradient/shadow on left edge when open
   - Current: z-index correct, positioning correct, but no visual depth
   - Need: Add inset shadow or gradient to create "behind content" visual effect

5. **Drag Gestures**: Not implemented
   - Need: Touch tracking for left/right edges
   - Swipe from edge should drag the stage
   - 20% threshold to snap open/closed

## CSS Selectors to Update
- `.poly-app.poly-mobile-runtime-active .poly-mobile-left-wing-toggle` — simplify styling
- `.poly-app.poly-mobile-runtime-active .poly-mobile-right-wing-toggle` — add styling
- `.poly-app.poly-mobile-runtime-active .chat-header` — fix flex layout to prevent wrap
- `.poly-app.poly-mobile-runtime-active .chat-side-column` — add gradient/shadow on left edge
- `.poly-app.poly-mobile-runtime-active.poly-mobile-right-wing-open .chat-side-column` — add visual depth

---
