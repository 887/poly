# Phase 2.16 — Chat Composer Improvements & Slash Commands

**Status:** ✅ Complete  
**Scope:** Chat view UX fixes + Discord-style slash command autocomplete

---

## Goals

Bring the chat composer closer to Discord-quality UX with four targeted fixes plus a 
brand-new slash command system:

1. **Member list for DMs** — DM channels had no member list toggle and the sidebar was hardcoded `false`. Now shows a contact info panel.
2. **`+` button inside the input shell** — Moved inside the rounded compose box, matching Discord's layout.
3. **`send_message` DM fix** — Silently failed for DM channels (early-returned when `selected_server` was `None`).
4. **Slash command autocomplete** — Full Discord-style `/command` popup with arrow/Tab/Enter navigation.

---

## Checklist

### DM Member List Panel
- [x] Fix `member_list_visible` in `chat_view.rs`: DM channels now use `dm_right_sidebar_visible` instead of hardcoded `false`
- [x] Add 👤 toggle button in header for DM channels (`chat-toggle-contact` i18n key)
- [x] Add `DmContactPanel` component showing avatar, display name, presence, backend badge
- [x] Add `dm-contact-panel`, `.dm-contact-avatar-wrap`, `.dm-contact-name`, etc. CSS
- [x] Add `chat-toggle-contact` i18n key to all 4 locales
- [x] Add `dm-contact-panel-title` and `dm-contact-not-found` i18n keys to all 4 locales
- [x] Add `presence-online/away/dnd/offline` i18n keys to all 4 locales

### `+` Button Inside Input Shell
- [x] Move `button.composer-upload-btn` from outside `div.message-input-shell` to be its first child
- [x] Add `div.message-input-text-area` wrapper (flex: 1, column) around textarea + toolbar
- [x] Update `.message-input-shell` CSS to `flex-direction: row; align-items: flex-end;`
- [x] Update `.composer-upload-btn` CSS to `flex-shrink: 0; align-self: flex-end;`
- [x] Add `.message-input-text-area` CSS

### `send_message` DM Fix
- [x] Replace early-return-on-missing-server with fallback: if `server_id` is None, look up backend via `active_account_id`
- [x] DM messages now actually send via `tracing::warn!` on no-backend instead of silent discard

### Slash Command System — `poly-client` types
- [x] Add `CommandScope` enum (`Global`, `Channel`, `DirectMessage`) to `clients/client/src/types.rs`
- [x] Add `ChatCommand` struct (`name`, `description`, `provider`, `is_builtin`, `usage`, `scope`)
- [x] Add `get_channel_commands(&self, channel_id: &str) -> ClientResult<Vec<ChatCommand>>` to `ClientBackend` trait with default `Ok(vec![])`

### Slash Command System — Demo Backend
- [x] Add `demo_channel_commands(channel_id: &str)` to `clients/demo/src/data.rs` (8 commands: MusicCat play/skip/queue, ModBot ban/kick/timeout, AI Bot changelog/gif)
- [x] Implement `get_channel_commands` for `DemoClient` in `clients/demo/src/lib.rs`
- [x] Implement `get_channel_commands` for `DemoClient2` in `clients/demo/src/lib.rs`

### Slash Command System — UI
- [x] Add `BUILTIN_COMMANDS` constant (shrug, tableflip, unflip, me, spoiler, tts, nick, msg)
- [x] Add `filtered_slash_commands(query, backend_cmds) -> Vec<ChatCommand>` helper
- [x] Add `apply_builtin_command(text) -> Option<String>` for shrug/tableflip/unflip/me/spoiler transforms
- [x] Add signals: `command_suggestions`, `active_command_idx`, `show_command_popup`
- [x] Add `use_effect` to pre-load backend commands when channel changes
- [x] Modify `oninput` to detect `/command` and show/hide popup
- [x] Modify `onkeydown` for Arrow Up/Down navigation, Tab/Enter to select, Escape to close
- [x] Apply `apply_builtin_command()` transform before sending on Enter
- [x] Add `SlashCommandPopup` Dioxus component (scrollable list, highlighted active item, click to select)
- [x] Position popup via `.message-input-area { position: relative }` + popup `position: absolute; bottom: calc(100% + 4px)`
- [x] Add slash command popup CSS (`.slash-command-popup`, `.slash-command-item`, `.selected`, `.slash-command-name`, `.slash-command-desc`, `.slash-command-provider`, etc.)

### Quality
- [x] `cargo cranky --workspace` — zero warnings/errors
- [x] `cargo check -p poly-web --target wasm32-unknown-unknown` — WASM compatible
- [x] `cargo fmt --all` — formatted

---

## Architecture Notes

### `get_channel_commands` Default Implementation
All backends that don't implement slash commands return `Ok(Vec::new())` via the default impl. This means no existing backend breaks.

### Popup Position Strategy
The slash command popup uses `position: absolute` relative to `.message-input-area` (which gains `position: relative`). The popup renders as a flex sibling before `.message-input-row` in the DOM order. CSS uses `bottom: calc(100% + 4px)` to place it above the input area.

### Built-in Command Transforms
`apply_builtin_command()` is called on the raw input text in the `onkeydown` Enter handler BEFORE passing to `send_message`. The current transforms:
- `/shrug` → `¯\_(ツ)_/¯`
- `/tableflip` → `(╯°□°）╯︵ ┻━┻`
- `/unflip` → `┬─┬ ノ( ゜-゜ノ)`
- `/me <action>` → `*action*`
- `/spoiler <text>` → `||text||`

Non-matching slash commands (like `/play song-name`) are sent as-is, letting the backend interpret them.

### DM Contact Panel
Looks up the `DmChannel` from `chat_data.dm_channels` by `channel_id`. Shows:
- Large circular avatar (or initial fallback)
- Status dot (presence indicator)
- Display name
- Presence label (Online/Away/DND/Offline)
- Backend badge

---

## Session Summary

All 5 user requests fully implemented:
1. ✅ DM contact panel with toggle button (👤)
2. ✅ `+` button moved inside the rounded input shell  
3. ✅ Shift+Enter for multi-line was already implemented correctly — no change needed
4. ✅ Full slash command system with popup, keyboard nav, built-in transforms
5. ✅ Phase 2.16 plan doc created

Bonus fix: `send_message` DM bug (silent failure) resolved.
