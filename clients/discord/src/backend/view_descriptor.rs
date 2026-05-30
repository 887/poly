//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::DiscordClient;
use async_trait::async_trait;
use poly_client::{ClientResult, SidebarDeclaration, SidebarLayoutKind, ActionOutcome, ClientError, IsBackend, ViewDescriptor, ViewKind, ViewHeader, ViewBody, CardSpec, Cursor, ViewRowsPage, ViewRow, MenuTargetKind, ViewDetail};


#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for DiscordClient {
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

    /// Account-level overview: a card grid of the user's Discord guilds.
    ///
    /// Each card shows the guild name, description (if any), and a
    /// `"N members · X unread · @Y mentions"` meta line.  The actual row
    /// data is fetched by `get_view_rows` when `channel_id == ""`.
    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-discord-overview-title".to_string()),
                subtitle_key: Some("plugin-discord-overview-subtitle".to_string()),
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

    /// Paged row data for views.
    ///
    /// When `channel_id == ""` (the account-overview sentinel emitted by the
    /// host's `AccountOverviewView` route), returns one [`ViewRow`] per joined
    /// Discord guild, mapping guild name / description / unread badges into the
    /// card-grid layout declared by [`get_account_overview_view`].
    ///
    /// Member counts are fetched in parallel via `GET /guilds/{id}?with_counts=true`.
    /// Individual failures degrade gracefully to `"? members"` so one
    /// rate-limited guild doesn't blank the entire overview.
    ///
    /// Non-overview `channel_id`s return `NotSupported` (channel views are not
    /// yet implemented for Discord).
    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        if !channel_id.is_empty() {
            return Err(ClientError::NotSupported("view-rows not yet implemented".into()));
        }

        let servers = self.get_servers().await?;

        // Fan out member-count fetches in parallel; degrade gracefully on
        // individual failures so one unavailable guild doesn't blank the card.
        let member_counts: Vec<Option<u32>> = {
            use futures::future;
            let futs: Vec<_> = servers
                .iter()
                .map(|s| self.http.get_guild_with_counts(&s.id))
                .collect();
            future::join_all(futs)
                .await
                .into_iter()
                .map(|r| r.ok().and_then(|g| g.approximate_member_count))
                .collect()
        };

        let rows = servers
            .into_iter()
            .zip(member_counts)
            .map(|(s, member_count_opt)| {
                let meta = {
                    let members_str = member_count_opt.map_or_else(|| "? members".to_string(), |n| format!("{n} members"));
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
