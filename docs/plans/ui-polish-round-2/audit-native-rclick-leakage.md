# Native Right-Click Leakage Audit

> Generated: 2026-04-21
> Source branches audited: HEAD (74bd47a)
> Files scanned: `crates/core/src/ui/**`, `clients/*/src/`, `crates/ui-macros/src/`

---

## 0. Summary

| Category | Count |
|---|---|
| Components with a `#[context_menu(...)]` attribute | 318 (all — lint-gate enforces coverage) |
| Components annotated `allow_default` (intentionally permits native menu) | ~19 per CSV; **actual source count is lower** — see §3 |
| Native HTML leaf elements inside `inherit`-decorated components that bypass the root `None` guard | **28 call sites** (`<input>`, `<textarea>`, `<a>`, `<img>`) |
| `dangerous_inner_html` blocks that can emit `<a>` tags | **2** (markdown renderer + `CustomBlock`) |
| Surfaces flagged by the previous plan as "not yet wired" that are still missing runtime guards | **3** (`ForumPostCard`, `DmMemberRow`, `VoiceTile`) |
| Root-level `oncontextmenu: prevent_default` guard (§4.5.1 of the plan) | **NOT PRESENT** — the plan claims shipped at commit f627d9fc, but `main_layout.rs` has no such handler |

---

## 1. Critical: The global root guard is missing

- [ ] **`crates/core/src/ui/main_layout.rs:294-305`** — The `.main-layout` root `<div>` has an `onclick` handler that dismisses the Poly menu, but **no `oncontextmenu` handler**. The plan (§4.5.1) states this was shipped in commit `f627d9fc` at `main_layout.rs:299`, but the current source has no `oncontextmenu` on that element. Without this guard, every component annotated `#[context_menu(None)]` still shows the native browser menu — the annotation is compile-time only (see §3 below for the macro behavior gap). This is the single highest-impact fix.

---

## 2. Macro behavior gap

- [ ] **`crates/ui-macros/src/context_menu.rs:78-92` — `expand()` is a no-op at runtime**. The macro is an argument-validator only. It parses the variant (`None`, `allow_default`, `inherit`, or a menu path), then re-emits the original item unchanged (`item2.into()`). It does **not**:
  - Inject an `oncontextmenu: evt.prevent_default()` handler for `None`
  - Inject `oncontextmenu: evt.stop_propagation()` for `allow_default`
  - Wrap the root element in any `ContextMenuHost` div
  - Call `open_menu::<Foo>(evt, ctx)` for typed menu variants

  The plan's §2.2 "expansion sketch" describes all of these behaviours, but they are documented as Phase B runtime work deferred to a later commit. The lint-gate enforces that every `#[component]` is *classified*, but classification alone provides zero runtime suppression. Every `None`-annotated component relies entirely on the missing root guard (§1 above) to suppress the native menu.

---

## 3. Leaf-element leaks inside `inherit`-decorated components

Because the macro injects nothing, the only runtime suppression path is a manual `oncontextmenu` handler on the specific element. The following elements have none. They will show the native browser menu on right-click (or show it even inside a `None` parent until the root guard from §1 is added).

### 3.1 `<textarea>` — OS spell-check / cut-copy-paste menu

- [ ] `crates/core/src/ui/account/common/chat_view.rs:4194` — **message composer `<textarea>`** inside `ChatView` (`None`). The message input is the most-used editable surface in the app. The OS right-click spell-check menu is not just annoying here — it can conflict with the Poly context menu if/when that is wired.
- [ ] `crates/core/src/ui/account/common/chat_view.rs:5744` — **`MessageInlineEdit` `<textarea>`** — annotated `#[context_menu(inherit)]`, but the CSV lists it as `allow_default`. Actual source shows `inherit`. The inline editor shows the OS text-edit context menu (undo, cut, copy, paste, spell-check). This is one of the surfaces the plan intended to be `allow_default` — the CSV is stale.
- [ ] `crates/core/src/ui/account/common/user_profile_modal.rs:361` — **`NoteEditor` `<textarea>`** inside `NoteEditor` (`inherit`). User note field in the profile modal. Same OS text-menu leak.
- [ ] `crates/core/src/ui/agent/chat_style_editor.rs:134` — **signature `<textarea>`** in `ChatStyleEditor` (`None`). Agent signature input.
- [ ] `crates/core/src/ui/agent/chat_style_editor.rs:145` — **extra-notes `<textarea>`** in `ChatStyleEditor` (`None`).
- [ ] `crates/core/src/ui/agent/profile.rs:75` — **agent profile bio `<textarea>`** (`inherit`).
- [ ] `crates/core/src/ui/settings/theme.rs:386` — **CSS editor `<textarea>`** in `CssEditorArea` (`inherit`). The CSS editor is arguably a good `allow_default` candidate (paste, select-all, undo are useful here).
- [ ] `crates/core/src/ui/create_forum_post.rs:103` — **forum post body `<textarea>`** in `CreateForumPostPage` (`None`).

### 3.2 `<input type="text|email|password|number|search|file|range|color|radio|checkbox">` — OS input menus

Text and password inputs show the OS cut/copy/paste/spellcheck menu; range inputs show nothing but still bubble the event; file inputs trigger OS file dialogs. These are mostly desirable `allow_default` surfaces.

- [ ] `crates/core/src/ui/account/common/chat_view.rs` — no standalone `<input>` in composer (uses `<textarea>`), but see `SearchFilterRow` and `ConversationSearchInput` (both `inherit`) which contain no inputs directly — the composed `<input>` lives inside `SettingsSearchBar` / `SearchInput` helpers.
- [ ] `crates/core/src/ui/settings/mod.rs:224` — **`SettingsSearchBar` `<input type="text">`** (`inherit`). Search bar in the settings panel.
- [ ] `crates/core/src/ui/client_ui/settings_section.rs:295` — **plugin settings toggle `<input type="checkbox">`** (`inherit`).
- [ ] `crates/core/src/ui/client_ui/settings_section.rs:337` — **plugin text field `<input type="text">`** (`inherit`).
- [ ] `crates/core/src/ui/client_ui/settings_section.rs:433` — **plugin slider `<input type="range">`** (`inherit`).
- [ ] `crates/core/src/ui/client_ui/view/toolbar.rs:215` — **forum filter `<input type="search">`** in `ViewToolbar` (`inherit`).
- [ ] `crates/core/src/ui/settings/backup.rs:411` — **reauth password `<input type="password">`** in `ReauthForm` (`inherit`). Password inputs leak the OS "paste" / password-manager menu — acceptable but should be explicit `allow_default`.
- [ ] `crates/core/src/ui/settings/backup.rs:565` — **URL `<input type="text">`** in `WizardStep1` (`inherit`).
- [ ] `crates/core/src/ui/settings/backup.rs:660,673` — **label + passphrase `<input>`s** in `WizardStep2` (`inherit`).
- [ ] `crates/core/src/ui/settings/theme.rs:284` — **color picker `<input type="color">`** in `ColorOverridesGrid` (`inherit`). The browser color picker is a native OS widget — right-clicking is browser-defined behaviour.
- [ ] `crates/core/src/ui/settings/plugins.rs:287` — **plugin URL `<input type="text">`** in `AddWasmPlugin` (`allow_default` — but the CSV `allow_default` annotation is on the component wrapper, not the input element; since the macro injects nothing this has no runtime effect).
- [ ] `crates/core/src/ui/settings/plugins.rs:325` — **plugin file `<input type="file">`** in `AddWasmPlugin` (`allow_default`). Same caveat.
- [ ] `crates/core/src/ui/agent/integrations.rs:81,103` — **MCP toggle checkbox + port number input** in `McpToggleRow` (`inherit`).
- [ ] `crates/core/src/ui/search.rs:132` — **search `<input type="text">`** in `SearchInput` (`inherit`).
- [ ] `crates/core/src/ui/server_overview.rs:55` — **repo search `<input type="text">`** in `ServerOverviewPage` (`None`).
- [ ] `crates/core/src/ui/create_channel.rs:81` — **channel name `<input type="text">`** in `CreateChannelPage` (`None`).
- [ ] `crates/core/src/ui/create_server.rs:74` — **server name `<input type="text">`** in `CreateServerPage` (`None`).
- [ ] `crates/core/src/ui/create_forum_post.rs:81,92,178` — **title, URL, and search `<input>`s** in `CreateForumPostPage` / `ForumSearchPage` (`None`).

### 3.3 `<a>` (anchor) tags — browser "Open in new tab", "Copy link", "Save link as"

- [ ] `crates/core/src/ui/account/common/chat_view.rs:5522` — **file attachment link `<a href target="_blank">`** inside `AttachmentsView` (`inherit` in source; CSV incorrectly lists as `allow_default`). Users right-clicking a non-image file attachment get "Open in new tab / Save link as / Copy link address" — probably fine, but not explicitly declared `allow_default`.
- [ ] `crates/core/src/ui/account/common/chat_view.rs:5379` — **markdown-rendered `<a>` tags** inside `MessageContentView` (`inherit`). The `render_markdown_html` function's `ammonia` builder explicitly allows the `<a>` tag (line 5360). Any markdown link `[text](url)` in a message produces a real `<a href>` node in the DOM emitted by `dangerous_inner_html`. Right-clicking that anchor shows the native "Open / Copy / Save" menu. There is no `oncontextmenu` on the `.message-markdown` wrapper div.
- [ ] `crates/core/src/ui/account/common/media_viewer.rs:198,206` — **download and open-in-new-tab `<a href>` links** inside `MessageMediaViewerOverlay` (`allow_default`). The component is correctly `allow_default`, but since the macro injects nothing, the DOM guard must come from the pre-existing `oncontextmenu: move |evt| evt.prevent_default()` handler on the media viewer root (has_oncontextmenu=1 per CSV). Confirm this handler exists and that it does not accidentally fire before the `<a>` default.
- [ ] `crates/core/src/ui/account/common/channel_list.rs:888` — **HN Algolia link `<a href>`** inside `ServerChannelView` (`inherit`). Right-click opens browser link menu.
- [ ] `crates/core/src/ui/code_explorer.rs:114` — **"Search on backend" `<a href>`** in `CodeExplorerView` (`None`). The component suppresses context menus per annotation, but since the macro injects nothing there is no actual suppression.
- [ ] `crates/core/src/ui/agent/integrations.rs:153,160` — **two MCP docs `<a href>` links** in `McpConfigBlock` (`inherit`).
- [ ] `crates/core/src/ui/settings/plugin_settings.rs:494` — **plugin homepage `<a href target="_blank">`** in `PluginManifestPanel` (`None`). Annotated `None` but no runtime suppression; anchor gets browser right-click menu.

### 3.4 `<img>` — "Save image as", "Copy image", "Open image in new tab"

Every `<img>` in the app produces a native browser right-click menu unless an `oncontextmenu` handler fires on that element or a non-propagating ancestor. The macro injects nothing.

- [ ] `crates/core/src/ui/account/common/chat_view.rs:5507` — **inline image attachment `<img>`** in `AttachmentsView` (`inherit`). The `onclick` navigates to the media viewer. Right-click shows "Save image as / Open image / Copy image". This was planned as an `allow_default` surface (the CSV says so), but: (a) the actual source annotation is `inherit`, not `allow_default`; (b) even if it were `allow_default`, the macro injects nothing. This is the most visible leak to users.
- [ ] `crates/core/src/ui/account/common/chat_view.rs:4075` — **link preview thumbnail `<img>`** in the attachment preview card section of `ChatView` (`None`).
- [ ] `crates/core/src/ui/account/common/chat_view.rs:3933` — **message author avatar `<img>`** (`None`).
- [ ] `crates/core/src/ui/account/common/chat_view.rs:2282` — **DM header avatar `<img>`** (`None`).
- [ ] `crates/core/src/ui/account/common/user_profile_modal.rs:242` — **user profile avatar `<img>`** in `UserProfileModal` (`None`).
- [ ] `crates/core/src/ui/account/common/voice_view.rs:516` — **voice participant avatar `<img>`** in `VoiceTile` (`inherit`).
- [ ] `crates/core/src/ui/account/common/voice_bar.rs:212` — **voice dock tile avatar `<img>`** in `VoiceDockTile` (`inherit`).
- [ ] `crates/core/src/ui/account/common/forum_view.rs:365,469` — **forum post + comment author avatar `<img>`** in `ForumPostCard` / `ForumComment` (both `inherit`). Right-click on forum surfaces was one of the explicit problem cases in the original plan.
- [ ] `crates/core/src/ui/account/common/friends_panel.rs:257,297` — **friend avatar `<img>` in `FriendsGrid` and `BlockedUsersGrid`** (`inherit`).
- [ ] `crates/core/src/ui/account/common/dm_user_sidebar.rs:127` — **DM sidebar user avatar `<img>`** in `DmMemberRow` (`inherit`).
- [ ] `crates/core/src/ui/account/common/account_server_bar.rs:414` — **server icon `<img>`** in `ServerIconDisplay` (`inherit`). Ironically the parent `AccountServerIcon` (`inherit`) has an inline `oncontextmenu` handler, but `ServerIconDisplay` is a separate child component without one. The event bubbles through.
- [ ] `crates/core/src/ui/favorites_sidebar.rs:730,1023,1037` — **server icon `<img>`, icon `<img>`, and account source-badge `<img>`** in `FavoriteServerIcon` (`inherit`). `FavoriteServerIcon` has a manual `oncontextmenu` handler, but the three `<img>` elements inside it do not have individual guards. The bubble path through the icon works, but the inner images can produce a "ghost" duplicate native menu before propagation.
- [ ] `crates/core/src/ui/account/server/settings/overview.rs:59,168` — **icon preview and banner preview `<img>`s** in `IconPanel` / `BannerPanel` (both `allow_default`). Intended to allow native save-image menu, but no runtime mechanism exists to actually allow them while blocking other surfaces.
- [ ] `crates/core/src/ui/account/common/media_viewer.rs:254,294` — **main viewer image and thumbnail strip `<img>`s** in `MessageMediaViewerOverlay` (`allow_default`). Intentional, but relies on the pre-existing manual handler.
- [ ] `crates/core/src/ui/account/common/channel_list.rs:520` — **server banner `<img>`** in `ServerChannelView` (`inherit`).
- [ ] `crates/core/src/ui/account/settings/profile.rs:98` — **profile avatar `<img>`** in `PolyProfileSettings` (`None`).
- [ ] `crates/core/src/ui/account/common/user_sidebar.rs:281` — **member list user avatar `<img>`** (`None`).
- [ ] `crates/core/src/ui/account/common/saved_items_view.rs:393` — **saved-item author avatar `<img>`** in `SavedPinnedItemCard` (`inherit`).
- [ ] `crates/core/src/ui/search.rs:166` — **search result avatar `<img>`** in `AvatarIcon` (`inherit`).
- [ ] `crates/core/src/ui/account/common/conversation_search_view.rs:86` — **conversation search avatar `<img>`** in `AvatarIcon` (`inherit`).
- [ ] `crates/core/src/ui/mod.rs:1316` — **startup overlay account avatar `<img>`** in `StartupOverlay` (`None`).
- [ ] `crates/core/src/ui/server_overview.rs:158` — **repo card icon `<img>`** in `RepoCard` (`inherit`).
- [ ] `crates/core/src/ui/voice_banner.rs:88` — **voice banner participant avatar `<img>`** in `VoiceBannerParticipants` (`inherit`).
- [ ] `crates/core/src/ui/account/common/thread_view.rs:392` — **thread message author avatar `<img>`** in `ThreadMessageRow` (`inherit`).
- [ ] `crates/core/src/ui/account/common/account_bar.rs:139,223` — **account bar user avatar + popup avatar `<img>`s** in `AccountBarUserInfo` / `AccountProfilePopup` (both `inherit`).
- [ ] `crates/core/src/ui/account/common/chat_view.rs:2531` — **mobile server icon in chat header `<img>`** in `ChatHeaderActions` (`inherit`).
- [ ] `crates/core/src/ui/account/common/direct_call_overlay.rs:172` — **incoming call avatar `<img>`** in `OutgoingDirectCallOverlay` (`None`).
- [ ] `crates/core/src/ui/account/common/new_conversation_view.rs:108` — **new conversation search result avatar `<img>`** in `NewConversationView` (`None`).
- [ ] `crates/core/src/ui/account/common/channel_list.rs:1148,1509` — **DM friend avatar + voice participant avatar `<img>`s** in `FriendItem` / `VoiceParticipantEntry` (`inherit`).

### 3.5 `dangerous_inner_html` blocks that may contain `<a>` or `<img>`

- [ ] `crates/core/src/ui/account/common/chat_view.rs:5379` — **`MessageContentView` markdown block** (`inherit`). The `render_markdown_html` function explicitly adds `"a"` to the `ammonia` allowlist (line 5360). Markdown links render as real `<a>` anchors inside `dangerous_inner_html`. The wrapper `<div class="message-markdown">` has no `oncontextmenu` handler. Right-clicking a rendered link shows the OS anchor menu.
- [ ] `crates/core/src/ui/client_ui/custom_block.rs:355` — **`CustomBlock` plugin-rendered HTML** (`inherit`). Plugin HTML goes through `ammonia` sanitization that strips `<script>`, `<iframe>`, `<form>`, `<input>` (§65 of that file), but `<a>` and `<img>` are in the default allowlist. A plugin that renders links or images will produce interactive DOM nodes that show the native browser right-click menu.

---

## 4. CSV vs. source discrepancies (stale `context-menu-coverage.csv`)

The coverage CSV at `docs/plans/context-menu-coverage.csv` was generated at an earlier commit. Several entries are incorrect as of HEAD:

| CSV claim | File | CSV decorator | Actual source decorator |
|---|---|---|---|
| `AttachmentsView` | `chat_view.rs:5170` (CSV) / `5449` (source) | `allow_default` | `inherit` |
| `MessageInlineEdit` | `chat_view.rs:5451` (CSV) / `5733` (source) | `allow_default` | `inherit` |
| Line numbers for many `chat_view.rs` components are off by ~280 lines, suggesting a large insertion happened after the CSV was generated |

- [ ] Regenerate `context-menu-coverage.csv` by running `cargo check --features regen-baseline -p poly-lint-gate` and re-running `scripts/audit_context_menus.sh` (per plan §1.1.1 and §5.2.1).

---

## 5. Surfaces flagged by the past plan still not wired

The original plan (§5.1.3 note) left three TODOs for typed menus that need authoring:

- [ ] `crates/core/src/ui/account/common/forum_view.rs:656` — `ForumPostCard` (`inherit`) — typed `ForumPostContextMenu` is referenced but only partially wired; the `ForumComment` at line 781 also has a manual `oncontextmenu` handler but the parent `ForumPostCard` and `HnFeedView` have no per-row menu. Right-clicking HN/Lemmy post rows shows the native browser context menu.
- [ ] `crates/core/src/ui/account/common/dm_user_sidebar.rs:93` — `DmMemberRow` (`inherit`) — the plan noted a `UserRowContextMenu` is needed but not authored. Right-clicking a DM user row shows the native menu.
- [ ] `crates/core/src/ui/account/common/voice_view.rs:445` — `VoiceTile` (`inherit`) — no per-tile context menu and no native-menu suppression on participant avatars inside voice tiles.

---

## 6. Recommendations

### 6.1 Single highest-ROI fix: add the root guard (addresses all §2 leaks at once)

Add `oncontextmenu: |evt| evt.prevent_default()` to the `.main-layout` root `<div>` in `main_layout.rs` around line 295. This was the claimed §4.5.1 ship that did not land. With this one change, every component annotated `#[context_menu(None)]` and `#[context_menu(inherit)]` inside a `None` ancestor will have native menu suppression without any per-component change.

```rust
div {
    class: "main-layout",
    // Belt-and-suspenders: suppress native menu app-wide.
    // Per-component `allow_default` surfaces opt back in via stop_propagation.
    oncontextmenu: |evt| evt.prevent_default(),
    onclick: move |_| { /* dismiss Poly menu */ },
    // …
}
```

### 6.2 Implement the Phase B macro expansion

The `expand()` in `crates/ui-macros/src/context_menu.rs` currently returns the item unchanged. Implement the planned DOM-level wrapper:
- `None` → wrap root element with `oncontextmenu: |evt| evt.prevent_default()`.
- `allow_default` → wrap root element with `oncontextmenu: |evt| evt.stop_propagation()` (so the root guard in §6.1 does not fire, but browser default is allowed).
- Typed menu → open the named menu + prevent_default.
- `inherit` → no wrapper (propagate to ancestor).

Until this is done, the classification system is entirely documentation; runtime behavior is determined only by manual `oncontextmenu` handlers and the root guard.

### 6.3 Explicit `allow_default` for text-editing surfaces

For `MessageInlineEdit`, `NoteEditor`, `CssEditorArea`, `ChatStyleEditor` textareas, the compose `<textarea>`, and all password/text `<input>` fields: change the decorator from `inherit` to `allow_default`. Once Phase B macro expansion lands, this will automatically add `stop_propagation` so the root guard (§6.1) doesn't fire, letting the OS text-edit menu through. In the short term, add explicit `oncontextmenu: |evt| evt.stop_propagation()` on those elements.

### 6.4 Wrap markdown-rendered blocks and `CustomBlock`

Add `oncontextmenu: |evt| evt.stop_propagation()` to the `<div class="message-markdown">` wrapper and the `<div class="custom-block-content">` div. This allows the native "Open link / Save link" menu on rendered `<a>` tags inside those blocks while preventing the generic "Inspect / View source" menu from firing on the surrounding message div. Annotate these `allow_default` once the macro expansion is in place.

### 6.5 Server icon and avatar images: decide per-surface

- **Server icons** (`FavoriteServerIcon`, `AccountServerIcon`): these already have manual `oncontextmenu` open-poly-menu handlers. The `<img>` inside them should stay suppressed (the poly menu should appear, not "Save image as"). The existing handlers achieve this if the inner `<img>` bubbles upward to them. Verify there is no browser-specific behaviour that fires native before bubbling on `<img>`.
- **User avatars** (everywhere in §3.4 above): consider a global CSS rule `img { pointer-events: none; }` on the app container combined with the root guard. This is the pattern Discord uses to prevent `<img>` native menus without per-site handlers. Then selectively re-enable pointer events on images that need them (`MessageMediaViewerOverlay` thumbnails, `IconPanel` / `BannerPanel` preview images).
- **Attachment inline images** (`AttachmentsView`): re-annotate to `allow_default` and add `oncontextmenu: |evt| evt.stop_propagation()` on the wrapping `.attachment-image` div so users can "Save image as" from the in-chat image.

### 6.6 Regenerate the coverage CSV

The CSV is stale; at minimum two decorator values are wrong and all `chat_view.rs` line numbers are off by ~280 lines. Run the audit script and update before the next round of fixes.

### 6.7 Global `img { pointer-events: none }` consideration

A single CSS rule `img { -webkit-user-drag: none; pointer-events: none; }` scoped to `.main-layout` would silently suppress the native right-click menu on all avatar and decoration images without any Rust-side per-site handlers. Images that need interaction (inline attachment images, media viewer thumbnails) would get `pointer-events: auto` overrides. This pattern is simpler to audit than per-component `oncontextmenu` and survives future component additions automatically.
