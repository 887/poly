//! `impl ViewDescriptorBackend for StoatClient` — sidebar/view declarations + account overview.
//!
//! Split out from `lib.rs` in SOLID-audit-stoat D.2 (C.1).

use async_trait::async_trait;
use futures::future;
use poly_client::{
    CardSpec, ClientError, ClientResult, Cursor, IsBackend as _, MenuTargetKind,
    SidebarDeclaration, SidebarLayoutKind, ViewBody, ViewDescriptor, ViewDetail, ViewHeader,
    ViewKind, ViewRow, ViewRowsPage, ActionOutcome,
};

use super::StoatClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for StoatClient {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::ChannelList,
            sections: Vec::new(),
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
                title_key: Some("plugin-stoat-overview-title".to_string()),
                subtitle_key: Some("plugin-stoat-overview-subtitle".to_string()),
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
        // Empty channel_id is the account-overview sentinel emitted by
        // `AccountOverviewView` (routes.rs line ~149). Map each joined
        // server to a card row with member count + unread indicators.
        if !channel_id.is_empty() {
            return Err(ClientError::NotSupported("view-rows not yet implemented".into()));
        }

        let servers = self.get_servers().await?;

        // Fan out member-count fetches in parallel; degrade gracefully
        // on individual failures so one unauthorized server doesn't
        // blank the entire overview.
        let member_counts: Vec<Option<usize>> = {
            let futs: Vec<_> = servers
                .iter()
                .map(|s| self.http.fetch_server_members(&s.id))
                .collect();
            future::join_all(futs)
                .await
                .into_iter()
                .map(|r| r.ok().map(|resp| resp.members.len()))
                .collect()
        };

        let rows = servers
            .into_iter()
            .zip(member_counts)
            .map(|(s, member_count_opt)| {
                let meta = {
                    let members_str = member_count_opt
                        .map_or_else(|| "? members".to_string(), |n| format!("{n} members"));
                    let unread_part = if s.unread_count > 0 {
                        format!(" · {} unread", s.unread_count)
                    } else {
                        String::new()
                    };
                    let mention_part = if s.mention_count > 0 {
                        format!(" · @{}", s.mention_count)
                    } else {
                        String::new()
                    };
                    format!("{members_str}{unread_part}{mention_part}")
                };
                ViewRow {
                    id: s.id.clone(),
                    primary_text: s.name.clone(),
                    secondary_text: s.description.clone(),
                    meta_text: Some(meta),
                    icon: s.icon_url,
                    badge: None,
                    context_menu_target_kind: MenuTargetKind::Server,
                    preview_image_url: None,
                    is_video: false,
                }
            })
            .collect();

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
