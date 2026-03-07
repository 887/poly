# Phase 2.17 — Replies, Client-Loaded Emoji/Stickers, and GIF Providers

**Status:** 🚧 In Progress
**Scope:** Rich chat media foundation across all backends + provider settings for GIF search

---

## Goals

Implement the next major chat-composer layer with three distinct architecture tracks:

1. **Replies**
   - Messages must carry reply metadata loaded from the backend.
   - The composer needs reply-target state and a visible reply bar.
   - Sending a reply must use a backend-aware API instead of a UI-only fake quote.

2. **Emoji + Stickers (client-loaded)**
   - Available custom emoji/stickers must be loaded by the active messenger backend.
   - The picker/search UI must support backend-provided assets (including server-scoped assets usable in the current channel).
   - The protocol must be generic enough for Discord, Stoat, Matrix, Teams, and demo.

3. **GIF Providers (app-level integrations)**
   - GIF search is not messenger-backend data; it is an app integration configured in settings.
   - Users can enable multiple providers and choose the active provider in the GIF dialog.
   - Providers show setup status in UI (e.g. `Klippy — not setup`) until the user supplies an API key.
   - Initial provider targets: **Klippy**, **Giphy**, **Imgur**.

---

## Architecture Decisions

### A. Replies are backend data, not UI-only state
- Add a shared `MessageReplyPreview` type to `poly-client`.
- Add `reply_to: Option<MessageReplyPreview>` to `Message`.
- Add a new backend method for sending replies without breaking the existing `send_message` path:
  - `send_reply_message(channel_id, reply_to_message_id, content)`
- Default implementation falls back to plain `send_message` so existing backends keep compiling.

### B. Emoji/stickers are channel-scoped catalogs loaded by the backend
- Add shared `CustomEmoji` and `StickerItem` types to `poly-client`.
- Add channel-scoped fetch methods:
  - `get_available_emojis(channel_id)`
  - `get_available_stickers(channel_id)`
- UI searches locally inside the returned catalogs first.
- Later phases may add server-side search methods if large catalogs require paging.

### C. GIF providers are app settings, not backend session state
- Persist provider config in `AppSettings`.
- Add a new Settings section: **Media**.
- Store:
  - active provider
  - enabled/disabled providers
  - provider API keys
- The GIF dialog will show provider status based on these settings.

---

## Checklist

### Shared Protocol
- [x] Add `MessageReplyPreview` to `poly-client`
- [x] Add `reply_to: Option<MessageReplyPreview>` to `Message`
- [x] Add `CustomEmoji` type to `poly-client`
- [x] Add `StickerItem` type to `poly-client`
- [x] Add `send_reply_message(...)` to `ClientBackend`
- [x] Add `get_available_emojis(channel_id)` to `ClientBackend`
- [x] Add `get_available_stickers(channel_id)` to `ClientBackend`

### WASM Plugin Interface
- [x] Update `wit/messenger-plugin.wit` for new types + methods
- [x] Update `crates/plugin-host/src/bridge.rs`
- [x] Update `crates/plugin-host/src/registry.rs`
- [x] Update every `clients/*/src/guest.rs` implementation

### Demo Backend
- [x] Add demo reply metadata to selected messages
- [x] Add demo `send_reply_message`
- [x] Add demo emoji catalog per server/channel
- [x] Add demo sticker catalog per server/channel

### Storage + Settings
- [x] Add GIF provider settings to `AppSettings`
- [x] Add `SettingsSection::Media`
- [x] Add `crates/core/src/ui/settings/media.rs`
- [x] Add provider combobox + status rows + API-key fields
- [x] Add i18n keys for media settings

### UI (Initial Slice — Replies)
- [x] Add reply target state in `chat_view.rs`
- [x] Add reply action button wiring (sets `reply_target` signal)
- [x] Add `ReplyComposerBar` component with cancel button
- [x] Add `MessageReplyPreviewLine` component (shown above replied messages)
- [x] Add i18n key `chat-replying-to` in all four locales
- [x] Add reply CSS (`.message-reply-preview`, `.reply-composer-bar`, etc.)
- [x] Render message reply previews above replied messages
- [x] Send via `send_reply_message` when reply target is active
- [ ] Wire context-menu Reply action to `reply_target`
- [ ] Replace static emoji picker architecture with backend-catalog-ready picker model
- [ ] Add tabs/skeleton UI for Emoji / Stickers / GIFs
- [ ] Add GIF provider dropdown/status text in the GIF tab

### Validation
- [x] `cargo check --workspace`
- [x] `cargo cranky --workspace` — zero warnings
- [x] `cargo check -p poly-web --target wasm32-unknown-unknown`
- [x] `dx build --platform desktop` in `apps/desktop-devtools/` — succeeded
- [x] `cargo fmt --all`
- [ ] Desktop DevTools visual screenshot verification

---

## Session Notes

### 2026-03-07 (continued — session 2)
- Completed `ReplyComposerBar` and `MessageReplyPreviewLine` components in `chat_view.rs`.
- Added `chat-replying-to` i18n key in all four locales (en/de/fr/es).
- Added full reply CSS: `.message-reply-preview`, `.message-reply-arrow`, `.message-reply-author`,
  `.message-reply-snippet`, `.reply-composer-bar`, `.reply-composer-main`, `.reply-composer-title`,
  `.reply-composer-snippet`, `.reply-composer-close`.
- Fixed lint violations:
  - Renamed `GifProviderKind::from_str` → `from_slug` to avoid `should_implement_trait` lint.
  - Extracted `SendMessageCtx` struct to collapse 7 args into a single parameter (too_many_arguments).
- `cargo cranky --workspace` → zero warnings.
- `cargo check --workspace`, WASM check, `dx build` → all clean.
- Remaining next steps: context-menu reply wiring, backend-catalog emoji/sticker picker UI, GIF dialog.


