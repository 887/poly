//! `impl ContextActionBackend for TeamsClient` — context menus, composer, message actions.
//! C.1: context-menu items, toggle actions, and composer buttons.

use crate::TeamsClient;
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use poly_client::{MenuTargetKind, ClientResult, MenuItem, MenuSlot, MenuItemVariant, ActionOutcome, ClientError, PendingHandle, ComposerButton, ComposerSlot};

// ── C.1 — ContextActionBackend ───────────────────────────────────────────────

#[cfg(feature = "native")]
// lint-allow-unused: get_context_menu_items is a dispatch table across 4 MenuTargetKind variants; splitting scatters cohesive menu logic
#[allow(clippy::too_many_lines)]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ContextActionBackend for TeamsClient {
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        // Inline helper: build a MenuItem with common defaults.
        let item = |id: &str, label_key: &str, slot: MenuSlot, variant: MenuItemVariant| MenuItem {
            id: id.to_string(),
            parent_id: None,
            slot,
            label_key: label_key.to_string(),
            icon: None,
            item_variant: variant,
            shortcut: None,
            block: None,
        };

        match target {
            MenuTargetKind::Channel => {
                let hidden = self
                    .hidden_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                let pinned = self
                    .pinned_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                let muted = self
                    .muted_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                Ok(vec![
                    item("mark-read", "plugin-teams-menu-mark-read-label", MenuSlot::Top, MenuItemVariant::Normal),
                    item("mark-unread", "plugin-teams-menu-mark-unread-label", MenuSlot::Top, MenuItemVariant::Normal),
                    if pinned {
                        item("unpin-channel", "plugin-teams-menu-unpin-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("pin-channel", "plugin-teams-menu-pin-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    if hidden {
                        item("show-channel", "plugin-teams-menu-show-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("hide-channel", "plugin-teams-menu-hide-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    if muted {
                        item("unmute-channel", "plugin-teams-menu-unmute-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("mute-channel", "plugin-teams-menu-mute-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                ])
            }

            MenuTargetKind::Server => {
                let muted = self
                    .muted_teams
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                Ok(vec![
                    if muted {
                        item("unmute-team", "plugin-teams-menu-unmute-team-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("mute-team", "plugin-teams-menu-mute-team-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    item("get-team-code", "plugin-teams-menu-get-team-code-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("manage-team", "plugin-teams-menu-manage-team-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("team-settings", "plugin-teams-menu-team-settings-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("edit-per-team-profile", "plugin-teams-menu-edit-per-team-profile-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("leave-team", "plugin-teams-menu-leave-team-label", MenuSlot::BeforeLeave, MenuItemVariant::Destructive),
                ])
            }

            MenuTargetKind::User => Ok(vec![
                item("open-chat", "plugin-teams-menu-open-chat-label", MenuSlot::Top, MenuItemVariant::Normal),
                item("view-profile", "plugin-teams-menu-view-profile-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                item("schedule-meeting", "plugin-teams-menu-schedule-meeting-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
            ]),

            MenuTargetKind::Message => {
                let saved = self
                    .saved_messages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                Ok(vec![
                    item("react", "plugin-teams-menu-react-label", MenuSlot::Top, MenuItemVariant::Normal),
                    item("reply-in-thread", "plugin-teams-menu-reply-in-thread-label", MenuSlot::Top, MenuItemVariant::Normal),
                    if saved {
                        item("unsave-message", "plugin-teams-menu-unsave-message-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("save-message", "plugin-teams-menu-save-message-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    item("mark-important", "plugin-teams-menu-mark-important-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("delete-message", "plugin-teams-menu-delete-message-label", MenuSlot::BeforeLeave, MenuItemVariant::Destructive),
                ])
            }

            MenuTargetKind::Dm => {
                let muted = self
                    .muted_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                let hidden = self
                    .hidden_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                Ok(vec![
                    if muted {
                        item("unmute-dm", "plugin-teams-menu-unmute-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("mute-dm", "plugin-teams-menu-mute-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    if hidden {
                        item("show-dm", "plugin-teams-menu-show-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("hide-dm", "plugin-teams-menu-hide-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                ])
            }

            MenuTargetKind::Category => Ok(Vec::new()),
        }
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            // ── Channel toggles ──────────────────────────────────────────────
            "pin-channel" => {
                self.pinned_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unpin-channel" => {
                self.pinned_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "hide-channel" => {
                self.hidden_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "show-channel" => {
                self.hidden_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "mute-channel" => {
                self.muted_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unmute-channel" => {
                self.muted_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "mark-read"
            | "mark-unread"
            | "leave-team"
            | "get-team-code"
            | "manage-team"
            | "team-settings"
            | "edit-per-team-profile"
            | "open-chat"
            | "view-profile"
            | "schedule-meeting" => Ok(ActionOutcome::Noop),

            // ── Team toggles ─────────────────────────────────────────────────
            "mute-team" => {
                self.muted_teams
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unmute-team" => {
                self.muted_teams
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }

            // ── User actions ─────────────────────────────────────────────────

            // ── Message toggles ──────────────────────────────────────────────
            "save-message" => {
                self.saved_messages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unsave-message" => {
                self.saved_messages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "react" | "reply-in-thread" | "mark-important" | "delete-message" => {
                Ok(ActionOutcome::Noop)
            }

            // ── DM toggles ───────────────────────────────────────────────────
            "mute-dm" => {
                self.muted_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unmute-dm" => {
                self.muted_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "hide-dm" => {
                self.hidden_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "show-dm" => {
                self.hidden_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }

            _ => Err(ClientError::NotFound(format!("unknown action: {action_id}"))),
        }
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        Ok(vec![ComposerButton {
            id: "mention".to_string(),
            label_key: "plugin-teams-composer-mention-label".to_string(),
            icon: "@".to_string(),
            position: ComposerSlot::RightOfInput,
        }])
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        // Teams has no backend-specific per-message actions beyond host universals.
        Ok(Vec::new())
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "mention" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown composer action: {other}"))),
        }
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown message action: {action_id}")))
    }
}
