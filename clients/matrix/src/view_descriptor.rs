//! `impl ViewDescriptorBackend for MatrixClient` — sidebar layout, overview, view rows.

use async_trait::async_trait;
use poly_client::{ClientResult, SidebarDeclaration, SidebarLayoutKind, SidebarSection, ActionOutcome, ClientError, ViewDescriptor, ViewKind, ViewHeader, ViewBody, CardSpec, Cursor, ViewRowsPage, ViewRow, MenuTargetKind, ViewDetail};

use crate::build_sidebar_items;
use crate::MatrixClient;

// ── C.1 — ViewDescriptorBackend ──────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for MatrixClient {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        // F4: Switch to Custom layout so the host renders our full space tree.
        let entries = self.fetch_space_tree().await.unwrap_or_default();
        let items = build_sidebar_items(entries);
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Custom,
            sections: vec![SidebarSection {
                header_key: Some("plugin-matrix-sidebar-spaces-section".to_string()),
                collapsible: false,
                default_collapsed: false,
                items,
            }],
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown sidebar action: {action_id}")))
    }

    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-matrix-overview-title".to_string()),
                subtitle_key: Some("plugin-matrix-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Err(ClientError::NotSupported("channel-view not yet implemented".into()))
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        if !channel_id.is_empty() && channel_id != "account-overview" {
            return Err(ClientError::NotSupported("view-rows not yet implemented".into()));
        }

        let joined = self.http.fetch_joined_rooms().await?;
        let homeserver_url = self.homeserver_url().to_string();

        let mut rows = Vec::new();
        for room_id in &joined.joined_rooms {
            let state = self.http.fetch_room_state(room_id).await.unwrap_or_default();

            let name = Self::extract_canonical_alias(&state)
                .unwrap_or_else(|| Self::extract_room_name(&state, room_id));
            let topic = Self::extract_room_topic(&state);
            let member_count = Self::count_joined_members(&state);
            let icon = Self::extract_avatar_url(&state, &homeserver_url);

            // Unread / mention counts are not tracked in-memory by this backend
            // (no persistent sync state). They default to 0.
            let unread: u32 = 0;
            let mentions: u32 = 0;

            let meta_text = format!(
                "{member_count} members · {unread} unread · @{mentions} mentions"
            );

            let is_space = Self::is_space_room(&state);
            rows.push(ViewRow {
                id: room_id.clone(),
                primary_text: name,
                secondary_text: topic,
                meta_text: Some(meta_text),
                icon,
                badge: None,
                preview_image_url: None,
                is_video: false,
                context_menu_target_kind: if is_space {
                    MenuTargetKind::Server
                } else {
                    MenuTargetKind::Channel
                },
            });
        }

        Ok(ViewRowsPage { rows, next_cursor: None })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        _row_id: &str,
    ) -> ClientResult<ViewDetail> {
        Err(ClientError::NotSupported("view-detail not yet implemented".into()))
    }
}
