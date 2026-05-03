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
//! | `media_picker` | Unified media picker (emoji, GIF, stickers) + markdown toggle |
//! | `friends_panel` | Tiled friends browser |
//! | `notifications` | Aggregated notification feed across all backends |
//! | `user_sidebar` | Right-panel member list |
//! | `voice_bar` | Persistent voice connection status bar |
//! | `voice_view` | Voice/video participant tile view |

pub mod agent_panel;
pub mod account_bar;
pub mod discover_communities;
pub mod account_server_bar;
pub mod account_switcher;
pub mod attachment_context_menu;
pub mod avatar_context_menu;
pub mod account_context_menu;
pub mod channel_context_menu;
pub mod dm_context_menu;
pub mod group_dm_context_menu;
pub mod reaction_context_menu;
pub mod channel_list;
pub mod chat_history;
pub mod chat_view;
pub mod conversation_search_view;
pub mod direct_call;
pub mod direct_call_overlay;
pub mod discord_forum_view;
pub mod dm_user_sidebar;
pub mod draft_banner;
pub mod forum_composer;
pub mod emoji_picker;
pub mod forum_view;
pub mod friends_panel;
pub mod media_picker;
pub mod media_viewer;
pub mod new_conversation_view;
pub mod notifications;
pub mod saved_items_view;
pub mod thread_view;
pub mod overview_sidebar;
pub mod overview_subpages;
pub mod unsupported_placeholder;
pub mod user_profile_modal;
pub mod user_sidebar;
pub mod voice_account_footer;
pub mod voice_bar;
pub mod voice_view;

pub use agent_panel::AgentPanel;
pub use account_bar::AccountBar;
pub use draft_banner::{DraftBanner, DraftsSidebar};
pub use account_server_bar::AccountServerBar;
pub use account_switcher::AccountSwitcher;
pub use attachment_context_menu::AttachmentContextMenu;
pub use avatar_context_menu::AvatarContextMenu;
pub use account_context_menu::AccountContextMenu;
pub use channel_context_menu::ChannelContextMenu;
pub use dm_context_menu::DmContextMenu;
pub use group_dm_context_menu::GroupDmContextMenu;
pub use reaction_context_menu::ReactionContextMenu;
pub use channel_list::ChannelList;
pub use chat_view::ChatView;
pub use conversation_search_view::ConversationSearchView;
pub use direct_call_overlay::OutgoingDirectCallOverlay;
pub use dm_user_sidebar::DmUserSidebar;
pub use emoji_picker::EmojiPicker;
pub use discord_forum_view::DiscordForumView;
pub use forum_view::{ForumPostView, ForumView};
pub use friends_panel::FriendsPanel;
pub use media_picker::MediaPickerPopup;
pub use media_viewer::MessageMediaViewerOverlay;
pub use new_conversation_view::NewConversationView;
pub use notifications::NotificationsView;
pub use saved_items_view::SavedItemsView;
pub use thread_view::{ActiveThreadsBar, ThreadFullView, ThreadPanel, ViewThreadButton};
pub use overview_sidebar::OverviewSidebar;
pub use overview_subpages::{OverviewAgentsView, OverviewMissedView, OverviewStatsView};
pub use unsupported_placeholder::{FeatureUnsupportedPlaceholder, UnsupportedFeature};
pub use user_profile_modal::{UserProfileModal, open_user_profile};
pub use user_sidebar::UserSidebar;
pub use voice_account_footer::VoiceAccountFooter;
pub use voice_bar::VoiceBar;
pub use voice_view::VoiceChannelView;
