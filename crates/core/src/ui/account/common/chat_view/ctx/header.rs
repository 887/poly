//! `HeaderCtx` — the render context for the chat header region.
//!
//! Covers `layout::render_chat_header` and everything it delegates to: the
//! channel / server / DM identity block on the left and the action row (plus
//! its mobile overflow menu) on the right.
//!
//! Sixteen fields against `ChatViewMarkupCtx`'s 74, and every consumer takes it
//! by `&` — see `docs/plans/plan-chat-view-ctx-split.md`. Deliberately **not**
//! `Clone`: nothing in the header region needs an owned copy, and a derived
//! `Clone` is how the god-struct grew 18 deep-clone sites in the first place.

use dioxus::prelude::*;

use poly_client::{Channel, PresenceStatus, Server, User};

use crate::state::{BatchedSignal, ChatLists, UiLayout};

use super::super::ChatUtilityPanel;
use super::super::signals::ChatViewSignals;

/// Channel-id prefix marking a one-to-one direct message.
const DM_CHANNEL_PREFIX: &str = "dm-";
/// Channel-id prefix marking a multi-party group DM.
const GROUP_CHANNEL_PREFIX: &str = "group-";

/// Everything the chat header renders from, and nothing else.
///
/// The identity half (`channel_id` … `dm_user_presence`) is a snapshot taken
/// once per render; the signal half is `Copy` handles the header's event
/// handlers write through.
pub(crate) struct HeaderCtx {
    pub(crate) channel_id: Option<String>,
    pub(crate) current_channel: Option<Channel>,
    pub(crate) current_server: Option<Server>,
    pub(crate) is_dm_channel: bool,
    pub(crate) is_group_channel: bool,
    /// Whether the right wing (member list / DM contact panel) is open.
    pub(crate) member_list_visible: bool,
    /// Group-chat member count. The header renders only the count, so the
    /// member `Vec` itself is deliberately not carried here.
    pub(crate) group_member_count: usize,
    pub(crate) dm_user: Option<User>,
    pub(crate) dm_user_avatar: Option<String>,
    pub(crate) dm_user_presence: PresenceStatus,
    pub(crate) utility_panel: Signal<Option<ChatUtilityPanel>>,
    pub(crate) notifications_muted: Signal<bool>,
    pub(crate) show_search_filters: Signal<bool>,
    pub(crate) header_actions_overflow: Signal<bool>,
    pub(crate) header_actions_menu_open: Signal<bool>,
    /// Resize-driven rerender tick — forwarded to `ChatHeaderActions` for
    /// overflow detection.
    pub(crate) mobile_layout_resize_tick: Signal<u64>,
}

/// Snapshot the header region out of the signal bundle, once per render.
///
/// The `.read()` calls here are the header's *intended* reactive
/// subscriptions — the header must re-render when the selected channel, the
/// DM peer's presence, or the right-wing visibility changes. They were moved
/// verbatim out of `build_chat_view_markup_ctx`, where they carried the same
/// justification; the three separate `dm_channels` lookups it performed
/// (peer, avatar, presence) collapse into the single lookup below.
pub(crate) fn build_header_ctx(signals: &ChatViewSignals) -> HeaderCtx {
    let channel_id = signals.nav.read().selected_channel.cloned(); // poly-lint: allow render-time-read — reactive: the header must re-render on channel switch
    let is_dm_channel = channel_is_dm(channel_id.as_deref());
    let is_group_channel = channel_is_group(channel_id.as_deref());
    let dm_user = current_dm_user(signals.chat_lists, channel_id.as_deref(), is_dm_channel);

    HeaderCtx {
        current_channel: signals.chat_view_state.read().current_channel.clone(), // poly-lint: allow render-time-read — reactive: header title tracks the active channel
        current_server: signals.chat_view_state.read().current_server.clone(), // poly-lint: allow render-time-read — reactive: header badge tracks the active server
        group_member_count: signals.chat_view_state.read().active_group_members.len(), // poly-lint: allow render-time-read — reactive: group subtitle shows the live member count
        member_list_visible: right_wing_open(signals.ui_layout, is_dm_channel || is_group_channel),
        dm_user_avatar: dm_user.as_ref().and_then(|user| user.avatar_url.clone()),
        dm_user_presence: dm_user
            .as_ref()
            .map_or(PresenceStatus::Offline, |user| user.presence),
        dm_user,
        channel_id,
        is_dm_channel,
        is_group_channel,
        utility_panel: signals.utility_panel,
        notifications_muted: signals.notifications_muted,
        show_search_filters: signals.show_search_filters,
        header_actions_overflow: signals.header_actions_overflow,
        header_actions_menu_open: signals.header_actions_menu_open,
        mobile_layout_resize_tick: signals.mobile_layout_resize_tick,
    }
}

/// Whether `channel_id` names a one-to-one direct message.
///
/// A missing channel id is neither a DM nor a group — it is the "no channel
/// selected" state, which the header renders as an empty title.
fn channel_is_dm(channel_id: Option<&str>) -> bool {
    channel_id.unwrap_or_default().starts_with(DM_CHANNEL_PREFIX)
}

/// Whether `channel_id` names a multi-party group DM.
fn channel_is_group(channel_id: Option<&str>) -> bool {
    channel_id
        .unwrap_or_default()
        .starts_with(GROUP_CHANNEL_PREFIX)
}

/// Whether the right wing is open. DM and group channels track this on a
/// different `UiLayout` flag than server channels do.
fn right_wing_open(ui_layout: BatchedSignal<UiLayout>, is_dm_or_group: bool) -> bool {
    if is_dm_or_group {
        return ui_layout.read().dm_right_sidebar_visible; // poly-lint: allow render-time-read — reactive: the header toggle reflects right-wing state
    }
    ui_layout.read().right_sidebar_visible // poly-lint: allow render-time-read — reactive: the header toggle reflects right-wing state
}

/// The DM peer for `channel_id`, or `None` when this is not a DM channel.
///
/// One `dm_channels` lookup serves the peer, the avatar and the presence dot;
/// `build_chat_view_markup_ctx` used to run three.
fn current_dm_user(
    chat_lists: BatchedSignal<ChatLists>,
    channel_id: Option<&str>,
    is_dm_channel: bool,
) -> Option<User> {
    if !is_dm_channel {
        return None;
    }

    let cid = channel_id.unwrap_or_default();
    chat_lists
        .read() // poly-lint: allow render-time-read — reactive: the DM header follows the peer's presence/avatar
        .dm_channels
        .iter()
        .find(|dm| dm.id == cid)
        .map(|dm| dm.user.clone())
}

#[cfg(test)]
mod tests {
    use super::{channel_is_dm, channel_is_group};

    #[test]
    fn dm_prefix_classifies_only_dm_channels() {
        assert!(channel_is_dm(Some("dm-alice")));
        assert!(!channel_is_dm(Some("group-standup")));
        assert!(!channel_is_dm(Some("general")));
    }

    #[test]
    fn group_prefix_classifies_only_group_channels() {
        assert!(channel_is_group(Some("group-standup")));
        assert!(!channel_is_group(Some("dm-alice")));
        assert!(!channel_is_group(Some("general")));
    }

    /// "No channel selected" must not be misread as a DM: the header renders
    /// the empty-state title for it, not the DM avatar block.
    #[test]
    fn missing_channel_id_is_neither_dm_nor_group() {
        assert!(!channel_is_dm(None));
        assert!(!channel_is_group(None));
    }

    /// A channel merely *containing* the prefix must not match — the check is
    /// anchored at the start, and `open-dm-notes` is a normal text channel.
    #[test]
    fn prefixes_are_anchored_at_the_start() {
        assert!(!channel_is_dm(Some("open-dm-notes")));
        assert!(!channel_is_group(Some("sub-group-chat")));
    }
}
