//! `impl ContextActionBackend for ForgejoClient` — server context menu items
//! (open, star, watch) and action invocation.

use async_trait::async_trait;
use poly_client::{MenuTargetKind, ClientResult, MenuItem, IsBackend, MenuSlot, MenuItemVariant, ActionOutcome, ClientError};
use crate::{ForgejoClient, mapping};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ContextActionBackend for ForgejoClient {
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        if target != MenuTargetKind::Server {
            return Ok(Vec::new());
        }

        let star_label_key = if self.is_authenticated() {
            let maybe_owner_repo = {
                let cache = self.repos.lock().await;
                cache.iter().find_map(|r| {
                    if mapping::server_id_for_repo(r) == target_id {
                        let (o, n) = mapping::split_full_name(&r.full_name);
                        Some((o, n))
                    } else {
                        None
                    }
                })
            };
            if let Some((owner, repo)) = maybe_owner_repo {
                let starred = self.api.is_starred(&owner, &repo).await.unwrap_or(false);
                if starred {
                    "plugin-forgejo-menu-unstar-repo-label"
                } else {
                    "plugin-forgejo-menu-star-repo-label"
                }
            } else {
                "plugin-forgejo-menu-star-repo-label"
            }
        } else {
            "plugin-forgejo-menu-star-repo-label"
        };

        Ok(vec![
            MenuItem {
                id: "open-in-forgejo".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-forgejo-menu-open-in-forgejo-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
            MenuItem {
                id: "star-repo".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: star_label_key.to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
            MenuItem {
                id: "watch-repo".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-forgejo-menu-watch-repo-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
        ])
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "open-in-forgejo" | "star-repo" | "watch-repo" => Ok(ActionOutcome::Noop),
            _ => Err(ClientError::NotFound(format!("unknown action: {action_id}"))),
        }
    }
}
