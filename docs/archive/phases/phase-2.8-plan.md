# Phase 2.8 Plan — Mobile Layout & Swipeable Panels

> **Status:** ⬜ Not Started  
> **Parent:** [Phase 2 Plan](phase-2-plan.md)  
> **Depends On:** Phase 2.7 ✅  
> **Last Updated:** 2026-03-02

---

## Overview

This phase delivers a mobile-first layout with swipeable panels, responsive
breakpoints, and touch-friendly interactions. The goal is a great experience
on Android and iOS (via Dioxus mobile) **and** on narrow browser viewports
(phone-sized web).

Items were originally scoped as 2.6.11 and deferred per user request to keep
Phase 2.6 focused on desktop UI polish.

---

## 2.8.1 Swipeable Panel Layout

Mobile layout uses **3 swipeable panels** on a horizontal scroll snap container:

| Panel | Content |
|-------|---------|
| Left  | Server sidebar + channel list |
| Center | Chat view (default visible) |
| Right | User sidebar / contextual panel |

- [ ] **2.8.1.1** Add `view-mobile.css` with horizontal scroll-snap layout
- [ ] **2.8.1.2** JS/RSX swipe gesture handler: `touchstart`/`touchend` to advance panels
- [ ] **2.8.1.3** Panel indicator dots (bottom of screen, shows which panel is active)
- [ ] **2.8.1.4** Programmatic panel navigation from header back/forward buttons
- [ ] **2.8.1.5** Selecting a channel auto-advances to center panel
- [ ] **2.8.1.6** Opening user sidebar auto-advances to right panel

---

## 2.8.2 Responsive Breakpoints

- [ ] **2.8.2.1** Define CSS breakpoints: `≥1024px` = desktop, `768–1023px` = tablet, `<768px` = mobile
- [ ] **2.8.2.2** Tablet layout: server sidebar always visible, channel list togglable
- [ ] **2.8.2.3** Mobile layout: server sidebar in left panel (hidden by default), no persistent sidebars
- [ ] **2.8.2.4** Header adapts: show hamburger menu icon on mobile to open left panel
- [ ] **2.8.2.5** Message input adapts: slightly larger touch targets on mobile

---

## 2.8.3 Touch-Friendly Interaction Targets

- [ ] **2.8.3.1** All interactive elements: minimum `44×44px` touch target (Apple HIG)
- [ ] **2.8.3.2** Channel items: taller rows on mobile (`48px` min height)
- [ ] **2.8.3.3** Message action buttons (reaction, reply, etc.) visible on long-press (mobile) vs hover (desktop)
- [ ] **2.8.3.4** Bottom input toolbar: larger send button on mobile
- [ ] **2.8.3.5** Settings list: full-width tappable rows

---

## 2.8.4 Android/iOS Entry Points

- [ ] **2.8.4.1** Verify `apps/android/` builds with Dioxus mobile target
- [ ] **2.8.4.2** Verify `apps/ios/` builds with Dioxus mobile target
- [ ] **2.8.4.3** Status bar / safe area insets handled correctly
- [ ] **2.8.4.4** Keyboard avoidance: input area scrolls above keyboard on mobile

---

## Completion Criteria

- [ ] Mobile layout with 3 swipeable panels works in a browser at `375px` width
- [ ] Breakpoints tested at 375px, 768px, 1024px viewport widths
- [ ] All touch targets ≥ 44×44px verified
- [ ] Android and iOS Dioxus builds succeed
- [ ] `cargo cranky --workspace` — zero warnings
- [ ] `cargo check -p poly-web --target wasm32-unknown-unknown` — passes
