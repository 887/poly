//! `impl ContextActionBackend for MatrixClient` — context menus, composer/message actions.

use async_trait::async_trait;
use poly_client::*;

use crate::MatrixClient;

// ── C.1 — ContextActionBackend ───────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ContextActionBackend for MatrixClient {
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        match target {
            // ── Server / Space ─────────────────────────────────────────────
            MenuTargetKind::Server => Ok(vec![
                Self::simple_item("space-settings", MenuSlot::AfterFavorites, "plugin-matrix-menu-space-settings-label", MenuItemVariant::Normal),
                Self::simple_item("edit-per-space-profile", MenuSlot::AfterFavorites, "plugin-matrix-menu-edit-per-space-profile-label", MenuItemVariant::Normal),
                Self::simple_item("e2ee-verification", MenuSlot::AfterFavorites, "plugin-matrix-menu-e2ee-verification-label", MenuItemVariant::Normal),
                // F10 additions
                Self::simple_item("browse-rooms-in-space", MenuSlot::AfterFavorites, "plugin-matrix-menu-browse-rooms-in-space-label", MenuItemVariant::Normal),
                Self::simple_item("add-room-to-space", MenuSlot::AfterFavorites, "plugin-matrix-menu-add-room-to-space-label", MenuItemVariant::Normal),
                Self::simple_item("leave-space", MenuSlot::BeforeLeave, "plugin-matrix-menu-leave-space-label", MenuItemVariant::Destructive),
            ]),

            // ── Channel / Room ─────────────────────────────────────────────
            MenuTargetKind::Channel => {
                // Distinct id per state: mark-read-room / mark-unread-room
                // Poisoned lock treated as "not read" — safe default.
                let is_read = self.marked_read.read()
                    .map(|g| g.contains(target_id))
                    .unwrap_or(false);
                let read_item = if is_read {
                    Self::simple_item("mark-unread-room", MenuSlot::Top, "plugin-matrix-menu-mark-unread-room-label", MenuItemVariant::Normal)
                } else {
                    Self::simple_item("mark-read-room", MenuSlot::Top, "plugin-matrix-menu-mark-read-room-label", MenuItemVariant::Normal)
                };

                // Distinct id per state: mute-room / unmute-room
                let is_muted = self.muted_rooms.read()
                    .map(|g| g.contains(target_id))
                    .unwrap_or(false);
                let mute_item = if is_muted {
                    Self::simple_item("unmute-room", MenuSlot::AfterFavorites, "plugin-matrix-menu-unmute-room-label", MenuItemVariant::Normal)
                } else {
                    Self::simple_item("mute-room", MenuSlot::AfterFavorites, "plugin-matrix-menu-mute-room-label", MenuItemVariant::Normal)
                };

                Ok(vec![
                    read_item,
                    mute_item,
                    Self::simple_item("leave-room", MenuSlot::BeforeLeave, "plugin-matrix-menu-leave-room-label", MenuItemVariant::Destructive),
                ])
            }

            // ── DM Channel ─────────────────────────────────────────────────
            MenuTargetKind::Dm => {
                let is_read = self.marked_read.read()
                    .map(|g| g.contains(target_id))
                    .unwrap_or(false);
                let read_item = if is_read {
                    Self::simple_item("mark-unread-room", MenuSlot::Top, "plugin-matrix-menu-mark-unread-room-label", MenuItemVariant::Normal)
                } else {
                    Self::simple_item("mark-read-room", MenuSlot::Top, "plugin-matrix-menu-mark-read-room-label", MenuItemVariant::Normal)
                };

                Ok(vec![
                    read_item,
                    Self::simple_item("leave-dm", MenuSlot::BeforeLeave, "plugin-matrix-menu-leave-dm-label", MenuItemVariant::Destructive),
                ])
            }

            // ── User ───────────────────────────────────────────────────────
            MenuTargetKind::User => {
                // Distinct id per state: ignore-user / unignore-user
                let is_ignored = self.ignored_users.read()
                    .map(|g| g.contains(target_id))
                    .unwrap_or(false);
                let ignore_item = if is_ignored {
                    Self::simple_item("unignore-user", MenuSlot::AfterFavorites, "plugin-matrix-menu-unignore-user-label", MenuItemVariant::Normal)
                } else {
                    Self::simple_item("ignore-user", MenuSlot::AfterFavorites, "plugin-matrix-menu-ignore-user-label", MenuItemVariant::Normal)
                };

                Ok(vec![
                    Self::simple_item("open-dm", MenuSlot::Top, "plugin-matrix-menu-open-dm-label", MenuItemVariant::Normal),
                    Self::simple_item("view-profile", MenuSlot::Top, "plugin-matrix-menu-view-profile-label", MenuItemVariant::Normal),
                    // Cross-signing stub
                    Self::simple_item("verify-user", MenuSlot::AfterFavorites, "plugin-matrix-menu-verify-user-label", MenuItemVariant::Normal),
                    ignore_item,
                ])
            }

            // ── Message ────────────────────────────────────────────────────
            MenuTargetKind::Message => Ok(vec![
                Self::simple_item("react-message", MenuSlot::Top, "plugin-matrix-menu-react-message-label", MenuItemVariant::Normal),
                Self::simple_item("reply-in-thread", MenuSlot::Top, "plugin-matrix-menu-reply-in-thread-label", MenuItemVariant::Normal),
                Self::simple_item("copy-permalink", MenuSlot::AfterFavorites, "plugin-matrix-menu-copy-permalink-label", MenuItemVariant::Normal),
                // Destructive — author or admin only
                Self::simple_item("redact-message", MenuSlot::BeforeLeave, "plugin-matrix-menu-redact-message-label", MenuItemVariant::Destructive),
            ]),

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
            // ── Noop actions (Server/Space, leave-room/dm, message ops, profile views)
            // All return Ok(Noop); merged into one arm to satisfy
            // clippy::match_same_arms.
            "space-settings"
            | "edit-per-space-profile"
            | "e2ee-verification"
            | "browse-rooms-in-space"
            | "add-room-to-space"
            | "leave-space"
            | "leave-room"
            | "leave-dm"
            | "open-dm"
            | "view-profile"
            | "verify-user"
            | "react-message"
            | "reply-in-thread"
            | "copy-permalink"
            | "redact-message" => Ok(ActionOutcome::Noop),

            // ── Channel / Room — state mutations ────────────────────────────
            // Poisoned lock treated as a no-op write — silent, non-panicking.
            "mark-read-room" => {
                if let Ok(mut g) = self.marked_read.write() {
                    g.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Noop)
            }
            "mark-unread-room" => {
                if let Ok(mut g) = self.marked_read.write() {
                    g.remove(target_id);
                }
                Ok(ActionOutcome::Noop)
            }
            "mute-room" => {
                if let Ok(mut g) = self.muted_rooms.write() {
                    g.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Noop)
            }
            "unmute-room" => {
                if let Ok(mut g) = self.muted_rooms.write() {
                    g.remove(target_id);
                }
                Ok(ActionOutcome::Noop)
            }
            // ── User — state mutations ───────────────────────────────────────
            "ignore-user" => {
                if let Ok(mut g) = self.ignored_users.write() {
                    g.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Noop)
            }
            "unignore-user" => {
                if let Ok(mut g) = self.ignored_users.write() {
                    g.remove(target_id);
                }
                Ok(ActionOutcome::Noop)
            }

            _ => Err(ClientError::NotFound(format!("unknown action: {action_id}"))),
        }
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }
    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        Ok(vec![ComposerButton {
            id: "me-action".to_string(),
            label_key: "plugin-matrix-composer-me-label".to_string(),
            icon: "🎭".to_string(),
            position: ComposerSlot::LeftOfInput,
        }])
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(vec![MenuItem {
            id: "verify-sender".to_string(),
            parent_id: None,
            slot: MenuSlot::AfterFavorites,
            label_key: "plugin-matrix-message-action-verify-sender-label".to_string(),
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
            "me-action" => Ok(ActionOutcome::Noop),
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
            "verify-sender" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown message action: {other}"))),
        }
    }
}
