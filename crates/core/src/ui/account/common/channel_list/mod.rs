//! Channel list — categories and channels for the selected server.
//!
//! Common implementation shared across all messenger backends.
//! Backend-specific channel list decorations live in per-backend
//! directories (`demo/`, `stoat/`, etc.).
//!
//! ## Module layout (C.2 — SOLID split)
//!
//! | Sub-module | Contents |
//! |---|---|
//! | `server_view` | `ServerBanner`, `ServerChannelView`, `ChannelsRolesPanel`, `load_channel_data`. Category/permission filtering is isolated in `ServerChannelFilter` (B.4). |
//! | `dm_view` | `DMFriendsView`, `load_dm_messages`, `activate_dm_channel`, `open_direct_message_from_active_account`, sort helpers. |
//! | `friends_view` | `FriendItem`. |
//! | `items` | `ChannelItemRow`, `CategorySection`, `VoiceParticipantEntry`, `DMChannelItem`, `GroupChannelItem`. |
//!
//! This `mod.rs` is a thin re-export surface + the top-level `ChannelList`
//! component (the only public entry-point for callers in `account/common`).

mod dm_view;
mod friends_view;
mod items;
mod server_view;

// Re-export symbols callers in sibling modules (`common/`) need.
// `pub(super)` makes the fn visible to `common` (the parent of `channel_list`)
// but not to the wider crate — matching the original file's access level.
pub(super) use dm_view::open_direct_message_from_active_account;

use crate::state::BatchedSignal;
use crate::state::{ChatViewState, View};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};
use server_view::{ServerBanner, ServerChannelView};

/// Actions for the channel list sidebar.
#[derive(Debug, Clone)]
pub enum ChannelListAction {
    /// User selected a server channel.
    SelectChannel(String),
    /// User selected a DM channel.
    SelectDm(String),
    /// User selected a group channel.
    SelectGroup(String),
    /// User clicked "New Conversation".
    NewConversation,
    /// User clicked "Saved Messages".
    OpenSavedMessages,
    /// User opened the server dropdown.
    ToggleServerDropdown,
}

impl UiAction for ChannelListAction {
    fn apply(self, _cx: ActionCx<'_>) {
        todo!("phase-E: ChannelListAction requires Signal + async handles");
    }
}

/// Main channel list component — delegates to sub-views based on current view.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(ChannelListAction)]
#[component]
pub fn ChannelList() -> Element {
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let current_view = *nav.read().view;
    let current_server = chat_view_state.read().current_server.clone(); // poly-lint: allow render-time-read — render snapshot for conditional rendering; subscription intentional
    let visible_category_ids = use_signal(Vec::<String>::new);

    rsx! {
        aside { class: "channel-list",
            ServerBanner {
                current_view,
                current_server: current_server.clone(),
                visible_category_ids,
            }

            div { class: "channel-entries",
                if current_view == View::DmsFriends {
                    dm_view::DMFriendsView {}
                } else if current_server.is_some() {
                    ServerChannelView { visible_category_ids }
                } else {
                    div { class: "channel-empty",
                        p { "{crate::i18n::t(\"chat-no-messages\")}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn channel_list_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<ChannelListAction>();
        let _ = ChannelListAction::SelectChannel("ch".into());
        let _ = ChannelListAction::SelectDm("dm".into());
        let _ = ChannelListAction::SelectGroup("grp".into());
        let _ = ChannelListAction::NewConversation;
        let _ = ChannelListAction::OpenSavedMessages;
        let _ = ChannelListAction::ToggleServerDropdown;
    }
}
