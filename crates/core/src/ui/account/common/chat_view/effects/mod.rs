//! Per-effect hooks extracted from `chat_view/mod.rs`.
//!
//! Each file owns exactly one `use_*_effect` function — the "reason to
//! change" (SRP) — along with any private helpers that are exclusive to that
//! effect.  The orchestrator (`use_chat_view_effects` in `chat_view/mod.rs`)
//! simply calls them all in order via `effects::use_*_effect(...)`.

mod mobile_layout_resize_rerender;
mod mobile_side_column;
mod composer_focus;
mod member_list;
mod search_messages;
mod pinned_messages;
mod history_state;
mod member_list_preferences;
mod command_preload;
mod unread_marker_visibility;
mod auto_dismiss_divider;

pub(super) use mobile_layout_resize_rerender::use_mobile_layout_resize_rerender_effect;
pub(super) use mobile_side_column::use_mobile_side_column_effect;
pub(super) use composer_focus::use_composer_focus_effect;
pub(super) use member_list::use_member_list_effect;
pub(super) use search_messages::use_search_messages_effect;
pub(super) use pinned_messages::use_pinned_messages_effect;
pub(super) use history_state::use_history_state_effect;
pub(super) use member_list_preferences::use_member_list_preferences_effect;
pub(super) use command_preload::use_command_preload_effect;
pub(super) use unread_marker_visibility::use_unread_marker_visibility_effect;
pub(super) use auto_dismiss_divider::use_auto_dismiss_divider_effect;
