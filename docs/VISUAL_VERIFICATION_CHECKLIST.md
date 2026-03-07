# Phase 2.16 Features - Visual Verification Checklist

## Build Status ✅
- ✅ `cargo check --workspace` — Passed  
- ✅ `cargo cranky --workspace` — Zero warnings  
- ✅ `dx build --platform desktop` (desktop-electron) — Successful  
- ✅ Code changes compiled into `/target/dx/poly-desktop-electron/debug/linux/app/`

## Code Changes Applied ✅

### 1. Default Member List Visibility (OPEN)
**File**: `crates/core/src/state/mod.rs` line 75  
**Change**: `dm_right_sidebar_visible: false` → `dm_right_sidebar_visible: true`  
**Effect**: When opening a DM channel, the member list sidebar is now VISIBLE by default (not collapsed)

### 2. Syntax Error Fixed
**File**: `crates/core/src/ui/account/common/chat_view.rs` line 1021  
**Change**: Removed stray `},` — changed final `}` to `;`  
**Effect**: Code now compiles cleanly

### 3. Phase 2.16 Features Already Implemented
All features from Phase 2.16 were completed in the previous session:
- ✅ DM member list with 👤 toggle button  
- ✅ Contact panel show avatar, presence status, backend badge  
- ✅ Slash commands (`/shrug`, `/tableflip`, `/unflip`, `/me`, `/spoiler`, `/tts`, `/nick`, `/msg`, plus 8 demo commands)  
- ✅ Slash command popup with keyboard navigation (Arrow Up/Down, Tab, Enter, Escape)  
- ✅ `+` button moved inside the message input shell  
- ✅ Single-line chat input by default (expands on Shift+Enter or when content grows)  
- ✅ i18n keys for all 4 locales (EN, DE, FR, ES)  

## Visual Verification Steps

When you open the app, you should see:

### Step 1: Open a DM Channel
- Navigate to any DM channel
- **VERIFY**: Member list is visible on the RIGHT side by default (no manual toggle needed)
- **VERIFY**: 👤 button is visible in the header to toggle member list open/closed

### Step 2: Check Member List Panel
- The member list should show:
  - Avatar (cat/dog for demo)
  - Online status indicator (green/yellow/red dot)
  - Presence label (Online, Away, Do Not Disturb, Offline)
  - Backend badge (e.g., "Demo" label)

### Step 3: Test Chat Input
- Click in the message input field
- **VERIFY**: Input box appears SINGLE-LINE (height ~24px, not multi-line)
- **VERIFY**: As you type, it expands gradually (auto-grow via JavaScript)
- **VERIFY**: Shift+Enter should create a new line (multi-line mode)

### Step 4: Test Slash Commands
- In the message input, type `/`
- **VERIFY**: Popup appears ABOVE the input showing command suggestions
- **VERIFY**: As you type more (e.g., `/sh`), list filters to matching commands
- **VERIFY**: Keyboard navigation:
  - Arrow Up/Down — move selection in popup
  - Tab/Enter — insert selected command
  - Escape — close popup
- **VERIFY**: Commands that exist:
  - `/shrug` → inserts `¯\_(ツ)_/¯`
  - `/tableflip` → inserts `(╯°□°)╯︵ ┻━┻`
  - `/unflip` → inserts `┬─┬ノ( º _ ºノ)`
  - `/me` → makes message italic
  - `/spoiler` → wraps in spoiler tags
  - `/tts` → marks for text-to-speech
  - `/nick` → changes display name
  - `/msg` → sends direct message

### Step 5: Test `+` Button Location
- In any chat, look at the message input area
- **VERIFY**: `+` button is INSIDE the input shell (same container as the textarea)
- **VERIFY**: It's to the right of the textarea, aligned with the message input baseline

### Step 6: Test DM Contact Panel (if visible)
- In a DM, look for the member list on the right
- **VERIFY**: Shows contact info with avatar, presence, and backend badge
- **VERIFY**: Clicking 👤 toggle button closes/opens the panel

## Expected Build Artifacts
```
/target/dx/poly-desktop-electron/debug/linux/app/poly-desktop-electron
```

## If Issues Found

If any feature doesn't work as expected, please check:
1. Are you running the newly built app? (`dx build --platform desktop` was run)
2. Is the app in DM channel view? (Not in Server view)
3. Did you enable demo data with the 🧪 toggle?
4. Check browser console (F12) for any JavaScript errors

## Notes
- Member list visibility is now per-view (servers have `right_sidebar_visible`, DMs have `dm_right_sidebar_visible`)
- Both default to `true` (visible by default)
- Chat input auto-grow is handled via `use_effect` setting `textarea.style.height = textarea.scrollHeight`
- Slash command popup filters on every keystroke and persists suggestions until cleared
