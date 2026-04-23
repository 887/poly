//! Account-scoped UI components for Poly — multi-backend architecture.
//!
//! All components in this module are tied to a specific messenger account
//! and are only meaningful when an account is active. App-level chrome
//! (FavoritesBar, VoiceBanner, MainLayout, SetupWizard) lives in the parent
//! `ui` module instead.
//!
//! ## Multi-Backend UI Architecture
//!
//! Poly is a multi-backend messenger client. Each backend (Demo, Stoat,
//! Discord, Matrix, Teams, Poly native) can have different UI needs —
//! different context menu items, settings panels, channel decorations, etc.
//!
//! The UI is split into:
//! - **`common/`** — Shared components used by ALL backends (channel list,
//!   chat view, voice view, etc.)
//! - **Per-backend directories** (`demo/`, `stoat/`, `discord/`, `matrix/`,
//!   `teams/`, `poly_native/`) — Backend-specific overrides and additions
//!
//! Dispatch is done by matching on [`poly_client::BackendType`] at render time.
//! The `:backend` slug is always available from the current route URL.
//!
//! See `docs/multi-client-architecture.md` for the full architecture guide.
//!
//! ## Directory structure
//! ```text
//! ui/account/
//! ├── mod.rs              ← YOU ARE HERE — re-exports + dispatch
//! ├── common/             ← Shared UI (channel list, chat, voice, etc.)
//! ├── demo/               ← Demo backend overrides
//! ├── stoat/              ← Stoat backend overrides
//! ├── discord/            ← Discord backend overrides
//! ├── matrix/             ← Matrix backend overrides
//! ├── teams/              ← Teams backend overrides
//! ├── poly_native/        ← Poly native server overrides
//! ├── server/             ← Server-scoped components (context menu, settings)
//! └── settings/           ← Account-scoped settings
//! ```
//!
//! ## Sub-modules (common)
//! | Module | Contents |
//! |---|---|
//! | `common::account_bar` | Bottom user-info panel (avatar, name, mic/speaker shortcuts) |
//! | `common::account_server_bar` | Bar 2 — DMs, Notifications, Servers navigation |
//! | `common::account_switcher` | Multi-account switcher bar in DM view |
//! | `common::channel_list` | Channel/DM list for the selected server or DM home |
//! | `common::chat_view` | Message list + input box |
//! | `common::emoji_picker` | Emoji grid for reactions and input |
//! | `common::friends_panel` | Tiled friends browser |
//! | `common::notifications` | Aggregated notification feed across all backends |
//! | `common::user_sidebar` | Right-panel member list |
//! | `common::voice_bar` | Persistent voice connection status bar |
//! | `common::voice_view` | Voice/video participant tile view |
//!
//! ## Sub-modules (per-backend)
//! | Module | Contents |
//! |---|---|
//! | `demo` | Demo backend context menu, settings overrides |
//! | `stoat` | Stoat (Revolt) backend overrides |
//! | `discord` | Discord backend overrides |
//! | `matrix` | Matrix backend overrides |
//! | `teams` | Microsoft Teams backend overrides |
//! | `poly_native` | Poly native server backend overrides |

// ── Common (shared across all backends) ──────────────────────────────────────
pub mod channel;
pub mod common;
pub mod server;
pub mod settings;

// ── Per-backend UI overrides (feature-gated) ─────────────────────────────────
#[cfg(feature = "demo")]
pub mod demo;

#[cfg(feature = "stoat")]
pub mod stoat;

#[cfg(feature = "discord")]
pub mod discord;

#[cfg(feature = "matrix")]
pub mod matrix;

#[cfg(feature = "teams")]
pub mod teams;

// Poly native server — always compiled (it's our own protocol)
pub mod poly_native;

// ── Re-exports (common components) ───────────────────────────────────────────
// These re-exports preserve backward compatibility — existing code that does
// `use super::account::ChannelList` continues to work.
pub use common::AccountBar;
pub use common::AccountServerBar;
pub use common::AccountSwitcher;
pub use common::AttachmentContextMenu;
pub use common::AvatarContextMenu;
pub use common::ChannelContextMenu;
pub use common::ReactionContextMenu;
pub use common::ChannelList;
pub use common::ChatView;
pub use common::ConversationSearchView;
pub use common::DmUserSidebar;
pub use common::EmojiPicker;
pub use common::DiscordForumView;
pub use common::{ForumPostView, ForumView};
pub use common::FriendsPanel;
pub use common::NewConversationView;
pub use common::NotificationsView;
pub use common::OutgoingDirectCallOverlay;
pub use common::SavedItemsView;
pub use common::{ActiveThreadsBar, ThreadFullView, ThreadPanel, ViewThreadButton};
pub use common::UserSidebar;
pub use common::VoiceBar;
pub use common::VoiceChannelView;
pub use common::{UserProfileModal, open_user_profile};
pub use channel::ChannelSettingsPage;
pub use server::{ServerContextMenu, ServerSettingsPage};
pub use settings::AccountSettingsPage;

// ── Backend dispatch ─────────────────────────────────────────────────────────
//
// Backend-specific UI used to live in per-backend Rust modules
// (`{demo,stoat,discord,matrix,teams,poly_native}::context_menu`). Those are
// gone — backends now declare their own context-menu items via the
// `client-menus` WIT interface and the host renders them uniformly through
// [`crate::ui::client_ui::ClientMenu`].
