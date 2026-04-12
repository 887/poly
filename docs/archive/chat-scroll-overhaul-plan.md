# Chat Scroll Overhaul Plan — Discord-Style Anchoring + Jump to Present

> **Created:** 2026-03-22
> **Status:** In Progress

## Goal

Fix the chat message list to behave like Discord:

1. **Messages anchor to the bottom** — when there are only a few messages, they appear at the
   bottom of the viewport (near the input), not floating at the top with a huge empty space.
2. **"Jump to Present" button** — appears when the user has scrolled up far enough that they are
   not at the latest messages. Matches Discord's "Jump to Present" pill at the bottom of the chat.
3. **"You're Viewing Older Messages"** banner (already partially implemented as unread banner).
   Enhanced to also show when `has_more_after = true` (i.e. not at live tail regardless of read state).
4. **Auto-scroll on new messages** — when the user is at the bottom (at the live tail), incoming
   messages auto-scroll down. When scrolled up, they do NOT auto-scroll — instead the Jump to
   Present button shows with a new-message count.
5. **Unread red line persists** — the divider line stays after the user marks read / scrolls past
   the unread boundary. It only disappears when the channel is switched.
6. **"X new messages since HH:MM" bar dismisses** when the user scrolls past the unread divider
   (just like Discord — once you've scrolled past it, the notification bar disappears, but the
   red line stays).

## Rules for This Phase

- Work strictly item-by-item. After each item: compile/WASM check, visual verification.
- Do NOT break the existing 500+ message smooth scroll loading (before/after spacers, history paging).
- All UI strings through i18n (`.ftl` files).
- `#[rustfmt::skip]` before every `#[component]`.

---

## Implementation Details

### 1. Bottom-anchoring (CSS fix)

**Root cause:** `.message-list-content` uses `min-height: 100%` with `flex-direction: column`.
When there are few messages, they stack from the **top** of the container.

**Fix:** Change `.message-list-content` to use `justify-content: flex-end` (or use
`margin-top: auto` on the first child, or a flex spacer div before the messages).

The cleanest approach: add `justify-content: flex-end` to `.message-list-content`.
This causes the messages to "stick to the bottom" of the container naturally.

**Interaction with history spacers:** The before-spacer (for older unloaded messages) must
still push messages down. With `justify-content: flex-end`, the spacer is at the top and
naturally collapses when empty. The after-spacer (for newer unloaded messages) goes at the
bottom and pushes the viewport down — this is exactly what we want.

**Potential issue:** With `justify-content: flex-end`, `overflow-anchor` behavior may change.
Since we already have `overflow-anchor: none` and JS-based scroll restoration, this should be fine.

### 2. "Jump to Present" Button + "Scrolled Up" Signal

Add a new signal `scrolled_from_bottom: Signal<bool>` that tracks whether the user is
scrolled far enough from the bottom that they should see the Jump to Present button.

Threshold: if `scrollHeight - scrollTop - clientHeight > JUMP_TO_PRESENT_THRESHOLD_PX` (e.g. 200px),
OR if `has_more_after = true` (meaning newer messages exist beyond the loaded window).

The button appears:
- Anchored to the bottom of `.chat-content-column`, above the typing indicator + input.
- Contains: "↓ Jump to Present" or "↓ X new messages" when there are new messages.
- Clicking it: if `has_more_after`, reloads the channel (fresh load from latest), else
  calls `request_scroll_to_bottom()`.

Track `scrolled_from_bottom` inside the existing `spawn_message_list_scroll_work` function,
which already reads scroll metrics on every scroll event.

### 3. Auto-scroll on New Messages

In `demo.rs` `spawn_event_stream_listener`, after appending a new live message:
- If user is at bottom (not `scrolled_from_bottom`): call `request_scroll_to_bottom()`
- If user is scrolled up: increment a `new_messages_since_scroll_up` counter signal

For the `send_message` handler: always scroll to bottom after sending.

### 4. Unread Banner Enhancement

The existing `chat-unread-banner` shows when `unread_count > 0 AND unread_marker_not_on_screen`.

Enhance it to also show a "You're Viewing Older Messages / Jump to Present" bar when
`has_more_after = true` (even if no unread messages). This matches Discord's behavior.

Two distinct modes:
- **Unread mode**: "X new messages since HH:MM" + "Mark as Read" button
- **Historical mode**: "You're viewing older messages" + "Jump to Present" button (when `has_more_after`)

Both hide when the user scrolls to the live tail.

### 5. Red Divider Persistence

Current behavior: the red divider disappears after `mark_channel_as_read()` clears `unread_count`.

New behavior:
- Add `unread_divider_visible: bool` to `ChatHistoryUiState` (separate from `unread_count > 0`).
- Set `unread_divider_visible = true` on initial load when `unread_count > 0`.
- `mark_channel_as_read()` / scrolling past the divider sets `unread_count = 0` (hides banner)
  but does NOT set `unread_divider_visible = false`.
- Only channel switch resets `unread_divider_visible = false`.

This way the red line stays while you read the new messages, then gets pushed up by newer messages.

---

## Checklist

### 0) Plan doc creation
- [x] Create this plan document.

### 1) Bottom-anchoring CSS fix
- [ ] Add `justify-content: flex-end` to `.message-list-content` in `chat.css`.
- [ ] Verify in poly-web: few-message chats show messages at bottom.
- [ ] Verify: 500+ message chat still works, history loading still works.

### 2) `scrolled_from_bottom` signal + scroll tracking
- [ ] Add `scrolled_from_bottom: Signal<bool>` to `ChatViewSignals`.
- [ ] Update `MessageListScrollWorkCtx` to carry it.
- [ ] In `spawn_message_list_scroll_work`: compute and update `scrolled_from_bottom`.
- [ ] Pass through `ChatViewMarkupCtx`.
- [ ] Add `JUMP_TO_PRESENT_THRESHOLD_PX` constant.

### 3) "Jump to Present" button UI
- [ ] Add i18n strings: `chat-jump-to-present`, `chat-viewing-older-messages`.
- [ ] Add CSS for `.chat-jump-to-present` button.
- [ ] Render the button in `render_chat_content_column` when `scrolled_from_bottom || has_more_after`.
- [ ] Wire click: if `has_more_after` → reload channel messages from latest; else `request_scroll_to_bottom()`.
- [ ] Show live new-message count on the button when messages arrived while scrolled up.

### 4) Auto-scroll on new live messages
- [ ] Pass `scrolled_from_bottom` signal into `spawn_event_stream_listener` in `demo.rs`.
- [ ] After pushing live message: if at bottom → `request_scroll_to_bottom()`; else increment counter.
- [ ] Send_message handler: always scroll to bottom.

### 5) Unread divider persistence
- [ ] Add `unread_divider_visible` to `ChatHistoryUiState`.
- [ ] Render divider based on `unread_divider_visible` (not just `unread_count > 0`).
- [ ] `mark_channel_as_read()`: clear `unread_count` but preserve `unread_divider_visible`.
- [ ] Channel switch: reset `unread_divider_visible = false`.

### 6) Verification + docs closeout
- [ ] `cargo check -p poly-core`
- [ ] `cargo check -p poly-web --target wasm32-unknown-unknown`
- [ ] `cargo cranky -p poly-core`
- [ ] Poly-web visual pass: few messages at bottom, jump to present button, auto-scroll, divider.
- [ ] Session notes appended.

---

## Session Notes

### 2026-03-22
- Phase initialized.
- Codebase analyzed: root cause of floating messages is `.message-list-content` lacking
  `justify-content: flex-end`.
- `scrolled_from_bottom` signal needs to be threaded through `MessageListScrollWorkCtx`.
- Existing `render_unread_banner` is the right place to add the "viewing older messages" mode.
- `spawn_event_stream_listener` in `demo.rs` currently does NOT auto-scroll — fix needed.
