//! Account-scoped UI components for Poly.
//!
//! All components in this module are tied to a specific messenger account
//! and are only meaningful when an account is active. App-level chrome
//! (FavoritesBar, VoiceBanner, MainLayout, SetupWizard) lives in the parent
//! `ui` module instead.
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
//! | `settings` | Account-scoped settings (notifications only) |

pub mod settings;

pub mod account_bar;
pub mod account_server_bar;
pub mod account_switcher;
pub mod channel_list;
pub mod chat_view;
pub mod emoji_picker;
pub mod friends_panel;
pub mod notifications;
pub mod user_sidebar;
pub mod voice_bar;
pub mod voice_view;

pub use account_bar::AccountBar;
pub use account_server_bar::AccountServerBar;
pub use account_switcher::AccountSwitcher;
pub use channel_list::ChannelList;
pub use chat_view::ChatView;
pub use emoji_picker::EmojiPicker;
pub use friends_panel::FriendsPanel;
pub use notifications::NotificationsView;
pub use settings::AccountSettingsPage;
pub use user_sidebar::UserSidebar;
pub use voice_bar::VoiceBar;
pub use voice_view::VoiceChannelView;
