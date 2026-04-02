# Chat Scroll & History — Implementation Spec

> Status: **Working** — tagged `chat-ui-working` on 2026-04-02
> Commits: `7e89582f` → `70063cdf` on `main`

---

## Overview

Poly's chat view uses a **column-reverse CSS layout** for the message list. This means:

- `scrollTop = 0` → visual bottom (newest messages, live tail)
- `scrollTop < 0` → scrolled up (older messages)
- `dist_from_bottom = (-scrollTop).max(0)`
- `dist_from_top = (scrollHeight - clientHeight - dist_from_bottom).max(0)`

All scroll math must account for this negative-scrollTop convention. The browser naturally preserves viewport position when content is prepended above (older messages), so no manual scroll correction is needed for prepend operations.

---

## Features Implemented

### 1. Older Message Loading (Infinite Scroll Up)

**Problem:** `use_history_state_effect` had a race condition where it ran with empty messages during the initial load, set `has_more_before = false` and `channel_id = Some(ch)`, then returned early on the next run (channel ID matched) — so older messages could never be loaded.

**Fix:** Added `messages_loaded: bool` field to `ChatHistoryUiState`. The guard now checks both `channel_id` match AND `messages_loaded`. On first run with empty messages (race), `messages_loaded = false` so the effect re-runs when messages arrive.

```rust
// chat_history.rs
pub struct ChatHistoryUiState {
    pub messages_loaded: bool,  // NEW — distinguishes race-init from proper init
    // ...
}

// chat_view.rs — use_history_state_effect
if history_state.read().channel_id.as_deref() == Some(&active_channel_id)
    && history_state.read().messages_loaded   // <-- must also be true
{
    return;
}
let messages_loaded = !messages.is_empty();
// has_more_before only set true when messages actually loaded
let mut next_history = ChatHistoryUiState {
    has_more_before: messages_loaded,
    messages_loaded,
    // ...
};
```

**Scroll detection** (column-reverse, negative scrollTop):
```rust
let dist_from_bottom = (-metrics.scroll_top).max(0.0);
let max_scroll = (metrics.scroll_height - metrics.client_height).max(0.0);
let dist_from_top = (max_scroll - dist_from_bottom).max(0.0);
```

---

### 2. Scroll Position Memory

**Problem:** Switching channels and back always jumped to the latest message, losing the user's reading position.

**Solution:** Three-part approach:

#### a) Auto-save on every scroll event (JS)
```js
// scroll_runtime.js
scrollEl.addEventListener("scroll", function () {
    if (window.__polyCurrentChannelId) {
        window.__polyMessageScrollPositions[window.__polyCurrentChannelId] = scrollEl.scrollTop;
    }
}, { passive: true });
```
Saves continuously so the position is always current before any channel switch, regardless of Dioxus eval timing.

#### b) RAF-deferred restore (JS)
```js
window.polyRestoreScrollPosition = function (channelId) {
    var seq = ++_scrollSeq;
    requestAnimationFrame(function () {
        if (_scrollSeq !== seq) return; // sequence guard drops stale callbacks
        var el = document.getElementById("message-list-scroll");
        if (!el) return;
        var saved = window.__polyMessageScrollPositions[channelId];
        el.scrollTop = Number.isFinite(saved) ? saved : 0;
    });
};
```
RAF ensures the assignment runs after Dioxus has patched the DOM with the new channel's messages.

#### c) Set `__polyCurrentChannelId` on channel entry (Rust)
```rust
// chat_history.rs
pub fn request_restore_scroll_position_or_bottom(channel_id: &str) {
    document::eval(&format!(
        "window.__polyCurrentChannelId = {encoded}; window.polyRestoreScrollPosition?.({encoded});"
    ));
}
```

---

### 3. View Anchor — Return to Reading Position

**Problem:** When scrolled up in a channel (e.g. reading old messages from April 1), switching away and back loaded the latest 36 messages — the April 1 messages weren't in the window, so scroll position clamped to the wrong place.

**Solution:** Save the first-visible message as an anchor on `scrollend`, then load `MessageQuery::around` that message on channel re-entry.

#### a) Save anchor on scrollend (JS)
```js
scrollEl.addEventListener("scrollend", function () {
    if (!window.__polyCurrentChannelId) return;
    var channelId = window.__polyCurrentChannelId;
    var hostRect = scrollEl.getBoundingClientRect();
    var rows = scrollEl.querySelectorAll('[id^="message-"]');
    var anchorEl = null;
    for (var i = 0; i < rows.length; i++) {
        var rect = rows[i].getBoundingClientRect();
        if (rect.bottom > hostRect.top + 1 && rect.top < hostRect.bottom) {
            anchorEl = rows[i]; break;
        }
    }
    if (anchorEl) {
        window.__polyChannelAnchors[channelId] = {
            elementId: anchorEl.id,                          // e.g. "message-msg2-general-531"
            messageId: anchorEl.id.replace(/^message-/, ""), // e.g. "msg2-general-531"
            offset: anchorEl.getBoundingClientRect().top - hostRect.top,
        };
    }
}, { passive: true });
```

#### b) Read anchor and load around it (Rust)
```rust
// channel_list.rs — load_channel_data
let anchor = read_channel_view_anchor(&channel_id).await;
let query = if let Some((_, ref msg_id, _)) = anchor {
    poly_client::MessageQuery {
        around: Some(msg_id.clone()),
        limit: Some(initial_message_query(unread_count).limit.unwrap_or(36)),
        ..Default::default()
    }
} else {
    initial_message_query(unread_count)
};
if let Ok(messages) = guard.get_messages(&channel_id, query).await {
    let mut data = chat_data.write();
    data.messages = messages;
    data.messages_loaded_via_anchor = anchor.is_some(); // signals has_more_after=true
    drop(data);
    if let Some((ref element_id, _, offset_px)) = anchor {
        request_restore_to_anchor(&channel_id, element_id, offset_px);
    } else {
        request_restore_scroll_position_or_bottom(&channel_id);
    }
}
```

#### c) Restore viewport to exact pixel offset (JS + Rust)
```rust
// chat_history.rs
pub fn request_restore_to_anchor(channel_id: &str, element_id: &str, offset_px: f64) {
    document::eval(&format!(
        "window.__polyCurrentChannelId = {encoded_channel_id}; \
         window.polyPreserveMessageAnchor?.({msg_json}, {offset_px});"
    ));
}
```
```js
// scroll_runtime.js
window.polyPreserveMessageAnchor = function (anchorId, offsetPx) {
    var seq = ++_anchorSeq;
    requestAnimationFrame(function () {
        if (_anchorSeq !== seq) return;
        var host = document.getElementById("message-list-scroll");
        var anchor = document.getElementById(anchorId);
        if (!host || !anchor) return;
        var currentOffset = anchor.getBoundingClientRect().top - host.getBoundingClientRect().top;
        host.scrollTop = Math.min(0, host.scrollTop + currentOffset - offsetPx);
    });
};
```

#### d) `has_more_after = true` when loaded via anchor
```rust
// chat_view.rs — use_history_state_effect
let has_more_after = messages_loaded && chat_snapshot.messages_loaded_via_anchor;
```
This activates the bottom sentinel and "Jump to Present" button when the user is viewing an older window.

`ChatData.messages_loaded_via_anchor: bool` is set by `load_channel_data` and consumed here.

---

### 4. Jump to Present — One Click

**Problem:** When `has_more_after = true`, clicking "Jump to Present" only called `scrollTop = 0` (visual bottom of the *loaded* window), but the newest messages weren't loaded. Required two clicks.

**Fix:** Button click directly triggers `load_newer_messages` (which chain-loads up to 20 pages) then RAF-deferred scroll to bottom:

```rust
// chat_view.rs — render_jump_to_present onclick
if history_state.read().has_more_after && !history_state.read().loading_after {
    history_state.write().loading_after = true;
    spawn(async move {
        load_newer_messages(app_state, client_manager, chat_data, history_state).await;
        request_scroll_to_bottom_deferred(); // RAF-deferred
    });
} else {
    request_scroll_to_bottom();
}
```

The button label is always "Jump to Present" with a smaller subtitle "(You're Viewing Older Messages)" when `has_more_after = true`.

---

### 5. No Stale Jump to Present on Channel Switch

**Problem:** `scrolled_from_bottom` signal persisted across channel switches — channels opened at the bottom still showed the "Jump to Present" button inherited from the previous channel.

**Fix:** Reset in `use_history_state_effect` on channel switch:

```rust
if is_channel_switch {
    scrolled_from_bottom.set(false);
    new_messages_while_scrolled_up.set(0);
    // ... mark-as-read logic
}
```

---

## Key Files

| File | Role |
|------|------|
| `crates/core/assets/scripts/scroll_runtime.js` | All JS scroll helpers: save/restore position, anchor tracking, RAF guards |
| `crates/core/assets/styling/chat.css` | Jump to Present button styles incl. subtitle |
| `crates/core/src/ui/account/common/chat_history.rs` | Rust helpers: `ChatHistoryUiState`, query builders, JS interop fns |
| `crates/core/src/ui/account/common/chat_view.rs` | Main chat component: scroll loop, history state effect, render |
| `crates/core/src/ui/account/common/channel_list.rs` | `load_channel_data`: anchor lookup + `around` query |
| `crates/core/src/state/chat_data.rs` | `ChatData.messages_loaded_via_anchor` flag |

---

## Constants

```rust
pub const INITIAL_MESSAGE_PAGE_SIZE: u32 = 36;
pub const OLDER_MESSAGES_PAGE_SIZE: u32 = 50;
pub const MAX_LOADED_MESSAGES: usize = 200;       // fixed sliding window
pub const UNREAD_CONTEXT_MESSAGE_COUNT: u32 = 12;
const MESSAGE_HISTORY_SENTINEL_PX: f64 = 8.0;     // spacer height for sentinels
const MESSAGE_HISTORY_EDGE_THRESHOLD_PX: f64 = 1.0;
const JUMP_TO_PRESENT_THRESHOLD_PX: f64 = 200.0;  // px from bottom to show button
const MAX_CHAINED_NEWER_HISTORY_PAGES: usize = 20;
```

---

## Data Flow on Channel Switch

```
User clicks channel
  → load_channel_data spawned
  → read_channel_view_anchor (JS eval → __polyChannelAnchors[channelId])
  → if anchor: MessageQuery { around: msg_id }
    else:      MessageQuery { limit: 36 }
  → get_messages
  → chat_data.messages = result
  → chat_data.messages_loaded_via_anchor = anchor.is_some()
  → if anchor: request_restore_to_anchor (sets __polyCurrentChannelId + RAF polyPreserveMessageAnchor)
    else:      request_restore_scroll_position_or_bottom (sets __polyCurrentChannelId + RAF polyRestoreScrollPosition)

use_history_state_effect fires (reactive on chat_data)
  → if is_channel_switch: scrolled_from_bottom.set(false), new_messages_while_scrolled_up.set(0)
  → has_more_after = messages_loaded_via_anchor
  → history_state.set(new ChatHistoryUiState { has_more_after, ... })

scroll loop sees has_more_after = true
  → bottom sentinel active → "Jump to Present" shows with subtitle

User clicks "Jump to Present"
  → load_newer_messages (chain up to 20 pages until empty batch)
  → request_scroll_to_bottom_deferred (RAF scrollTop = 0)
  → has_more_after = false → button disappears

User scrolls naturally to bottom
  → scroll loop: dist_from_bottom = 0 ≤ sentinel + threshold
  → near_bottom = true, bottom_edge_armed = true → load_newer_messages fires
  → same chain-load path
```

---

## Anchor Lifecycle

```
scrollend fires (Chrome 114+, naturally debounced)
  → find first element with id^="message-" visible in viewport
  → __polyChannelAnchors[channelId] = { elementId, messageId, offset }

channel re-entry
  → read_channel_view_anchor reads __polyChannelAnchors[channelId]
  → returns (element_id, message_id, offset_px) or None
  → None if channel was at bottom (no anchor saved) → normal latest-messages load

anchor does NOT survive F5 (session-only, in-memory JS object)
```
