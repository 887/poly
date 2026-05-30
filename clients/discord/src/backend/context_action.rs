//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::DiscordClient;
use async_trait::async_trait;
use poly_client::{MenuTargetKind, MenuItem, ClientError, MenuSlot, MenuItemVariant, ActionOutcome, PendingHandle, ClientResult, ComposerButton};


#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ContextActionBackend for DiscordClient {
    async fn get_context_menu_items(
        &self, target: MenuTargetKind, target_id: &str,
    ) -> Result<Vec<MenuItem>, ClientError> {
        match target {
            MenuTargetKind::Server => {
                // State-aware: Mute Server / Unmute Server, plus static items.
                let muted = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_servers.contains(target_id);
                let mute_item = if muted {
                    MenuItem {
                        id: "unmute-server".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-unmute-server-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "mute-server".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mute-server-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    MenuItem {
                        id: "invite-people".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-invite-people-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    MenuItem {
                        id: "privacy-settings".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-privacy-settings-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    MenuItem {
                        id: "edit-per-server-profile".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-edit-per-server-profile-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    MenuItem {
                        id: "server-boost".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-server-boost-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    mute_item,
                    MenuItem {
                        id: "leave-server".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-leave-server-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::Channel => {
                // State-aware: Mute/Unmute Channel, Mark Read.
                let muted = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_channels.contains(target_id);
                let mute_item = if muted {
                    MenuItem {
                        id: "unmute-channel".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-unmute-channel-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "mute-channel".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mute-channel-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    mute_item,
                    MenuItem {
                        id: "mark-channel-read".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mark-channel-read-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::User => {
                // State-aware: Block/Unblock, Add Friend/Remove Friend, Open DM.
                let blocked = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).blocked_users.contains(target_id);
                let is_friend = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).friend_ids.contains(target_id);
                let block_item = if blocked {
                    MenuItem {
                        id: "unblock-user".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-unblock-user-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "block-user".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-block-user-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    }
                };
                let friend_item = if is_friend {
                    MenuItem {
                        id: "remove-friend".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-remove-friend-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "add-friend".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-add-friend-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    MenuItem {
                        id: "open-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-open-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    friend_item,
                    block_item,
                ])
            }
            MenuTargetKind::Message => {
                // Copy Link is always available; Delete is destructive.
                Ok(vec![
                    MenuItem {
                        id: "copy-message-link".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-copy-message-link-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    MenuItem {
                        id: "delete-message".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-delete-message-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::Dm => {
                // State-aware: Mute/Unmute DM, Close DM.
                let muted = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_dms.contains(target_id);
                let mute_item = if muted {
                    MenuItem {
                        id: "unmute-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-unmute-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "mute-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mute-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    mute_item,
                    MenuItem {
                        id: "close-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-close-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::Category => Ok(Vec::new()),
        }
    }

    async fn invoke_context_action(
        &self, action_id: &str, _target: MenuTargetKind, target_id: &str,
    ) -> Result<ActionOutcome, ClientError> {
        match action_id {
            // Server / channel / user / message actions that are pure no-ops at this layer.
            "invite-people"
            | "privacy-settings"
            | "edit-per-server-profile"
            | "server-boost"
            | "leave-server"
            | "mark-channel-read"
            | "open-dm"
            | "copy-message-link"
            | "delete-message"
            | "close-dm" => Ok(ActionOutcome::Noop),
            "mute-server" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_servers.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unmute-server" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_servers.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            // Channel actions
            "mute-channel" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_channels.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unmute-channel" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_channels.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            // User actions
            "add-friend" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).friend_ids.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "remove-friend" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).friend_ids.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            "block-user" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).blocked_users.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unblock-user" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).blocked_users.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            // DM actions
            "mute-dm" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_dms.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unmute-dm" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_dms.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            other => Err(ClientError::NotFound(format!("unknown action: {other}"))),
        }
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }
    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        // Stickers/GIF picker lives in the unified MediaPickerPopup
        // (composer-common emoji button → tabs for emoji/GIF/stickers).
        // Don't duplicate it as a separate composer button.
        Ok(vec![])
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(vec![MenuItem {
            id: "pin-message".to_string(),
            parent_id: None,
            slot: MenuSlot::AfterFavorites,
            label_key: "plugin-discord-message-action-pin-message-label".to_string(),
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
        Err(ClientError::NotFound(format!("unknown composer action: {action_id}")))
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "pin-message" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown message action: {other}"))),
        }
    }
}
