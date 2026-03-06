# Message Interactions Plan
**Created:** 2026-03-06  
**Status: ✅ COMPLETE**

---

## Feature Spec (from screenshots)

### For **own** messages (hover)
- [x] ✏️ Edit button → inline edit mode (textarea replacing message text)
- [x] 🗑️ Delete button (removes message locally)
- [x] Still keep 😀+ reaction button

### For **other people's** messages (hover)
- [x] ↩️ Reply button (stub)
- [x] ➡️ Forward button (stub)
- [x] Still keep 😀+ reaction button

### Right-click context menu (all messages)
- [x] Quick reactions row: 👍 ✅ ⚖️ 🔞 + more
- [x] Add Reaction >
- [x] Reply ↩
- [x] Forward ➡
- [x] Copy Text (JS clipboard)
- [x] Apps >
- [x] Mark Unread
- [x] Copy Message Link
- [x] Speak Message
- [x] Report Message (red, only for others' messages) ✓
- [x] Delete (red, only for own messages) ✓
- [x] Copy Message ID (JS clipboard)

---

## Implementation Steps

### Step 1 — Self-user identification ✅
- [x] 1a. In `ChatView`, look up active_account_id → `client_manager.sessions[id].user.id` → `self_user_id: String`
- [x] 1b. Compute `is_own: bool` for each message: `msg.author.id == self_user_id`

### Step 2 — Inline edit state signals ✅
- [x] 2a. Add `editing_msg_id: Signal<Option<String>>` 
- [x] 2b. Add `edit_draft: Signal<String>`

### Step 3 — Differentiated hover action bars ✅
- [x] 3a. Own messages hover bar: 😀+ | ✏️ | 🗑️
- [x] 3b. Other messages hover bar: 😀+ | ↩️ | ➡️

### Step 4 — Inline edit mode rendering ✅
- [x] 4a. When `editing_msg_id == Some(msg_id)`, render textarea instead of `MessageContentView`
- [x] 4b. Save on Enter / Save button → update message in `chat_data.messages`, `edited = true`
- [x] 4c. Cancel on Escape / Cancel button → clear edit state

### Step 5 — Right-click context menu ✅
- [x] 5a. Added `MsgContextMenu` state struct (x, y, message info, is_own)
- [x] 5b. Added `msg_context_menu: Signal<Option<MsgContextMenu>>` to `ChatView`
- [x] 5c. Added `oncontextmenu` handler to each message div
- [x] 5d. `MsgContextMenuOverlay` component renders fixed-position menu
- [x] 5e. Click outside (backdrop) closes menu
- [x] 5f. Copy Text action uses JS clipboard
- [x] 5g. Copy Message ID uses JS clipboard

### Step 6 — CSS ✅
- [x] 6a. `.msg-action-btn-danger` for delete button hover
- [x] 6b. `.msg-context-menu`, `.context-menu` fixed overlay
- [x] 6c. `.msg-context-quick-reactions`, `.msg-context-quick-reaction-btn`
- [x] 6d. `.message-inline-edit`, `.message-edit-input`, `.message-edit-hint`, `.message-edit-link-btn`

### Step 7 — Visual verification ✅
- [x] 7a. Screenshot: hover own message → see 😀+ ✏️ 🗑️ buttons ✓
- [x] 7b. Screenshot: hover other's message → see 😀+ ↩️ ➡️ buttons ✓
- [x] 7c. Screenshot: right-click message → context menu appears ✓
- [x] 7d. Screenshot: click ✏️ → inline textarea appears with pre-filled text ✓
- [x] 7e. Screenshot: type new text, save → message updated with (edited) indicator ✓

---

## Feature Spec (from screenshots)

### For **own** messages (hover)
- [ ] ✏️ Edit button → inline edit mode (textarea replacing message text)
- [ ] 🗑️ Delete button (stub — just removes locally for now)
- [ ] Still keep 😀+ reaction button

### For **other people's** messages (hover)
- [ ] ↩️ Reply button (stub)
- [ ] ➡️ Forward button (stub)
- [ ] Still keep 😀+ reaction button

### Right-click context menu (all messages)
- [ ] Quick reactions row: 👍 ✅ ⚖️ 🔞 + more
- [ ] Add Reaction >
- [ ] Reply ↩
- [ ] Forward ➡
- [ ] Copy Text 
- [ ] Apps >
- [ ] Mark Unread
- [ ] Copy Message Link
- [ ] Speak Message
- [ ] Report Message (red, only for others' messages)
- [ ] Copy Message ID

---

## Implementation Steps

### Step 1 — Self-user identification
- [ ] 1a. In `ChatView`, look up active_account_id → `client_manager.sessions[id].user.id` → `self_user_id: String`
- [ ] 1b. Compute `is_own: bool` for each message: `msg.author.id == self_user_id`

### Step 2 — Inline edit state signals
- [ ] 2a. Add `editing_msg_id: Signal<Option<String>>` 
- [ ] 2b. Add `edit_draft: Signal<String>`

### Step 3 — Differentiated hover action bars
- [ ] 3a. Own messages hover bar: 😀+ | ✏️ | 🗑️
- [ ] 3b. Other messages hover bar: 😀+ | ↩️ | ➡️

### Step 4 — Inline edit mode rendering
- [ ] 4a. When `editing_msg_id == Some(msg_id)`, render textarea instead of `MessageContentView`
- [ ] 4b. Save on Enter / Save button → update message in `chat_data.messages`
- [ ] 4c. Cancel on Escape / Cancel button → clear edit state

### Step 5 — Right-click context menu
- [ ] 5a. Add `MsgContextMenu` state struct (x, y, message info, is_own)
- [ ] 5b. Add `msg_context_menu: Signal<Option<MsgContextMenu>>` to `ChatView`
- [ ] 5c. Add `oncontextmenu` handler to each message div
- [ ] 5d. `MsgContextMenuOverlay` component renders fixed-position menu
- [ ] 5e. Click outside or item click → close menu
- [ ] 5f. Wire Copy Text action (JS clipboard)
- [ ] 5g. Wire Copy Message ID action (JS clipboard)

### Step 6 — CSS
- [ ] 6a. `.message-actions` gains new buttons (edit, delete, reply, forward)
- [ ] 6b. `.msg-context-menu` fixed overlay — dark background, border-radius, etc.
- [ ] 6c. `.msg-context-menu-item`, `.msg-context-menu-quick-reactions`, `.msg-context-menu-separator`
- [ ] 6d. `.message-inline-edit` textarea + Save/Cancel buttons

### Step 7 — Visual verification (web devtools MCP)
- [ ] 7a. Screenshot: hover own message → see ✏️ 🗑️ buttons
- [ ] 7b. Screenshot: hover other's message → see ↩️ ➡️ buttons  
- [ ] 7c. Screenshot: right-click message → context menu appears
- [ ] 7d. Screenshot: click ✏️ → inline textarea appears
- [ ] 7e. Screenshot: type new text, save → message updated

---

## Lint/Build policy
- cargo cranky --workspace must pass (zero warns)
- WASM check must pass
- No `#[allow(...)]` suppressions
