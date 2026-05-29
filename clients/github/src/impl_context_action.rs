use async_trait::async_trait;
use poly_client::{MenuTargetKind, ClientResult, MenuItem, IsBackend, MenuSlot, MenuItemVariant, ActionOutcome, ClientError};

use crate::GitHubClient;

// ── C.1 — ContextActionBackend ───────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ContextActionBackend for GitHubClient {
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        if target != MenuTargetKind::Server {
            return Ok(Vec::new());
        }

        let star_label_key = if self.is_authenticated() {
            match self.resolve_owner_repo_from_server_id(target_id).await {
                Some((owner, repo)) => {
                    let starred = self.cli.is_starred(&owner, &repo).await.unwrap_or(false);
                    if starred {
                        "plugin-github-menu-unstar-repo-label"
                    } else {
                        "plugin-github-menu-star-repo-label"
                    }
                }
                None => "plugin-github-menu-star-repo-label",
            }
        } else {
            "plugin-github-menu-star-repo-label"
        };

        Ok(vec![
            MenuItem {
                id: "open-in-github".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-github-menu-open-in-github-label".to_string(),
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
                label_key: "plugin-github-menu-watch-repo-label".to_string(),
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
            "open-in-github" | "star-repo" | "watch-repo" => Ok(ActionOutcome::Noop),
            _ => Err(ClientError::NotFound(format!("unknown action: {action_id}"))),
        }
    }
}
