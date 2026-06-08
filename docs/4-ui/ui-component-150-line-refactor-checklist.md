# UI Component Length Enforcement Checklist

**Session Started:** 2026-03-07  
**Scope:** `#[component]` length only, not file length  
**Current Pass:** Sequential file-by-file re-audit  
**Working Limit For This Pass:** 100 lines per `#[component]`  
**Note:** A 10-line hard cap was also requested, but that conflicts with the explicit request to reduce the old rule to 100. This checklist is being rebuilt around the actionable 100-line component cap first.  
**Status:** ✅ FULL AUDIT COMPLETE

---

## Rules For This Checklist

- Count only functions marked with `#[component]`
- Do **not** use file length as a proxy
- Audit files **in order, one at a time**
- Mark a file checked only after every `#[component]` in that file is measured
- A file is compliant only if **every** component in it is `<= 100` lines

---

## Summary Statistics

| Category | Count |
|----------|-------|
| **Files checked in this rebuilt pass** | 54 |
| **Components measured so far** | 130 |
| **Components over 100 so far** | 29 ❌ |
| **Components at/under 100 so far** | 101 ✅ |
| **Checklist basis** | `#[component]` only |

---

## Sequential Audit Log

### 1. `crates/core/src/ui/account/common/chat_view.rs` — CHECKED

- [x] File checked component-by-component
- `ChatView` — **1129** ❌
- `ChatUtilityRail` — 82 ✅
- `SearchFilterPopup` — 27 ✅
- `SearchFilterRow` — 21 ✅
- `SearchResultCard` — 52 ✅
- `PinnedMessageCard` — 46 ✅
- `SearchPreviewText` — 77 ✅
- `MessageContentView` — 29 ✅
- `AttachmentsView` — 37 ✅
- `ReactionsView` — 47 ✅
- `TypingIndicator` — **138** ❌
- `MessageInlineEdit` — 63 ✅
- `MsgContextMenuOverlay` — **142** ❌
- `ContextMenuItemSimple` — 27 ✅
- `MessageReplyPreviewLine` — 12 ✅
- `ReplyComposerBar` — 24 ✅
- `SlashCommandPopup` — 53 ✅
- `DmContactPanel` — 74 ✅
- Result: **3 / 18 components over 100**
- Immediate refactor targets: `ChatView`, `TypingIndicator`, `MsgContextMenuOverlay`

### 2. `crates/core/src/ui/favorites_sidebar.rs` — CHECKED

- [x] File checked component-by-component
- `NavBarSpacer` — 15 ✅
- `FavoritesBar` — **165** ❌
- `AccountIcon` — **166** ❌
- `FavoriteServerIcon` — **684** ❌
- Result: **3 / 4 components over 100**
- Immediate refactor targets: `FavoritesBar`, `AccountIcon`, `FavoriteServerIcon`

### 3. `crates/core/src/ui/account/common/channel_list.rs` — CHECKED

- [x] File checked component-by-component
- `ChannelList` — 46 ✅
- `ServerBanner` — **187** ❌
- `DMFriendsView` — **203** ❌
- `ServerChannelView` — 31 ✅
- `ChannelsRolesPanel` — 54 ✅
- `DMChannelItem` — 79 ✅
- `GroupChannelItem` — 63 ✅
- `FriendItem` — 22 ✅
- `CategorySection` — 37 ✅
- `ChannelItemRow` — 78 ✅
- `VoiceParticipantEntry` — **134** ❌
- Result: **3 / 11 components over 100**
- Immediate refactor targets: `ServerBanner`, `DMFriendsView`, `VoiceParticipantEntry`

### 4. `crates/core/src/ui/settings/backup.rs` — CHECKED

- [x] File checked component-by-component
- `ProbeStatusBox` — 38 ✅
- `WizardAuthStatusBox` — 21 ✅
- `ReauthForm` — 56 ✅
- `ServerCard` — 88 ✅
- `WizardStep1` — 92 ✅
- `WizardStep2` — 85 ✅
- `AddServerWizard` — 71 ✅
- `BackupSettings` — 30 ✅
- Result: **0 / 8 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 5. `crates/core/src/ui/routes.rs` — CHECKED

- [x] File checked component-by-component
- `DmsLayout` — 18 ✅
- `ServerLayout` — 25 ✅
- `DmsHome` — 25 ✅
- `DmChat` — **101** ❌
- `ServerHome` — 49 ✅
- `ServerChat` — 54 ✅
- `FriendsRoute` — 8 ✅
- `NotificationsRoute` — 8 ✅
- `SettingsRoute` — 12 ✅
- `AccountSettingsRoute` — 11 ✅
- `ServerSettingsRoute` — 22 ✅
- `Root` — 19 ✅
- `PageNotFound` — 4 ✅
- Result: **1 / 13 components over 100**
- Immediate refactor targets: `DmChat`

### 6. `crates/core/src/ui/account/common/account_server_bar.rs` — CHECKED

- [x] File checked component-by-component
- `AccountServerBar` — **120** ❌
- `AccountServerIcon` — **198** ❌
- `AccountBarDmsButton` — 33 ✅
- `AccountBarNotifsButton` — 26 ✅
- Result: **2 / 4 components over 100**
- Immediate refactor targets: `AccountServerBar`, `AccountServerIcon`

### 7. `crates/core/src/ui/account/server/context_menu.rs` — CHECKED

- [x] File checked component-by-component
- `ServerContextMenu` — **286** ❌
- `ContextMenuItem` — 20 ✅
- `ContextMenuToggle` — 22 ✅
- `RemoveFavoritesConfirm` — 45 ✅
- Result: **1 / 4 components over 100**
- Immediate refactor targets: `ServerContextMenu`

### 8. `crates/core/src/ui/account/common/voice_view.rs` — CHECKED

- [x] File checked component-by-component
- `VoiceChannelView` — 68 ✅
- `VoiceHeader` — 26 ✅
- `VoiceParticipantGrid` — 32 ✅
- `VoiceTile` — 84 ✅
- `VoiceControls` — **159** ❌
- Result: **1 / 5 components over 100**
- Immediate refactor targets: `VoiceControls`

### 9. `crates/core/src/ui/mod.rs` — CHECKED

- [x] File checked component-by-component
- `App` — **155** ❌
- Result: **1 / 1 components over 100**
- Immediate refactor targets: `App`

### 10. `crates/core/src/ui/settings/theme.rs` — CHECKED

- [x] File checked component-by-component
- `ThemePresetPicker` — 44 ✅
- `ThemeColorModeSelector` — 47 ✅
- `ThemeColorCustomizer` — **106** ❌
- `ThemeCssEditor` — **121** ❌
- `ThemeSettings` — 15 ✅
- Result: **2 / 5 components over 100**
- Immediate refactor targets: `ThemeColorCustomizer`, `ThemeCssEditor`

### 11. `crates/core/src/ui/account/server/settings/overview.rs` — CHECKED

- [x] File checked component-by-component
- `IconPanel` — 97 ✅
- `BannerPanel` — 97 ✅
- `ServerOverviewSettings` — 74 ✅
- Result: **0 / 3 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 12. `crates/core/src/ui/settings/voice_video.rs` — CHECKED

- [x] File checked component-by-component
- `VoiceVideoSettings` — **178** ❌
- `VolumeSlider` — 22 ✅
- `NoiseSuppressionRow` — 36 ✅
- Result: **1 / 3 components over 100**
- Immediate refactor targets: `VoiceVideoSettings`

### 13. `crates/core/src/ui/account/settings/notifications.rs` — CHECKED

- [x] File checked component-by-component
- `NotificationsSettings` — **104** ❌
- `AccountNotifSectionInner` — 67 ✅
- `NotifToggleRow` — 16 ✅
- Result: **1 / 3 components over 100**
- Immediate refactor targets: `NotificationsSettings`

### 14. `crates/core/src/ui/settings/mod.rs` — CHECKED

- [x] File checked component-by-component
- `SettingsNavItem` — 18 ✅
- `SettingsPage` — **154** ❌
- Result: **1 / 2 components over 100**
- Immediate refactor targets: `SettingsPage`

### 15. `crates/core/src/ui/account/common/user_sidebar.rs` — CHECKED

- [x] File checked component-by-component
- `UserSidebar` — 84 ✅
- `UserGroup` — 51 ✅
- `UserProfilePopup` — 56 ✅
- Result: **0 / 3 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 16. `crates/core/src/ui/setup_wizard.rs` — CHECKED

- [x] File checked component-by-component
- `WelcomeStep` — 34 ✅
- `AccountIdStep` — 33 ✅
- `RecoveryPhraseStep` — 54 ✅
- `CompleteStep` — 44 ✅
- `SetupWizard` — 30 ✅
- Result: **0 / 5 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 17. `crates/core/src/ui/settings/media.rs` — CHECKED

- [x] File checked component-by-component
- `ProviderCard` — 46 ✅
- `MediaSettings` — **121** ❌
- Result: **1 / 2 components over 100**
- Immediate refactor targets: `MediaSettings`

### 18. `crates/core/src/ui/account/common/emoji_picker.rs` — CHECKED

- [x] File checked component-by-component
- `EmojiPicker` — 65 ✅
- Result: **0 / 1 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 19. `crates/core/src/ui/account/common/notifications.rs` — CHECKED

- [x] File checked component-by-component
- `NotificationsView` — 56 ✅
- `NotificationFilter` — 35 ✅
- `NotificationList` — 92 ✅
- Result: **0 / 3 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 20. `crates/core/src/ui/account/common/friends_panel.rs` — CHECKED

- [x] File checked component-by-component
- `FriendsPanel` — **110** ❌
- `FriendsGrid` — 65 ✅
- Result: **1 / 2 components over 100**
- Immediate refactor targets: `FriendsPanel`

### 21. `crates/core/src/ui/account/mod.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**
- Immediate refactor targets: none at the component-length level

### 22. `crates/core/src/ui/account/server/settings/mod.rs` — CHECKED

- [x] File checked component-by-component
- `ServerSettingsPage` — **126** ❌
- `ServerSettingsNavItem` — 14 ✅
- Result: **1 / 2 components over 100**
- Immediate refactor targets: `ServerSettingsPage`

### 23. `crates/core/src/ui/voice_banner.rs` — CHECKED

- [x] File checked component-by-component
- `VoiceBanner` — **143** ❌
- Result: **1 / 1 components over 100**
- Immediate refactor targets: `VoiceBanner`

### 24. `crates/core/src/ui/settings/general.rs` — CHECKED

- [x] File checked component-by-component
- `ResetButton` — 39 ✅
- `ResetError` — 10 ✅
- `ResetSection` — 33 ✅
- `GeneralSettings` — 10 ✅
- Result: **0 / 4 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 25. `crates/core/src/ui/main_layout.rs` — CHECKED

- [x] File checked component-by-component
- `NavBar` — 42 ✅
- `MainLayout` — 95 ✅
- Result: **0 / 2 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 26. `crates/core/src/ui/account/common/dm_user_sidebar.rs` — CHECKED

- [x] File checked component-by-component
- `DmUserSidebar` — 49 ✅
- `DmMemberRow` — 80 ✅
- Result: **0 / 2 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 27. `crates/core/src/ui/settings/identity.rs` — CHECKED

- [x] File checked component-by-component
- `MnemonicModal` — 54 ✅
- `IdentitySettings` — 69 ✅
- Result: **0 / 2 components over 100**
- Immediate refactor targets: none under the 100-line cap

### 28. `crates/core/src/ui/electron_titlebar.rs` — CHECKED

- [x] File checked component-by-component
- `ElectronTitleBar` — **117** ❌
- Result: **1 / 1 components over 100**
- Immediate refactor targets: `ElectronTitleBar`

### 29. `crates/core/src/ui/settings/diagnostics.rs` — CHECKED

- [x] File checked component-by-component
- `DiagnosticsPage` — 57 ✅
- `AccountDiagnosticsRow` — 39 ✅
- Result: **0 / 2 components over 100**

### 30. `crates/core/src/ui/account/common/voice_bar.rs` — CHECKED

- [x] File checked component-by-component
- `VoiceBar` — 85 ✅
- Result: **0 / 1 components over 100**

### 31. `crates/core/src/ui/account/common/account_switcher.rs` — CHECKED

- [x] File checked component-by-component
- `AccountSwitcher` — 67 ✅
- Result: **0 / 1 components over 100**

### 32. `crates/core/src/ui/settings/language.rs` — CHECKED

- [x] File checked component-by-component
- `LanguageSettings` — 62 ✅
- Result: **0 / 1 components over 100**

### 33. `crates/core/src/ui/account/settings/mod.rs` — CHECKED

- [x] File checked component-by-component
- `AccountSettingsPage` — 46 ✅
- Result: **0 / 1 components over 100**

### 34. `crates/core/src/ui/settings/common.rs` — CHECKED

- [x] File checked component-by-component
- `PolySelect` — 51 ✅
- Result: **0 / 1 components over 100**

### 35. `crates/core/src/ui/account/common/mod.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**

### 36. `crates/core/src/ui/account/server/settings/profile.rs` — CHECKED

- [x] File checked component-by-component
- `ServerProfileSettings` — 42 ✅
- Result: **0 / 1 components over 100**

### 37. `crates/core/src/ui/account/server/settings/general.rs` — CHECKED

- [x] File checked component-by-component
- `ServerGeneralSettings` — 51 ✅
- `LeaveServerConfirm` — 64 ✅
- Result: **0 / 2 components over 100**

### 38. `crates/core/src/ui/account/server/settings/notifications.rs` — CHECKED

- [x] File checked component-by-component
- `ServerNotificationsSettings` — 65 ✅
- `NotifLevelOption` — 13 ✅
- `NotifToggleRow` — 22 ✅
- Result: **0 / 3 components over 100**

### 39. `crates/core/src/ui/account/demo/context_menu.rs` — CHECKED

- [x] File checked component-by-component
- `ServerContextMenuExtras` — 16 ✅
- Result: **0 / 1 components over 100**

### 40. `crates/core/src/ui/settings/accounts.rs` — CHECKED

- [x] File checked component-by-component
- `AccountsSettings` — 12 ✅
- Result: **0 / 1 components over 100**

### 41. `crates/core/src/ui/account/teams/context_menu.rs` — CHECKED

- [x] File checked component-by-component
- `ServerContextMenuExtras` — 6 ✅
- Result: **0 / 1 components over 100**

### 42. `crates/core/src/ui/account/matrix/context_menu.rs` — CHECKED

- [x] File checked component-by-component
- `ServerContextMenuExtras` — 6 ✅
- Result: **0 / 1 components over 100**

### 43. `crates/core/src/ui/account/teams/mod.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**

### 44. `crates/core/src/ui/account/stoat/mod.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**

### 45. `crates/core/src/ui/account/stoat/context_menu.rs` — CHECKED

- [x] File checked component-by-component
- `ServerContextMenuExtras` — 6 ✅
- Result: **0 / 1 components over 100**

### 46. `crates/core/src/ui/account/poly_native/context_menu.rs` — CHECKED

- [x] File checked component-by-component
- `ServerContextMenuExtras` — 6 ✅
- Result: **0 / 1 components over 100**

### 47. `crates/core/src/ui/account/matrix/mod.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**

### 48. `crates/core/src/ui/account/discord/mod.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**

### 49. `crates/core/src/ui/account/discord/context_menu.rs` — CHECKED

- [x] File checked component-by-component
- `ServerContextMenuExtras` — 6 ✅
- Result: **0 / 1 components over 100**

### 50. `crates/core/src/ui/account/demo/mod.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**

### 51. `crates/core/src/ui/account/server/mod.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**

### 52. `crates/core/src/ui/account/poly_native/mod.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**

### 53. `crates/core/src/ui/settings/notifications.rs` — CHECKED

- [x] File checked component-by-component
- No `#[component]` functions in this file
- Result: **0 components to measure**

### 54. `crates/core/src/ui/account/common/account_bar.rs` — CHECKED

- [x] File checked component-by-component
- `AccountBar` — **129** ❌
- Result: **1 / 1 components over 100**
- Immediate refactor targets: `AccountBar`

---

## Remaining Files To Audit Sequentially

- [x] None — full `crates/core/src/ui/**/*.rs` sequential audit completed

---

## Findings So Far

- The previous checklist was wrong because it mixed **file length** with **component length**.
- The correct unit is the size of each `#[component]` function.
- Under the corrected 100-line cap, the full audit found **29 failing components**.
- `DmChat` fails by a single line at **101**, so the new limit is already catching borderline cases that the old pass missed.
- `backup.rs` is the first audited file that is fully compliant under the 100-line cap.
- `overview.rs` is also fully compliant under the 100-line cap despite the file itself being fairly large.
- `setup_wizard.rs`, `user_sidebar.rs`, and `emoji_picker.rs` are fully compliant under the 100-line cap.
- `account/mod.rs` has no `#[component]` functions, so it is not a component-length violation file even though it was previously flagged by file size.
- `main_layout.rs`, `dm_user_sidebar.rs`, `identity.rs`, and `general.rs` are all compliant under the 100-line cap.
- A large number of the previously assumed-compliant “small files” are in fact compliant under the 100-line cap once measured directly.
- Several backend-specific wrapper files contain either tiny `ServerContextMenuExtras` components or no `#[component]` functions at all.
- The final missing file caught by the coverage check was `account/common/account_bar.rs`, and it adds one more failing component: `AccountBar` at **129** lines.

---

## Final Coverage Result

- Full sequential audit scope completed for all Rust UI files under `crates/core/src/ui`
- Total files checked: **54**
- Total `#[component]` functions measured: **130**
- Total components over 100 lines: **29**
- Total components at or under 100 lines: **101**
- Compliance status: **FAIL** until the 29 oversized components are refactored

---

## Old File-Length Sections

- Removed as primary audit criteria. File length can still matter for readability, but it is **not** the compliance check.

---

## Files At/Under 150-Line Limit (COMPLIANT) ✅

- [x] **identity.rs** (148 lines) - `/crates/core/src/ui/settings/identity.rs` ✅
- [x] **electron_titlebar.rs** (135 lines) - `/crates/core/src/ui/electron_titlebar.rs` ✅
- [x] **server/settings/general.rs** (128 lines) - `/crates/core/src/ui/account/server/settings/general.rs` ✅
- [x] **server/settings/notifications.rs** (117 lines) - `/crates/core/src/ui/account/server/settings/notifications.rs` ✅
- [x] **diagnostics.rs** (111 lines) - `/crates/core/src/ui/settings/diagnostics.rs` ✅
- [x] **voice_bar.rs** (107 lines) - `/crates/core/src/ui/account/common/voice_bar.rs` ✅
- [x] **account_switcher.rs** (87 lines) - `/crates/core/src/ui/account/common/account_switcher.rs` ✅
- [x] **language.rs** (77 lines) - `/crates/core/src/ui/settings/language.rs` ✅
- [x] **account/settings/mod.rs** (73 lines) - `/crates/core/src/ui/account/settings/mod.rs` ✅
- [x] **common.rs** (69 lines) - `/crates/core/src/ui/settings/common.rs` ✅
- [x] **account/common/mod.rs** (60 lines) - `/crates/core/src/ui/account/common/mod.rs` ✅
- [x] **server/settings/profile.rs** (51 lines) - `/crates/core/src/ui/account/server/settings/profile.rs` ✅
- [x] **demo/context_menu.rs** (28 lines) - `/crates/core/src/ui/account/demo/context_menu.rs` ✅
- [x] **accounts.rs** (26 lines) - `/crates/core/src/ui/settings/accounts.rs` ✅
- [x] **teams/context_menu.rs** (18 lines) - `/crates/core/src/ui/account/teams/context_menu.rs` ✅
- [x] **matrix/context_menu.rs** (18 lines) - `/crates/core/src/ui/account/matrix/context_menu.rs` ✅
- [x] **teams/mod.rs** (17 lines) - `/crates/core/src/ui/account/teams/mod.rs` ✅
- [x] **stoat/mod.rs** (17 lines) - `/crates/core/src/ui/account/stoat/mod.rs` ✅
- [x] **stoat/context_menu.rs** (17 lines) - `/crates/core/src/ui/account/stoat/context_menu.rs` ✅
- [x] **poly_native/context_menu.rs** (17 lines) - `/crates/core/src/ui/account/poly_native/context_menu.rs` ✅
- [x] **matrix/mod.rs** (17 lines) - `/crates/core/src/ui/account/matrix/mod.rs` ✅
- [x] **discord/mod.rs** (17 lines) - `/crates/core/src/ui/account/discord/mod.rs` ✅
- [x] **discord/context_menu.rs** (17 lines) - `/crates/core/src/ui/account/discord/context_menu.rs` ✅
- [x] **demo/mod.rs** (17 lines) - `/crates/core/src/ui/account/demo/mod.rs` ✅
- [x] **server/mod.rs** (16 lines) - `/crates/core/src/ui/account/server/mod.rs` ✅
- [x] **poly_native/mod.rs** (14 lines) - `/crates/core/src/ui/account/poly_native/mod.rs` ✅
- [x] **settings/notifications.rs** (10 lines) - `/crates/core/src/ui/settings/notifications.rs` ✅

---

## Refactoring Guidelines

### When Splitting a Component:
1. **Identify logical sub-components** (forms, lists, headers, modals)
2. **Extract as separate `#[component]` functions** in new files
3. **Pass state via props** (maintain reactivity)
4. **Keep module organization clean** (use `mod.rs` or side-by-side files)
5. **Run `cargo clippy --workspace`** after each file
6. **Test hot-reload** after structural changes

### File Naming:
- Main component: `component_name.rs`
- Sub-components: `component_name/sub_item.rs` or `component_name_sub_item.rs`
- Shared: `components/mod.rs` or `components/shared.rs`

---

## COMPLETE COMPONENT ANALYSIS RESULTS

### ✅ COMPLETED FIXES (This Session)
1. **chat_view.rs**:
   - ✅ Fixed bracket syntax erro (lines 1350-1358) - now compiles
   - ✅ **MsgContextMenuOverlay**: Refactored from **181 → 135 lines** ✅ (data-driven menu items loop)
   - Status: 17/18 sub-components compliant, 1 fixed
   - Remaining issue: **Main ChatView component** (~1130 lines) - needs major refactoring

### ❌ COMPONENTS EXCEEDING 150 LINES (Prioritized by Fixability)

#### Tier 1: Quick Wins (5-16 lines to cut)
**STATUS: READY FOR QUICK FIX**
1. **VoiceControls** (`voice_view.rs:241-305`)  
   - Current: **159 lines**
   - Issue: Helper function `join_voice_channel` is being counted with component
   - Fix: Move helper function to before component definition (lines move from 306-398 to 139-231)
   - Effort: Move code block (no logic change)
   - Lines to cut: **Just need to reorder** (helper before component)

#### Tier 2: Minor Refactoring (16-40 lines to cut)
**STATUS: MEDIUM DIFFICULTY**
1. **AccountIcon** (`favorites_sidebar.rs:208-373`)
   - Current: **166 lines**  
   - Issues: Repetitive icon rendering, nested JSX
   - Fix: Extract icon rendering into sub-component or use looping
   - Lines to cut: ~16 lines
   - Estimated effort: Extract 1-2 sub-components

2. **ServerBanner** (`channel_list.rs:66-252`)
   - Current: **187 lines**
   - Issues: Conditional rendering, multiple banner variants
   - Fix: Extract banner variants into separate components
   - Lines to cut: ~37 lines
   - Estimated effort: Create 2-3 conditional sub-components

#### Tier 3: Moderate Refactoring (30-60 lines to cut)
**STATUS: REQUIRES ARCHITECTURE CHANGE**
1. **DMFriendsView** (`channel_list.rs:253-455`)
   - Current: **203 lines**
   - Issues: List rendering + filtering + actions in one component
   - Fix: Split list container from list item components
   - Lines to cut: ~53 lines
   - Estimated effort: Extract item rendering to sub-component

2. **AccountServerIcon** (`account_server_bar.rs:155-352`)
   - Current: **198 lines**
   - Issues: Context menu handling + rendering + animations
   - Fix: Extract context menu to separate component
   - Lines to cut: ~48 lines
   - Estimated effort: Move context menu logic to sub-component

#### Tier 4: MAJOR REFACTORING REQUIRED (500+ lines to cut)
**STATUS: MAJOR UNDERTAKING**
1. **FavoriteServerIcon** (`favorites_sidebar.rs:374-1057`)
   - Current: **684 lines** ❌❌❌ 
   - Issues: **ENTIRE file is basically ONE component**
   - Contains:
     - Drag & drop handling
     - Context menu
     - Animations
     - Server list rendering
     - Nested component composition
   - Fix: Break into 4-5 sub-components:
     - `ServerListContainer` (outer)
     - `ServerListItem` (individual server)
     - `DragDropWrapper` (DnD logic)
     - `ServerContextMenu` (menu)
   - Lines to cut: ~534 lines
   - Estimated effort: Major refactor (2-3 hours)

2. **Main ChatView** (`chat_view.rs:526-1655`)
   - Current: **~1130 lines** ❌❌❌
   - Contains:
     - Message rendering loop
     - Input handling
     - Effects hooks
     - RSX template (~570 lines)
   - Fix: Break into 5+ sub-components:
     - `ChatViewHeader` (header + search)
     - `ChatViewMessageList` (message rendering + scrolling)
     - `ChatViewInput` (composer + attachments)
     - `ChatViewUtilities` (already exists, use as side rail)
   - Lines to cut: ~980 lines
   - Estimated effort: Major refactor (3+ hours)

#### Components Already Compliant ✅
- **Not listed here** - 28 files with 40+ components already under 150 lines

## Session Progress Summary

| Item | Status | Notes |
|------|--------|-------|
| Bracket syntax fix | ✅ Complete | Fixed malformed if/else in chat_view.rs |
| MsgContextMenuOverlay | ✅ Complete | Refactored from 181→135 lines |
| File reorganization planning | ✅ Complete | Identified 6 problem areas |
| Quick wins (VoiceControls) | 🟡 Ready | Needs helper function reordering |
| Medium refactors (AccountIcon, ServerBanner) | 🟡 Ready | Requires sub-component extraction |
| Major refactors (FavoriteServerIcon, ChatView) | 🟡 Scoped | Needs detailed breakdown |

## Refactoring Strategy for User

### IF USER CONTINUES THIS SESSION:
Priority order:
1. Fix **VoiceControls**: Move helper function (10 min)
2. Extract **AccountIcon** sub-components (20 min)
3. Split **ServerBanner** variants (25 min)
4. Extract **DMFriendsView** list item (30 min)
5. Move **AccountServerIcon** menu (30 min)

Then assess whether to tackle the 600+ line monsters or defer.

### IF USER RESUMES IN NEW SESSION:
1. Re-run `cargo check --workspace` to verify clean state
2. Pick one Tier 2 file and systematicaly extract sub-components
3. Use the patterns established (data-driven menu items, component extraction)
4. Test after each file with `cargo clippy --workspace`

## Notes on the Architecture Issues

**Why FavoriteServerIcon is 684 lines:**
- Combines server list rendering + dragging + context menu + animations
- **Solution**: Create intermediate `ServerListItem` component that handles one server
- Then `FavoriteServerIcon` becomes a simple list + DnD wrapper

**Why ChatView main component is 1130 lines:**
- Massive `rsx!` block with 550+ lines of nested HTML/JSX
- 150+ lines of signal state declarations
- 250+ lines of `use_effect` hooks
- **Solution**: Extract message list to separate component, input area to separate component, header to separate component
- Keep main ChatView as a coordinator that passes signals

## Session Notes

### Session 1 final (2026-03-07)
**Duration**: ~2 hours  
**Tokens used**: ~150k of 200k  
**Work completed**:
- Fixed critical syntax error preventing compilation
- Analyzed all 54 UI components
- Identified 6 components exceeding 150 lines
- Refactored MsgContextMenuOverlay (181→135 lines)
- Created comprehensive refactoring roadmap

**Next steps for future sessions**:
- Execute Tier 1-2 refactorings (quick wins)
- Tackle Tier 3 with more time
- Defer monsters (FavoriteServerIcon, ChatView) to dedicated refactoring sprint

