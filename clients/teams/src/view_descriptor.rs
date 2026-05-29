//! `impl ViewDescriptorBackend for TeamsClient` — sidebar/channel views and rows.
//! C.1: channel views via Microsoft Graph.

use crate::TeamsClient;
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use poly_client::{ClientResult, SidebarDeclaration, SidebarLayoutKind, ActionOutcome, ClientError, ViewDescriptor, ViewKind, ViewHeader, ViewBody, CardSpec, ListSpec, RowTemplate, Cursor, ViewRowsPage, ViewRow, MenuTargetKind, IsBackend, ViewDetail, CustomBlock};

// ── C.1 — ViewDescriptorBackend ──────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for TeamsClient {
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
                title_key: Some("plugin-teams-overview-title".to_string()),
                subtitle_key: Some("plugin-teams-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        // C.1: team channels render as a flat message list.
        // Empty channel_id is the account-overview sentinel — not a channel view.
        if channel_id.is_empty() {
            return Err(ClientError::NotSupported("get_channel_view: empty channel_id is not a channel".into()));
        }
        Ok(ViewDescriptor {
            kind: ViewKind::FlatList,
            header: None,
            toolbar: None,
            body: ViewBody::ListBody(ListSpec {
                row_template: RowTemplate {
                    primary_field: "content".to_string(),
                    secondary_field: Some("author".to_string()),
                    meta_field: Some("timestamp".to_string()),
                    icon_field: None,
                },
                page_size: 50,
            }),
        })
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        // Empty channel_id signals the account overview — return one card per team.
        if !channel_id.is_empty() {
            // C.1: Fetch messages for the team channel and map to ViewRows.
            // channel_id is "team_id/channel_id" per plugin contract.
            let msgs = if let Some((team_id, ch_id)) = channel_id.split_once('/') {
                self.http.get_channel_messages(team_id, ch_id, Some(50)).await?
            } else {
                // Plain chat ID (DM) — use chats endpoint.
                self.http.get_chat_messages(channel_id, Some(50)).await?
            };

            let rows = msgs
                .into_iter()
                .map(|m| {
                    let author_name = m
                        .from
                        .as_ref()
                        .and_then(|f| f.user.as_ref())
                        .and_then(|u| u.display_name.as_deref())
                        .unwrap_or("Unknown")
                        .to_string();
                    let timestamp = chrono::DateTime::parse_from_rfc3339(&m.created_date_time)
                        .map(|dt| dt.format("%H:%M").to_string())
                        .unwrap_or_default();
                    ViewRow {
                        id: m.id,
                        primary_text: m.body.content,
                        secondary_text: Some(author_name),
                        meta_text: Some(timestamp),
                        icon: None,
                        badge: None,
                        context_menu_target_kind: MenuTargetKind::Message,
                        preview_image_url: None,
                        is_video: false,
                    }
                })
                .collect();

            return Ok(ViewRowsPage { rows, next_cursor: None });
        }

        let servers = self.get_servers().await?;

        // Fetch channel counts concurrently for each team.
        let mut rows = Vec::with_capacity(servers.len());
        for server in &servers {
            let channel_count = self
                .get_channels(&server.id)
                .await
                .map(|chs| chs.len())
                .unwrap_or(0);

            let meta = format!(
                "{} channel{} · {} unread · @{} mentions",
                channel_count,
                if channel_count == 1 { "" } else { "s" },
                server.unread_count,
                server.mention_count,
            );

            rows.push(ViewRow {
                id: server.id.clone(),
                primary_text: server.name.clone(),
                secondary_text: server.description.clone(),
                meta_text: Some(meta),
                icon: None,
                badge: if server.mention_count > 0 {
                    Some(format!("@{}", server.mention_count))
                } else if server.unread_count > 0 {
                    Some(server.unread_count.to_string())
                } else {
                    None
                },
                context_menu_target_kind: MenuTargetKind::Server,
                preview_image_url: None,
                is_video: false,
            });
        }

        Ok(ViewRowsPage { rows, next_cursor: None })
    }

    async fn get_view_detail(
        &self,
        channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        // C.1: Fetch a single message and return its body as a detail block.
        // Graph has no single-message GET endpoint for channels; fall back to
        // the message list and find the row by id. This is a best-effort impl —
        // the message may have scrolled out of the default page. A paginated
        // search is deferred to a future pass (D.*).
        let msgs = if let Some((team_id, ch_id)) = channel_id.split_once('/') {
            self.http.get_channel_messages(team_id, ch_id, Some(50)).await?
        } else {
            self.http.get_chat_messages(channel_id, Some(50)).await?
        };

        let msg = msgs
            .into_iter()
            .find(|m| m.id == row_id)
            .ok_or_else(|| ClientError::NotFound(format!("message {row_id} not found in channel {channel_id}")))?;

        let author_name = msg
            .from
            .as_ref()
            .and_then(|f| f.user.as_ref())
            .and_then(|u| u.display_name.as_deref())
            .unwrap_or("Unknown")
            .to_string();

        // Teams Graph returns `body.content` as HTML when `contentType == "html"`,
        // or plain text otherwise. Wrap plain-text content in a <p> so the host
        // sanitizer treats it consistently.
        let body_html = if msg.body.content_type.as_deref() == Some("html") {
            format!("<p><strong>{author_name}:</strong></p>{}", msg.body.content)
        } else {
            // Inline-escape the three dangerous chars present in plain-text messages.
            let escaped = msg
                .body
                .content
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!("<p><strong>{author_name}:</strong> {escaped}</p>")
        };

        Ok(ViewDetail {
            body_block: CustomBlock {
                sanitized_html: body_html,
                stylesheet: None,
                max_height_px: None,
            },
            comments_section: None,
        })
    }
}
