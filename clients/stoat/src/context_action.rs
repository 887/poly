//! `impl ContextActionBackend for StoatClient` — context-menu/message-action menus + handlers.
//!
//! Split out from `lib.rs` in SOLID-audit-stoat D.2 (C.1 / F10).

use async_trait::async_trait;
use poly_client::{
    ActionOutcome, ClientError, ClientResult, ComposerButton, ComposerSlot, MenuItem,
    MenuItemVariant, MenuSlot, MenuTargetKind, PendingHandle,
};

use super::StoatClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ContextActionBackend for StoatClient {
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        fn normal(id: &str, label_key: &str, slot: MenuSlot) -> MenuItem {
            MenuItem {
                id: id.to_string(),
                parent_id: None,
                slot,
                label_key: label_key.to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            }
        }

        fn destructive(id: &str, label_key: &str, slot: MenuSlot) -> MenuItem {
            MenuItem {
                id: id.to_string(),
                parent_id: None,
                slot,
                label_key: label_key.to_string(),
                icon: None,
                item_variant: MenuItemVariant::Destructive,
                shortcut: None,
                block: None,
            }
        }

        let (is_channel_muted, is_server_muted, is_user_blocked, is_friend, is_dm_muted) =
            self.menu_state
                .lock()
                .map(|state| {
                    (
                        state.muted_channels.contains(target_id),
                        state.muted_servers.contains(target_id),
                        state.blocked_users.contains(target_id),
                        state.friends.contains(target_id),
                        state.muted_dms.contains(target_id),
                    )
                })
                .unwrap_or((false, false, false, false, false));

        match target {
            MenuTargetKind::Channel => {
                let mute_item = if is_channel_muted {
                    normal("unmute-channel", "plugin-stoat-menu-unmute-channel-label", MenuSlot::AfterFavorites)
                } else {
                    normal("mute-channel", "plugin-stoat-menu-mute-channel-label", MenuSlot::AfterFavorites)
                };
                Ok(vec![
                    mute_item,
                    normal("mark-channel-read", "plugin-stoat-menu-mark-channel-read-label", MenuSlot::AfterFavorites),
                ])
            }
            MenuTargetKind::Server => {
                let mute_item = if is_server_muted {
                    normal("unmute-server", "plugin-stoat-menu-unmute-server-label", MenuSlot::AfterFavorites)
                } else {
                    normal("mute-server", "plugin-stoat-menu-mute-server-label", MenuSlot::AfterFavorites)
                };
                Ok(vec![
                    normal("invite-people", "plugin-stoat-menu-invite-people-label", MenuSlot::AfterFavorites),
                    normal("privacy-settings", "plugin-stoat-menu-privacy-settings-label", MenuSlot::AfterFavorites),
                    normal("edit-per-server-profile", "plugin-stoat-menu-edit-per-server-profile-label", MenuSlot::AfterFavorites),
                    normal("manage-bots", "plugin-stoat-menu-manage-bots-label", MenuSlot::AfterFavorites),
                    mute_item,
                    destructive("leave-server", "plugin-stoat-menu-leave-server-label", MenuSlot::BeforeLeave),
                ])
            }
            MenuTargetKind::User => {
                let block_item = if is_user_blocked {
                    normal("unblock-user", "plugin-stoat-menu-unblock-user-label", MenuSlot::BeforeLeave)
                } else {
                    destructive("block-user", "plugin-stoat-menu-block-user-label", MenuSlot::BeforeLeave)
                };
                let friend_item = if is_friend {
                    normal("remove-friend", "plugin-stoat-menu-remove-friend-label", MenuSlot::AfterFavorites)
                } else {
                    normal("add-friend", "plugin-stoat-menu-add-friend-label", MenuSlot::AfterFavorites)
                };
                Ok(vec![
                    normal("open-dm", "plugin-stoat-menu-open-dm-label", MenuSlot::AfterFavorites),
                    friend_item,
                    block_item,
                ])
            }
            MenuTargetKind::Message => Ok(vec![
                normal("react-message", "plugin-stoat-menu-react-message-label", MenuSlot::Top),
                normal("copy-message-link", "plugin-stoat-menu-copy-message-link-label", MenuSlot::AfterFavorites),
                destructive("delete-message", "plugin-stoat-menu-delete-message-label", MenuSlot::BeforeLeave),
            ]),
            MenuTargetKind::Dm => {
                let mute_item = if is_dm_muted {
                    normal("unmute-dm", "plugin-stoat-menu-unmute-dm-label", MenuSlot::AfterFavorites)
                } else {
                    normal("mute-dm", "plugin-stoat-menu-mute-dm-label", MenuSlot::AfterFavorites)
                };
                Ok(vec![
                    destructive("close-dm", "plugin-stoat-menu-close-dm-label", MenuSlot::BeforeLeave),
                    mute_item,
                ])
            }
            MenuTargetKind::Category => Ok(vec![]),
        }
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "mute-channel" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_channels.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "unmute-channel" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_channels.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            "mark-channel-read" | "leave-server" | "delete-message" => {
                Ok(ActionOutcome::Completed)
            }
            "mute-server" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_servers.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "unmute-server" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_servers.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            "invite-people" | "privacy-settings" | "edit-per-server-profile" | "manage-bots" => {
                Ok(ActionOutcome::Noop)
            }
            "block-user" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.blocked_users.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "unblock-user" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.blocked_users.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            "add-friend" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.friends.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "remove-friend" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.friends.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            "open-dm" | "react-message" | "copy-message-link" => Ok(ActionOutcome::Noop),
            "close-dm" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.closed_dms.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "mute-dm" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_dms.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "unmute-dm" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_dms.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            other => Err(ClientError::NotFound(format!("unknown stoat action: {other}"))),
        }
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }
    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        Ok(vec![ComposerButton {
            id: "emoji-picker".to_string(),
            label_key: "plugin-stoat-composer-emoji-label".to_string(),
            icon: "😀".to_string(),
            position: ComposerSlot::RightOfInput,
        }])
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(vec![MenuItem {
            id: "report".to_string(),
            parent_id: None,
            slot: MenuSlot::AfterFavorites,
            label_key: "plugin-stoat-message-action-report-label".to_string(),
            icon: None,
            item_variant: MenuItemVariant::Normal,
            shortcut: None,
            block: None,
        }])
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "emoji-picker" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown composer action: {other}"))),
        }
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "report" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown message action: {other}"))),
        }
    }
}
