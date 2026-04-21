# Context-Menu `inherit` Audit

> Generated: 2026-04-21

## 0. Summary

- **Total `inherit` sites (workspace, excluding worktrees):** 284
- **"Fine, leave as-is"** (transparent passthroughs / subcomponents inside a typed-menu parent): **192**
- **"Should be `none`"** (leaf surface or panel with no typed ancestor menu, leaks native menu): **72**
- **"Should declare a typed menu"** (genuine missing menu — user expects Poly items here): **20**

> The root `oncontextmenu: evt.prevent_default()` guard at `main_layout.rs:299` blocks the
> native menu for all of these even today, so there is no active regression — but the
> guard is belt-and-suspenders, not the policy. The items below are the places that
> *semantically* should have explicit policies rather than silently cascading `inherit`
> up to the root guard.

---

## 1. Should be `none` (leak fixes — suppress native menu, no Poly menu needed)

Settings pages, panels, and utility widgets where a user might right-click but where no
Poly menu is meaningful. Each should become `#[context_menu(none)]` to be explicit
about opting out.

### 1.1 Global settings pages / sections (all inherit up to root guard)

- [ ] `crates/core/src/ui/settings/general.rs:436` — `LayoutSettings` — *top-level settings section; no interactive right-click surface. Should be `none`.*
- [ ] `crates/core/src/ui/settings/general.rs:456` — `GeneralSettings` — *same as above.*
- [ ] `crates/core/src/ui/settings/mod.rs:214` — `SettingsSearchBar` — *search input inside settings; the input field itself should get `allow_default`, but the wrapper can be `none`.*
- [ ] `crates/core/src/ui/settings/mod.rs:299` — `SettingsContentHeader` — *non-interactive header; `none`.*
- [ ] `crates/core/src/ui/settings/mod.rs:317` — `SettingsNavigation` — *nav rail inside settings modal; no Poly menu expected here.*
- [ ] `crates/core/src/ui/settings/mod.rs:420` — `SettingsAllSections` — *layout wrapper; `none`.*
- [ ] `crates/core/src/ui/settings/common.rs:22` — `PolySelect` — *generic select widget used across settings; `none` (or `allow_default` if the native `<select>` element needs its own browser affordances).*
- [ ] `crates/core/src/ui/settings/diagnostics.rs:71` — `AccountDiagnosticsRow` — *diagnostic info row; `none`.*
- [ ] `crates/core/src/ui/settings/accounts.rs:89` — `AccountRow` — *account row in settings list; no Poly action expected.*
- [ ] `crates/core/src/ui/settings/accounts.rs:136` — `AccountsSettings` — *settings section root; `none`.*

### 1.2 Account-scoped settings pages

- [ ] `crates/core/src/ui/account/settings/mod.rs:180` — `AccountSettingsSearchBar` — *`none` (input should be `allow_default`).*
- [ ] `crates/core/src/ui/account/settings/mod.rs:211` — `AccountSettingsContentHeader` — *header; `none`.*
- [ ] `crates/core/src/ui/account/settings/mod.rs:236` — `AccountSettingsPage` — *page-level shell; `none`.*
- [ ] `crates/core/src/ui/account/settings/notifications.rs:101` — `NotificationsSettings` — *settings page root; `none`.*
- [ ] `crates/core/src/ui/account/settings/notifications.rs:136` — `AccountNotifSignalsSection` — *section within settings; `none`.*
- [ ] `crates/core/src/ui/account/settings/notifications.rs:208` — `AccountNotifSectionInner` — *inner section; `none`.*
- [ ] `crates/core/src/ui/account/settings/notifications.rs:277` — `NotifToggleRow` — *toggle row in settings; `none`.*
- [ ] `crates/core/src/ui/account/settings/profile.rs:47` — `PolyProfileSettings` — *settings page root; `none`.*
- [ ] `crates/core/src/ui/account/settings/voice_settings.rs:79` — `VoiceSettings` — *settings page root; `none`.*
- [ ] `crates/core/src/ui/account/settings/voice_settings.rs:129` — `MicDevicePicker` — *device picker; `none` (dropdown handled by native `<select>`).*
- [ ] `crates/core/src/ui/account/settings/voice_settings.rs:164` — `SpeakerDevicePicker` — *same; `none`.*
- [ ] `crates/core/src/ui/account/settings/voice_settings.rs:199` — `NoiseCancelToggle` — *toggle; `none`.*
- [ ] `crates/core/src/ui/account/settings/voice_settings.rs:232` — `TestMicButton` — *button; `none`.*
- [ ] `crates/core/src/ui/account/settings/content_social.rs:38` — `SensitiveMediaRow` — *toggle row; `none`.*
- [ ] `crates/core/src/ui/account/settings/content_social.rs:69` — `ToggleRow` — *toggle row; `none`.*
- [ ] `crates/core/src/ui/account/settings/content_social.rs:106` — `SensitiveMediaSection` — *section; `none`.*
- [ ] `crates/core/src/ui/account/settings/content_social.rs:156` — `SpamFilterSection` — *section; `none`.*
- [ ] `crates/core/src/ui/account/settings/content_social.rs:211` — `AgeRestrictedSection` — *section; `none`.*
- [ ] `crates/core/src/ui/account/settings/content_social.rs:253` — `SocialPermissionsSection` — *section; `none`.*
- [ ] `crates/core/src/ui/account/settings/content_social.rs:300` — `FriendRequestsSection` — *section; `none`.*
- [ ] `crates/core/src/ui/account/settings/content_social.rs:357` — `ContentSocialSettings` — *settings page root; `none`.*

### 1.3 Server settings pages

- [ ] `crates/core/src/ui/account/server/settings/mod.rs:147` — `ServerSettingsSearchBar` — *`none` (input `allow_default`).*
- [ ] `crates/core/src/ui/account/server/settings/mod.rs:178` — `ServerSettingsContentHeader` — *header; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/mod.rs:191` — `ServerSettingsNavigation` — *nav rail; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/mod.rs:228` — `ServerSettingsContent` — *content wrapper; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/mod.rs:384` — `ServerSettingsPage` — *page shell; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/mod.rs:526` — `ServerSettingsNavItem` — *nav list item; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/general.rs:32` — `ServerGeneralSettings` — *settings section root; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/general.rs:87` — `LeaveServerConfirm` — *confirm modal; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/notifications.rs:44` — `ServerNotificationsSettings` — *settings page root; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/notifications.rs:112` — `NotifLevelOption` — *radio-style option row; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/notifications.rs:128` — `NotifToggleRow` — *toggle row; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/overview.rs:39` — `IconPanel` — *server icon edit panel inside settings; no Poly menu needed.*
- [ ] `crates/core/src/ui/account/server/settings/overview.rs:153` — `BannerPanel` — *banner edit panel; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/overview.rs:253` — `ServerOverviewSettings` — *settings page root; `none`.*
- [ ] `crates/core/src/ui/account/server/settings/profile.rs:28` — `ServerProfileSettings` — *settings page root; `none`.*

### 1.4 Channel settings pages

- [ ] `crates/core/src/ui/account/channel/settings/mod.rs:49` — `ChannelSettingsContent` — *settings content wrapper; `none`.*
- [ ] `crates/core/src/ui/account/channel/settings/mod.rs:129` — `ChannelSettingsPage` — *settings page shell; `none`.*

### 1.5 Global settings subsections

- [ ] `crates/core/src/ui/settings/backup.rs:806` — `BackupSettings` — *backup settings page root; `none`.*
- [ ] `crates/core/src/ui/settings/identity.rs:155` — `IdentitySettings` — *identity settings page root; `none`.*
- [ ] `crates/core/src/ui/settings/language.rs:133` — `LanguageSettings` — *language settings page root; `none`.*
- [ ] `crates/core/src/ui/settings/media.rs:147` — `MediaSettings` — *media settings page root; `none`.*
- [ ] `crates/core/src/ui/settings/plugins.rs:374` — `PluginsSettings` — *plugins settings page root; `none`.*
- [ ] `crates/core/src/ui/settings/voice_video.rs:183` — `VoiceVideoSettings` — *voice/video settings page root; `none`.*
- [ ] `crates/core/src/ui/settings/theme.rs:463` — `ThemeSettings` — *theme settings page root; `none`.*

### 1.6 Modals and overlays (no typed menu; not a content-right-click surface)

- [ ] `crates/core/src/ui/account/common/user_profile_modal.rs:105` — `UserProfileModal` — *modal overlay; inner `NoteEditor` at :354 should stay `inherit` (text field) but the modal shell itself should be `none`. The root guard already blocks native menu; making it explicit avoids confusion.*
- [ ] `crates/core/src/ui/account/common/user_profile_modal.rs:354` — `NoteEditor` — *text area inside modal; should be `allow_default` (user may want to copy/paste with native menu), not `inherit`.*
- [ ] `crates/core/src/ui/account/common/direct_call_overlay.rs:20` — `OutgoingDirectCallOverlay` — *full-screen overlay; `none`.*
- [ ] `crates/core/src/ui/account/common/media_viewer.rs:23` — `MessageMediaViewerOverlay` — *image viewer overlay; the image element itself should be `allow_default` (Save Image), the surrounding chrome should be `none`.*
- [ ] `crates/core/src/ui/create_channel.rs:44` — `CreateChannelPage` — *modal/page form; `none`.*
- [ ] `crates/core/src/ui/create_server.rs:42` — `CreateServerPage` — *modal/page form; `none`.*

### 1.7 Utility rail, panels, and ephemeral UI that are not right-click targets

- [ ] `crates/core/src/ui/account/common/chat_view.rs:4810` — `ChatUtilityRail` — *the utility rail tab container (agent/search/settings); it has no typed menu of its own and is not a right-click surface the user expects a Poly menu on. `none`.*
- [ ] `crates/core/src/ui/account/common/chat_view.rs:5023` — `ChatSettingsPanel` — *per-channel notification settings panel inside utility rail; `none`.*
- [ ] `crates/core/src/ui/account/common/chat_view.rs:5127` — `SearchFilterPopup` — *message-search filter popup; `none`.*
- [ ] `crates/core/src/ui/account/common/chat_view.rs:5586` — `TypingIndicator` — *typing indicator bar; `none`.*
- [ ] `crates/core/src/ui/account/common/chat_view.rs:6108` — `DmContactListPanel` — *DM contact list panel inside chat; contains user rows but itself should be `none` (see typed-menu for `DmContactRow` in Section 2).*
- [ ] `crates/core/src/ui/account/common/emoji_picker.rs:336` — `EmojiPicker` — *emoji picker popup; `none` (emoji cells don't warrant a Poly context menu).*

---

## 2. Should declare a typed menu (genuine missing menu)

These are interactive list rows and surfaces the user will right-click with an expectation
of receiving Poly-specific actions. `UserRowContextMenu` is already defined in
`crates/core/src/ui/context_menu/menus.rs` and only needs to be wired. `DmMemberRow`
and `DmUserSidebar` need the same wire-up.

### 2.1 User/member rows in sidebars

- [ ] `crates/core/src/ui/account/common/user_sidebar.rs:104` — `UserSidebar` — *member-list panel; should host `UserRowContextMenu` (or become `none` if the row itself carries the menu). Currently `UserRowContextMenu` exists in `menus.rs` but is unwired.*
- [ ] `crates/core/src/ui/account/common/user_sidebar.rs:235` — `UserGroup` — *grouped section inside the member list; `inherit` is fine here once `UserSidebar` is typed, but the actual clickable user rows are in `user_sidebar.rs` as non-component render functions — the closest component host is this one. Suggested: `none` on the group header, `UserRowContextMenu` on a to-be-extracted `UserRow` component.*
- [ ] `crates/core/src/ui/account/common/dm_user_sidebar.rs:41` — `DmUserSidebar` — *DM channel member list panel; same gap as `UserSidebar`. The `DmMemberRow` at :94 is the direct right-click target. Suggested: wire `UserRowContextMenu` (already authored) to `DmMemberRow`.*
- [ ] `crates/core/src/ui/account/common/dm_user_sidebar.rs:94` — `DmMemberRow` — *individual user row in DM sidebar; primary right-click target. Suggested typed menu: `UserRowContextMenu` (items: View Profile, Send Message, Block).*
- [ ] `crates/core/src/ui/account/common/chat_view.rs:6158` — `DmContactRow` — *contact row in the DM contact list panel; interactive row the user will right-click. Suggested: `UserRowContextMenu`.*

### 2.2 Friends panel rows

- [ ] `crates/core/src/ui/account/common/friends_panel.rs:201` — `FriendsGrid` — *grid of friend cards; each card is right-clickable. Suggested: a new `FriendRowContextMenu` (items: Message, Remove Friend, Block). Alternatively the parent `FriendsPanel` already declares no typed menu, so a new component per-card should carry this.*
- [ ] `crates/core/src/ui/account/common/friends_panel.rs:278` — `BlockedUsersGrid` — *grid of blocked users; right-click for Unblock. Suggested: `BlockedUserContextMenu` or reuse a `UserRowContextMenu` variant.*

### 2.3 Notification items

- [ ] `crates/core/src/ui/account/common/notifications.rs:396` — `NotificationItemContent` — *individual notification card; right-click should offer "Mark as Read", "Jump to Message", "Clear". Suggested: `NotificationItemContextMenu` (new, simple).*

### 2.4 Saved items

- [ ] `crates/core/src/ui/account/common/saved_items_view.rs:367` — `SavedPinnedItemCard` — *a saved/pinned message card; right-click should offer "Jump to message", "Remove from saved", "Copy link". Suggested: `SavedItemContextMenu` (new).*

### 2.5 Thread rows

- [ ] `crates/core/src/ui/account/common/thread_view.rs:375` — `ThreadMessageRow` — *message row inside a thread view; same right-click expectations as a regular chat message. Suggested: wire the existing `MsgContextMenuOverlay` pattern (or a lightweight `ThreadMessageContextMenu`).*
- [ ] `crates/core/src/ui/account/common/thread_view.rs:203` — `ActiveThreadChip` — *clickable thread chip in the threads bar; right-click should offer "Open in full view", "Mute thread". Suggested: `ThreadChipContextMenu` (new).*

### 2.6 Code/repo explorer rows

- [ ] `crates/core/src/ui/code_explorer.rs:192` — `CodeExplorerEntry` — *file entry row in the code explorer; right-click should offer "Open", "Copy path". Suggested: `CodeExplorerEntryContextMenu` (new).*
- [ ] `crates/core/src/ui/server_overview.rs:121` — `RepoCard` — *repository card on the server overview page; right-click offers "Open repository", "Copy URL". Suggested: `RepoCardContextMenu` (new).*

### 2.7 Plugin sidebar rows

- [ ] `crates/core/src/ui/client_ui/sidebar/custom.rs:126` — `SidebarItemRow` — *custom plugin sidebar row; right-click is a natural affordance depending on backend. For now `none` until a plugin sidebar API is defined, but this is the place to wire `ClientSidebarItemContextMenu` once the API lands.*

### 2.8 List/tree body rows (plugin views)

- [ ] `crates/core/src/ui/client_ui/view/list_body.rs:275` — `ListBodyRow` — *row in a plugin-rendered list view; right-click should delegate to a backend-provided menu or `none`. Suggested: `none` as a safe default until a `ClientViewRowContextMenu` API is designed.*
- [ ] `crates/core/src/ui/client_ui/view/tree_body.rs:247` — `TreeBodyRow` — *row in a plugin-rendered tree view; same reasoning as above. Suggested: `none` until `ClientViewRowContextMenu` API exists.*

---

## 3. Fine as-is (transparent passthroughs or subcomponents inside a typed-menu parent)

These components either forward rendering to a parent that already has a typed menu, are
pure layout wrappers with no direct user interaction of their own, or live inside a
context-menu-owning component already (so their `inherit` correctly propagates up to the
typed menu host).

### 3.1 App shell / routing / top-level wrappers

- (no action) `crates/core/src/ui/mod.rs:1239` — `AppBody` — *top-level app body; routes content. Parent `App` is below.*
- (no action) `crates/core/src/ui/mod.rs:1278` — `StartupOverlay` — *startup overlay; shown before MainLayout is live, so inheriting from the App root is correct.*
- (no action) `crates/core/src/ui/mod.rs:1375` — `App` — *root component; inheriting means "I delegate to MainLayout below me." Correct.*
- (no action) `crates/core/src/ui/routes.rs:1079` — `DmsLayout` — *routing layout wrapper for DMs; transparent pass-through.*
- (no action) `crates/core/src/ui/routes.rs:1111` — `ServerLayout` — *routing layout wrapper for server views; transparent pass-through.*
- (no action) `crates/core/src/ui/main_layout.rs:161` — `NavBar` — *the nav bar; its right-click is caught by the root guard at :208. Acceptable.*
- (no action) `crates/core/src/ui/main_layout.rs:206` — `MainLayout` — *the root layout; the `oncontextmenu: prevent_default` guard lives here. `inherit` is slightly odd for the layout itself (it has no parent), but harmless.*
- (no action) `crates/core/src/ui/split_shell.rs:102` — `SplitMenuShell` — *layout shell for split-menu views; transparent.*
- (no action) `crates/core/src/ui/split_shell.rs:139` — `RightWingShell` — *right-wing layout shell; transparent.*

### 3.2 Electron titlebar (chrome decorations)

- (no action) `crates/core/src/ui/electron_titlebar.rs:51` — `ElectronNavButtons` — *nav buttons in the custom titlebar; inheriting from the titlebar parent is correct.*
- (no action) `crates/core/src/ui/electron_titlebar.rs:77` — `ElectronWindowControls` — *min/max/close buttons; no Poly menu warranted.*
- (no action) `crates/core/src/ui/electron_titlebar.rs:111` — `ElectronTitleBar` — *titlebar wrapper; transparent.*

### 3.3 Voice banner subcomponents (inside `VoiceBanner`)

- (no action) `crates/core/src/ui/voice_banner.rs:64` — `VoiceBannerParticipants`
- (no action) `crates/core/src/ui/voice_banner.rs:110` — `VoiceBannerChannelLink`
- (no action) `crates/core/src/ui/voice_banner.rs:166` — `VoiceBannerControls`
> All three are rendered inside `VoiceBanner` which itself doesn't yet have a typed menu.
> Given `VoiceBanner` is not a clickable item (it's an ambient HUD), `inherit` on all sub-
> pieces is correct — no one will right-click the voice HUD expecting a Poly menu.

### 3.4 Favorites sidebar subcomponents

- (no action) `crates/core/src/ui/favorites_sidebar.rs:62` — `NavBarSpacer` — *spacer; not interactive.*
- (no action) `crates/core/src/ui/favorites_sidebar.rs:78` — `SidebarTooltip` — *tooltip; inherits from enclosing server icon.*
- (no action) `crates/core/src/ui/favorites_sidebar.rs:393` — `AccountIcon` — *account icon row; the `ServerContextMenu` is on the parent `FavoritesBar`. However this is borderline — see note in Section 4.*
- (no action) `crates/core/src/ui/favorites_sidebar.rs:788` — `FavoriteServerIcon` — *server icon in the favorites bar. Has an explicit `oncontextmenu` inside its `rsx!` that calls `open_context_menu`. This predates the macro system. `inherit` on the component declaration is correct here as the menu is wired via raw handler, not the macro.*

### 3.5 Account server bar subcomponents

> `AccountServerBar` is the typed-menu host for `ServerContextMenu`. The sub-components
> below are all rendered inside it; `inherit` is correct.

- (no action) `crates/core/src/ui/account/common/account_server_bar.rs:106` — `AccountServerBar` — *hosts `ServerContextMenu`; is fine as `inherit` only if a parent declares the menu. It does not — `ServerContextMenu` is wired manually inside its `rsx!`. Same pattern as `FavoriteServerIcon`. Fine.*
- (no action) `crates/core/src/ui/account/common/account_server_bar.rs:225` — `AccountServerIcon`
- (no action) `crates/core/src/ui/account/common/account_server_bar.rs:396` — `ServerIconDisplay`
- (no action) `crates/core/src/ui/account/common/account_server_bar.rs:439` — `AccountBarDmsButton`
- (no action) `crates/core/src/ui/account/common/account_server_bar.rs:478` — `AccountBarFriendsButton`
- (no action) `crates/core/src/ui/account/common/account_server_bar.rs:516` — `AccountBarNotifsButton`
- (no action) `crates/core/src/ui/account/common/account_server_bar.rs:575` — `CreateServerButton`

### 3.6 Account bar subcomponents (inside `AccountBar`)

- (no action) `crates/core/src/ui/account/common/account_bar.rs:113` — `AccountBarUserInfo`
- (no action) `crates/core/src/ui/account/common/account_bar.rs:188` — `AccountProfilePopup`
- (no action) `crates/core/src/ui/account/common/account_bar.rs:282` — `AccountBarControls`
- (no action) `crates/core/src/ui/account/common/account_bar.rs:360` — `AccountBar`
- (no action) `crates/core/src/ui/account/common/account_switcher.rs:24` — `AccountSwitcher`

### 3.7 Channel list subcomponents (inside channel-list typed-menu host `ChannelContextMenu`)

> `ChannelContextMenu` owns the typed menu. The following render inside `ChannelList`
> which hosts that context-menu via the existing raw `oncontextmenu` handler.

- (no action) `crates/core/src/ui/account/common/channel_list.rs:456` — `ServerBanner`
- (no action) `crates/core/src/ui/account/common/channel_list.rs:646` — `DMFriendsView`
- (no action) `crates/core/src/ui/account/common/channel_list.rs:812` — `ServerChannelView`
- (no action) `crates/core/src/ui/account/common/channel_list.rs:1011` — `ChannelsRolesPanel`
- (no action) `crates/core/src/ui/account/common/channel_list.rs:1068` — `DMChannelItem`
- (no action) `crates/core/src/ui/account/common/channel_list.rs:1170` — `GroupChannelItem`
- (no action) `crates/core/src/ui/account/common/channel_list.rs:1242` — `FriendItem`
- (no action) `crates/core/src/ui/account/common/channel_list.rs:1285` — `CategorySection`
- (no action) `crates/core/src/ui/account/common/channel_list.rs:1325` — `ChannelItemRow`
- (no action) `crates/core/src/ui/account/common/channel_list.rs:1488` — `VoiceParticipantEntry`

### 3.8 Chat view subcomponents (inside `ChatView` which has `MsgContextMenuOverlay`)

> `ChatView` at `:885` owns `MsgContextMenuOverlay` for message-level menus. The following
> subcomponents are inside that host and `inherit` propagates correctly.

- (no action) `crates/core/src/ui/account/common/chat_view.rs:885` — `ChatView`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:2546` — `HeaderOverflowItem`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:2571` — `ChatHeaderActions`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5159` — `SearchFilterRow`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5185` — `SearchResultCard`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5240` — `PinnedMessageCard`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5289` — `SearchPreviewText`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5367` — `MessageContentView`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5450` — `AttachmentsView`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5536` — `ReactionsView`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5733` — `MessageInlineEdit`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5799` — `MsgContextMenuOverlay` — *the context menu overlay itself; inheriting is correct, menu opens inline.*
- (no action) `crates/core/src/ui/account/common/chat_view.rs:5984` — `ContextMenuItemSimple`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:6014` — `MessageReplyPreviewLine`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:6029` — `ReplyComposerBar`
- (no action) `crates/core/src/ui/account/common/chat_view.rs:6056` — `SlashCommandPopup`

### 3.9 Server context menu subcomponents (inside `ServerContextMenu`)

> These render inside the context menu itself; inheriting from the typed host is correct.

- (no action) `crates/core/src/ui/account/server/context_menu.rs:37` — `ServerContextMenu`
- (no action) `crates/core/src/ui/account/server/context_menu.rs:262` — `ContextMenuItem`
- (no action) `crates/core/src/ui/account/server/context_menu.rs:284` — `ContextMenuToggle`
- (no action) `crates/core/src/ui/account/server/context_menu.rs:308` — `RemoveFavoritesConfirm`

### 3.10 Channel context menu subcomponents

- (no action) `crates/core/src/ui/account/common/channel_context_menu.rs:29` — `ChannelContextMenu`
- (no action) `crates/core/src/ui/account/common/channel_context_menu.rs:151` — `ChannelMenuItem`

### 3.11 Voice view subcomponents (inside `VoiceChannelView`)

- (no action) `crates/core/src/ui/account/common/voice_view.rs:356` — `VoiceHeader`
- (no action) `crates/core/src/ui/account/common/voice_view.rs:391` — `VoiceScreenShareArea`
- (no action) `crates/core/src/ui/account/common/voice_view.rs:445` — `VoiceParticipantGrid`
- (no action) `crates/core/src/ui/account/common/voice_view.rs:480` — `VoiceTile`
- (no action) `crates/core/src/ui/account/common/voice_view.rs:568` — `VoiceJoinButton`
- (no action) `crates/core/src/ui/account/common/voice_view.rs:612` — `VoiceChatBar`

### 3.12 Voice bar subcomponents (inside `VoiceBar`)

- (no action) `crates/core/src/ui/account/common/voice_bar.rs:110` — `VoiceBar`
- (no action) `crates/core/src/ui/account/common/voice_bar.rs:151` — `VoiceDockInfo`
- (no action) `crates/core/src/ui/account/common/voice_bar.rs:176` — `VoiceDockParticipants`
- (no action) `crates/core/src/ui/account/common/voice_bar.rs:191` — `VoiceDockTile`
- (no action) `crates/core/src/ui/account/common/voice_bar.rs:241` — `VoiceDockControls`
- (no action) `crates/core/src/ui/account/common/voice_bar.rs:391` — `VoiceLatencyBar`
- (no action) `crates/core/src/ui/account/common/voice_bar.rs:440` — `VoicePreviewPanel`
- (no action) `crates/core/src/ui/account/common/voice_account_footer.rs:7` — `VoiceAccountFooter`

### 3.13 Thread view subcomponents (inside `ThreadPanel` / `ThreadFullView`)

- (no action) `crates/core/src/ui/account/common/thread_view.rs:78` — `ViewThreadButton`
- (no action) `crates/core/src/ui/account/common/thread_view.rs:147` — `ActiveThreadsBar`
- (no action) `crates/core/src/ui/account/common/thread_view.rs:251` — `ThreadPanel`
- (no action) `crates/core/src/ui/account/common/thread_view.rs:324` — `ThreadPanelHeader`
- (no action) `crates/core/src/ui/account/common/thread_view.rs:419` — `ThreadFullView`

### 3.14 Notifications subcomponents

- (no action) `crates/core/src/ui/account/common/notifications.rs:290` — `NotificationSidebarButton`
- (no action) `crates/core/src/ui/account/common/notifications.rs:317` — `NotificationFilter`
- (no action) `crates/core/src/ui/account/common/notifications.rs:354` — `NotificationList`

### 3.15 Saved items subcomponents (no typed parent yet → see Section 2.4 for `SavedPinnedItemCard`)

- (no action) `crates/core/src/ui/account/common/saved_items_view.rs:48` — `HighlightedSavedText`
- (no action) `crates/core/src/ui/account/common/saved_items_view.rs:104` — `SavedItemsView` — *top-level component for the saved items panel; `none` would be appropriate here too, but it is also an acceptable `inherit` passthrough until `SavedPinnedItemCard` gets a typed menu.*
- (no action) `crates/core/src/ui/account/common/saved_items_view.rs:341` — `SidebarSourceButton`

### 3.16 Friends panel subcomponents (except `FriendsGrid` and `BlockedUsersGrid`)

- (no action) `crates/core/src/ui/account/common/friends_panel.rs:148` — `SidebarMenuButton`
- (no action) `crates/core/src/ui/account/common/friends_panel.rs:168` — `FriendsFilterBar`
- (no action) `crates/core/src/ui/account/common/friends_panel.rs:314` — `IgnoredUsersPlaceholder`

### 3.17 Draft banner subcomponents

- (no action) `crates/core/src/ui/account/common/draft_banner.rs:247` — `DraftBannerRow`
- (no action) `crates/core/src/ui/account/common/draft_banner.rs:456` — `DraftsSidebarRow`

### 3.18 Media picker subcomponents (inside `MediaPickerPopup`)

- (no action) `crates/core/src/ui/account/common/media_picker.rs:119` — `SidebarIcon`
- (no action) `crates/core/src/ui/account/common/media_picker.rs:157` — `EmojiSectionBlock`
- (no action) `crates/core/src/ui/account/common/media_picker.rs:216` — `EmojiTabContent`
- (no action) `crates/core/src/ui/account/common/media_picker.rs:347` — `PlaceholderTabContent`
- (no action) `crates/core/src/ui/account/common/media_picker.rs:360` — `MediaPickerFooter`
- (no action) `crates/core/src/ui/account/common/media_picker.rs:388` — `MediaPickerPopup`

### 3.19 Search page subcomponents

- (no action) `crates/core/src/ui/search.rs:112` — `SearchInput`
- (no action) `crates/core/src/ui/search.rs:154` — `AvatarIcon`
- (no action) `crates/core/src/ui/search.rs:182` — `AccountFilter`
- (no action) `crates/core/src/ui/search.rs:220` — `NodeRow`
- (no action) `crates/core/src/ui/search.rs:250` — `AvatarNodeRow`
- (no action) `crates/core/src/ui/search.rs:293` — `HighlightedSearchText`
- (no action) `crates/core/src/ui/search.rs:323` — `ServerNode`
- (no action) `crates/core/src/ui/search.rs:444` — `TypeFilters`

### 3.20 Agent page subcomponents

- (no action) `crates/core/src/ui/agent/mod.rs:110` — `AgentSearchBar`
- (no action) `crates/core/src/ui/agent/mod.rs:141` — `AgentContentHeader`
- (no action) `crates/core/src/ui/agent/mod.rs:154` — `AgentNavigation`
- (no action) `crates/core/src/ui/agent/mod.rs:201` — `AgentAllSections`
- (no action) `crates/core/src/ui/agent/integrations.rs:53` — `IntegrationItem`
- (no action) `crates/core/src/ui/agent/integrations.rs:70` — `McpToggleRow`
- (no action) `crates/core/src/ui/agent/integrations.rs:129` — `McpConfigBlock`
- (no action) `crates/core/src/ui/agent/integrations.rs:172` — `Integrations`
- (no action) `crates/core/src/ui/agent/profile.rs:32` — `AgentProfile`

### 3.21 Settings backup subcomponents (inside the `BackupSettings` host)

- (no action) `crates/core/src/ui/settings/backup.rs:334` — `ProbeStatusBox`
- (no action) `crates/core/src/ui/settings/backup.rs:375` — `WizardAuthStatusBox`
- (no action) `crates/core/src/ui/settings/backup.rs:399` — `ReauthForm`
- (no action) `crates/core/src/ui/settings/backup.rs:458` — `ServerCard`
- (no action) `crates/core/src/ui/settings/backup.rs:549` — `WizardStep1`
- (no action) `crates/core/src/ui/settings/backup.rs:644` — `WizardStep2`
- (no action) `crates/core/src/ui/settings/backup.rs:732` — `AddServerWizard`

### 3.22 Settings theme subcomponents

- (no action) `crates/core/src/ui/settings/theme.rs:98` — `ThemePresetPicker`
- (no action) `crates/core/src/ui/settings/theme.rs:145` — `ThemeColorModeSelector`
- (no action) `crates/core/src/ui/settings/theme.rs:195` — `ThemeColorCustomizer`
- (no action) `crates/core/src/ui/settings/theme.rs:247` — `ColorOverridesToggleRow`
- (no action) `crates/core/src/ui/settings/theme.rs:267` — `ColorOverridesGrid`
- (no action) `crates/core/src/ui/settings/theme.rs:311` — `ResetColorsButton`
- (no action) `crates/core/src/ui/settings/theme.rs:333` — `ThemeCssEditor`
- (no action) `crates/core/src/ui/settings/theme.rs:358` — `CssEditorToggleRow`
- (no action) `crates/core/src/ui/settings/theme.rs:378` — `CssEditorArea`
- (no action) `crates/core/src/ui/settings/theme.rs:401` — `CssEditorActions`

### 3.23 Settings identity subcomponents

- (no action) `crates/core/src/ui/settings/identity.rs:93` — `MnemonicModal`
- (no action) `crates/core/src/ui/settings/identity.rs:282` — `IdentityCard`

### 3.24 Settings other subsection helpers

- (no action) `crates/core/src/ui/settings/language.rs:115` — `LangRow`
- (no action) `crates/core/src/ui/settings/media.rs:73` — `ProviderPanel`
- (no action) `crates/core/src/ui/settings/media.rs:122` — `ProviderTabs`
- (no action) `crates/core/src/ui/settings/voice_video.rs:89` — `DeviceSelectRow`
- (no action) `crates/core/src/ui/settings/voice_video.rs:104` — `MicTestRow`
- (no action) `crates/core/src/ui/settings/voice_video.rs:129` — `VoiceModeRow`
- (no action) `crates/core/src/ui/settings/voice_video.rs:158` — `EchoCancellationRow`
- (no action) `crates/core/src/ui/settings/voice_video.rs:267` — `VolumeSlider`
- (no action) `crates/core/src/ui/settings/voice_video.rs:292` — `NoiseSuppressionRow`
- (no action) `crates/core/src/ui/settings/plugins.rs:154` — `NativePluginRow`
- (no action) `crates/core/src/ui/settings/plugins.rs:207` — `WasmPluginRow`
- (no action) `crates/core/src/ui/settings/plugins.rs:257` — `AddWasmPlugin`
- (no action) `crates/core/src/ui/settings/general.rs:138` — `LayoutModeButton`
- (no action) `crates/core/src/ui/settings/general.rs:152` — `LayoutModeSelector`
- (no action) `crates/core/src/ui/settings/general.rs:223` — `MirrorMenuToggle`
- (no action) `crates/core/src/ui/settings/general.rs:253` — `MirrorChatMessagesToggle`
- (no action) `crates/core/src/ui/settings/general.rs:346` — `ResetButton`
- (no action) `crates/core/src/ui/settings/general.rs:388` — `ResetError`
- (no action) `crates/core/src/ui/settings/general.rs:401` — `ResetSection`

### 3.25 Client UI helpers (compositing / plugin view framework)

- (no action) `crates/core/src/ui/client_ui/composer.rs:100` — `ComposerHooks`
- (no action) `crates/core/src/ui/client_ui/composer.rs:201` — `MessageActions`
- (no action) `crates/core/src/ui/client_ui/custom_block.rs:305` — `CustomBlock`
- (no action) `crates/core/src/ui/client_ui/menu.rs:102` — `ClientMenu`
- (no action) `crates/core/src/ui/client_ui/settings_section.rs:96` — `PluginSettingsSection`
- (no action) `crates/core/src/ui/client_ui/settings_section.rs:174` — `PluginSettingField`
- (no action) `crates/core/src/ui/client_ui/toast.rs:125` — `ToastRow`
- (no action) `crates/core/src/ui/client_ui/view/card_body.rs:12` — `CardBody`
- (no action) `crates/core/src/ui/client_ui/view/header.rs:16` — `ViewHeader`
- (no action) `crates/core/src/ui/client_ui/view/mod.rs:38` — `ClientView`
- (no action) `crates/core/src/ui/client_ui/view/split_body.rs:28` — `SplitBody`
- (no action) `crates/core/src/ui/client_ui/view/toolbar.rs:49` — `ViewToolbar`
- (no action) `crates/core/src/ui/client_ui/view/list_body.rs:62` — `ListBody`
- (no action) `crates/core/src/ui/client_ui/view/tree_body.rs:43` — `TreeBody`
- (no action) `crates/core/src/ui/client_ui/view/list_body.rs:361` — `ViewRowDetail`
- (no action) `crates/core/src/ui/client_ui/sidebar/channel_list_layout.rs:19` — `ChannelListLayout`
- (no action) `crates/core/src/ui/client_ui/sidebar/communities.rs:66` — `CommunitiesLayout`
- (no action) `crates/core/src/ui/client_ui/sidebar/communities.rs:156` — `CommunitiesSubscribedBody`
- (no action) `crates/core/src/ui/client_ui/sidebar/custom.rs:17` — `CustomSidebar`
- (no action) `crates/core/src/ui/client_ui/sidebar/custom.rs:46` — `CustomSidebarSection`
- (no action) `crates/core/src/ui/client_ui/sidebar/feed.rs:38` — `FeedLayout`
- (no action) `crates/core/src/ui/client_ui/sidebar/mod.rs:52` — `ClientSidebar`
- (no action) `crates/core/src/ui/client_ui/sidebar/repo_tree.rs:39` — `RepoTreeLayout`
- (no action) `crates/core/src/ui/client_ui/sidebar/repo_tree.rs:120` — `RepoTabs`
- (no action) `crates/core/src/ui/client_ui/sidebar/spaces_rooms.rs:24` — `SpacesRoomsLayout`

### 3.26 Signup / onboarding flows

- (no action) `crates/core/src/ui/signup/mod.rs:454` — `AddAccountNav`
- (no action) `crates/core/src/ui/signup/mod.rs:522` — `TestAccountsPanel`
- (no action) `crates/core/src/ui/signup/mod.rs:738` — `ReauthNav`
- (no action) `crates/core/src/ui/setup_wizard.rs:8` — `FeatureCard` — *feature marketing card in the setup wizard; `none` would be fine too but passthrough is harmless.*
- (no action) `clients/server-client/src/signup.rs:92` — `PolySignupPage`
- (no action) `clients/server-client/src/signup.rs:144` — `UrlConnectForm`
- (no action) `clients/server-client/src/signup.rs:215` — `ExistingAccountsForm`
- (no action) `clients/server-client/src/signup.rs:313` — `SignupDetailsForm`

### 3.27 Conversation search subcomponents

- (no action) `crates/core/src/ui/account/common/conversation_search_view.rs:43` — `ConversationSearchInput`
- (no action) `crates/core/src/ui/account/common/conversation_search_view.rs:76` — `AvatarIcon`
- (no action) `crates/core/src/ui/account/common/conversation_search_view.rs:103` — `AvatarNodeRow`
- (no action) `crates/core/src/ui/account/common/conversation_search_view.rs:133` — `ConversationTypeFilters`
- (no action) `crates/core/src/ui/account/common/conversation_search_view.rs:169` — `ConversationSearchView`
- (no action) `crates/core/src/ui/account/common/new_conversation_view.rs:16` — `NewConversationView`

### 3.28 Server overview page helpers

- (no action) `crates/core/src/ui/account/common/user_sidebar.rs:71` — `HighlightedName` — *text highlight subcomponent; inherits from `UserSidebar`.*

### 3.29 Desktop devtools shell

- (no action) `apps/desktop-devtools/src/main.rs:349` — `DevtoolsShell` — *internal devtools UI; not user-facing. `inherit` or `none` equally fine.*

---

## 4. Cross-cutting findings

### 4.1 Pattern: entire settings subtrees stamped `inherit` where `none` suffices

The largest single group of misfiled `inherit` sites is in `crates/core/src/ui/settings/`
and `crates/core/src/ui/account/settings/`. Every settings section — navigation bars,
content headers, toggle rows, device pickers, wizard steps — was batch-annotated with
`inherit` during Phase C. None of these surfaces benefit from a Poly context menu or
require a native one. Changing them all to `none` makes the intent explicit and eliminates
any future ambiguity about whether a missing typed parent would accidentally expose the
native menu. This accounts for approximately 55 of the 72 "should be `none`" items.

### 4.2 Pattern: `UserRowContextMenu` exists but is not wired anywhere

`crates/core/src/ui/context_menu/menus.rs` defines `UserRowContextMenu` with `impl
ContextMenuFor<()>` (items: View Profile, Message, Mute, Block). However neither
`UserSidebar`, `DmUserSidebar`, nor `DmMemberRow` use it. The three user-row surfaces in
the UI simply carry `inherit`. This means right-clicking any user in the member list or DM
list triggers the root guard (native menu suppressed) but shows nothing. This is a
completed-infrastructure gap — the menu type exists, it just needs to be wired as
`#[context_menu(UserRowContextMenu)]` on `DmMemberRow` and the equivalent in
`user_sidebar.rs`. The `DmContactRow` inside `chat_view.rs:6158` has the same gap.

### 4.3 Pattern: modal bodies use `inherit` rather than `none`

`UserProfileModal`, `OutgoingDirectCallOverlay`, `CreateChannelPage`, `CreateServerPage`,
and `MessageMediaViewerOverlay` all carry `inherit`. Modals are typically non-content
surfaces where neither a Poly menu nor the native browser menu makes sense. They should
be `none`. `MessageMediaViewerOverlay` is the one exception: the image inside it should
be `allow_default` so users can Save Image via the native browser affordance — but the
surrounding modal chrome should be `none`.

### 4.4 Pattern: plugin view framework rows use `inherit`

`ListBodyRow` (`:275`) and `TreeBodyRow` (`:247`) are the lowest-level row components in
the plugin view framework. They are interactive surfaces that users will right-click, but
there is no `ClientViewRowContextMenu` API yet. Until that API is designed, `none` is
safer than `inherit` because it makes the suppression intentional. If a future plugin
needs to expose row-level actions, the API surface can be added and these components can
be re-annotated to a typed menu.

### 4.5 Pattern: `NoteEditor` inside `UserProfileModal` should be `allow_default`

`NoteEditor` at `user_profile_modal.rs:354` is a text area. It inherits from the modal.
This is the one case inside the modal subtree where the native context menu (Cut/Copy/
Paste/Select All) is genuinely useful. It should carry `#[context_menu(allow_default)]`
rather than `inherit`.

### 4.6 Pattern: `AccountServerBar` and `FavoriteServerIcon` own menus via raw handlers

`FavoriteServerIcon` and `AccountServerBar` declare `inherit` on the component
declaration but wire `oncontextmenu` handlers directly inside their `rsx!` bodies — these
predate the macro system. The `inherit` annotation is technically fine here because the
menu is already suppressed at the DOM level by the raw handler. This is a technical debt
item: ideally these would be migrated to `#[context_menu(ServerContextMenu)]` via the
macro, but they are not leaking the native menu and are not urgent.
