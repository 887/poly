# Memory: Shared split shell + mobile right wing

*Stored: 2026-03-16T23:10:21.474446185+00:00*

---

# Shared split shell and mobile right wing (2026-03-16)

- Added `crates/core/src/ui/split_shell.rs` with `SplitMenuShell`, now used by DM/server route shells, app settings, search, account settings, and server settings so left split-menu pages share one structural wrapper.
- Moved `MainLayout` browser runtime JS out of inline Rust strings into:
  - `crates/core/assets/scripts/mobile_drawer_runtime.js`
  - `crates/core/assets/scripts/drag_bridge_runtime.js`
- `MainLayout` now has an explicit WASM browser-runtime path plus a native stub path instead of assuming DOM scripting exists for every renderer.
- Under 640px / mobile emulation, `.account-server-bar` now stays offscreen until the left drawer opens; verified closed/open geometry on account settings and app settings.
- The chat side rail (`.chat-side-column`) is now a true mobile right-side overlay wing controlled by `.poly-mobile-right-wing-open` instead of stacking below chat.
- Mobile route changes now clear both right-side member/contact visibility flags and close the visual right wing, preventing the old bug where a members panel stayed open after navigating to another chat on mobile.
- Verified live in poly-web at desktop + mobile widths with screenshots:
  - `devtools-screenshots/web-settings-shared-split-2026-03-16.png`
  - `devtools-screenshots/web-mobile-account-settings-closed-2026-03-16.png`
  - `devtools-screenshots/web-mobile-account-settings-open-2026-03-16.png`
  - `devtools-screenshots/web-mobile-server-members-open-2026-03-16.png`

