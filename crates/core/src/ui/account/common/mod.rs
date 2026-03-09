//! Common account-scoped UI components shared by ALL messenger backends.
//!
//! These components provide the default/shared implementation of the
//! account-scoped UI. Backend-specific overrides live in sibling modules
//! (`demo/`, `stoat/`, `discord/`, `matrix/`, `teams/`, `poly_native/`).
//!
//! ## Architecture
//! ```text
//! ui/account/
//! ├── common/          ← YOU ARE HERE — shared UI components
//! ├── demo/            ← Demo backend overrides
//! ├── stoat/           ← Stoat backend overrides
//! ├── discord/         ← Discord backend overrides
//! ├── matrix/          ← Matrix backend overrides
//! ├── teams/           ← Teams backend overrides
//! ├── poly_native/     ← Poly native server overrides
//! ├── server/          ← Server-scoped components (context menu, settings)
//! └── settings/        ← Account-scoped settings
//! ```
//!
//! ## Sub-modules
//! | Module | Contents |
//! |---|---|
//! | `account_bar` | Bottom user-info panel (avatar, name, mic/speaker shortcuts) |
//! | `account_server_bar` | Bar 2 — DMs, Notifications, Servers navigation |
//! | `account_switcher` | Multi-account switcher bar in DM view |
//! | `channel_list` | Channel/DM list for the selected server or DM home |
//! | `chat_view` | Message list + input box |
//! | `emoji_picker` | Emoji grid for reactions and input |
//! | `friends_panel` | Tiled friends browser |
//! | `notifications` | Aggregated notification feed across all backends |
//! | `user_sidebar` | Right-panel member list |
//! | `voice_bar` | Persistent voice connection status bar |
//! | `voice_view` | Voice/video participant tile view |

pub mod account_bar;
pub mod account_server_bar;
pub mod account_switcher;
pub mod channel_list;
pub mod chat_history;
pub mod chat_view;
pub mod dm_user_sidebar;
pub mod emoji_picker;
pub mod friends_panel;
pub mod notifications;
pub mod user_sidebar;
pub mod voice_account_footer;
pub mod voice_bar;
pub mod voice_view;

pub use account_bar::AccountBar;
pub use account_server_bar::AccountServerBar;
pub use account_switcher::AccountSwitcher;
pub use channel_list::ChannelList;
pub use chat_view::ChatView;
pub use dm_user_sidebar::DmUserSidebar;
pub use emoji_picker::EmojiPicker;
pub use friends_panel::FriendsPanel;
pub use notifications::NotificationsView;
pub use user_sidebar::UserSidebar;
pub use voice_account_footer::VoiceAccountFooter;
pub use voice_bar::VoiceBar;
pub use voice_view::VoiceChannelView;
