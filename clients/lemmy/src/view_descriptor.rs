//! `impl ViewDescriptorBackend for LemmyClient` — sidebar, view rows, post detail (C.1).
//!
//! Split out of `lib.rs` for Single Responsibility (B.1).

use async_trait::async_trait;
use poly_client::{ClientResult, SidebarDeclaration, SidebarLayoutKind, ActionOutcome, ClientError, SettingsScope, ViewDescriptor, ViewKind, ViewHeader, ViewBody, CardSpec, ListSpec, RowTemplate, ViewToolbar, TreeSpec, Cursor, ViewRowsPage, ViewRow, MessageContent, MenuTargetKind, ViewDetail, CustomBlock};

use crate::LemmyClient;
use crate::api::{
    cursor_to_page, map_comment_to_message, map_community_to_viewrow, map_post_to_viewrow,
    next_page_cursor,
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for LemmyClient {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Communities,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        let sort_value = match action_id {
            "sort-hot" => "Hot",
            "sort-active" => "Active",
            "sort-scaled" => "Scaled",
            "sort-controversial" => "Controversial",
            "sort-new" => "New",
            "sort-old" => "Old",
            "sort-most-comments" => "MostComments",
            "sort-new-comments" => "NewComments",
            "sort-top" | "sort-top-day" => "TopDay",
            "sort-top-hour" => "TopHour",
            "sort-top-six-hours" => "TopSixHour",
            "sort-top-twelve-hours" => "TopTwelveHour",
            "sort-top-week" => "TopWeek",
            "sort-top-month" => "TopMonth",
            "sort-top-year" => "TopYear",
            "sort-top-all" => "TopAll",
            _ => {
                return Err(ClientError::NotFound(format!(
                    "unknown sidebar action: {action_id}"
                )));
            }
        };
        self.settings_storage.set(
            SettingsScope::AccountGlobal,
            "",
            "current-sort",
            sort_value,
        )?;
        Ok(ActionOutcome::RefreshTarget)
    }

    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-lemmy-overview-title".to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        if Self::parse_comments_channel(channel_id).is_some() {
            return Ok(ViewDescriptor {
                kind: ViewKind::FlatList,
                header: Some(ViewHeader {
                    title_key: Some("plugin-lemmy-view-comments-title".to_string()),
                    subtitle_key: None,
                    info_block: None,
                }),
                toolbar: None,
                body: ViewBody::ListBody(ListSpec {
                    row_template: RowTemplate {
                        primary_field: "text".to_string(),
                        secondary_field: Some("author".to_string()),
                        meta_field: None,
                        icon_field: None,
                    },
                    page_size: 50,
                }),
            });
        }
        Ok(ViewDescriptor {
            kind: ViewKind::Tree,
            header: Some(ViewHeader {
                title_key: Some("plugin-lemmy-view-posts-title".to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: Some(ViewToolbar {
                sort_options: vec![],
                filter_options: vec![],
                tabs: vec![],
                action_items: vec![],
            }),
            body: ViewBody::TreeBody(TreeSpec {
                root_page_size: 25,
                max_depth: 8,
            }),
        })
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        cursor: Option<Cursor>,
        sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        if channel_id.is_empty() || channel_id == "lemmy-overview" {
            let resp = self.http.fetch_subscribed_communities().await?;
            let rows: Vec<ViewRow> = resp
                .communities
                .iter()
                .map(|view| map_community_to_viewrow(view, 0))
                .collect();
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }

        if let Some(community_id) = Self::parse_comments_channel(channel_id) {
            let limit: u32 = 50;
            let resp = self.http.fetch_community_comments(community_id, limit).await?;
            let rows: Vec<ViewRow> = resp.comments.iter().map(|view| {
                let msg = map_comment_to_message(view);
                let content_text = match &msg.content {
                    MessageContent::Text(s) => s.clone(),
                    MessageContent::WithAttachments { text, .. } => text.clone(),
                };
                let icon = msg.author.avatar_url;
                ViewRow {
                    id: msg.id,
                    primary_text: content_text,
                    secondary_text: Some(msg.author.display_name),
                    meta_text: None,
                    icon,
                    badge: None,
                    context_menu_target_kind: MenuTargetKind::Message,
                    preview_image_url: None,
                    is_video: false,
                }
            }).collect();
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }

        let community_id = Self::parse_feed_channel(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!(
                "get_view_rows: channel must be a lemmy-feed-{{id}} or lemmy-overview: {channel_id}"
            ))
        })?;

        let page = cursor_to_page(cursor.as_ref());
        let stored_sort = self.settings_storage.get(
            SettingsScope::AccountGlobal,
            "",
            "current-sort",
        );
        let sort: &str = sort_id
            .or(stored_sort.as_deref())
            .unwrap_or("Hot");
        let page_size: u32 = 25;

        let resp = self
            .http
            .fetch_posts_paged(community_id, sort, page, page_size)
            .await?;

        let now = chrono::Utc::now();
        let render_previews = self.render_previews_enabled();
        let rows: Vec<ViewRow> = resp.posts.iter().map(|v| map_post_to_viewrow(v, now, render_previews)).collect();
        let next_cursor = next_page_cursor(page, page_size.try_into().unwrap_or(usize::MAX), rows.len());

        Ok(ViewRowsPage { rows, next_cursor })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        fn html_escape(s: &str) -> String {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        }

        let post_id = row_id
            .parse::<i64>()
            .ok()
            .or_else(|| {
                row_id
                    .rsplit('/')
                    .next()
                    .and_then(|last| last.parse::<i64>().ok())
            })
            .ok_or_else(|| {
                ClientError::NotFound(format!("get_view_detail: cannot parse row id: {row_id}"))
            })?;

        let post_view = self.http.fetch_post(post_id).await?;
        let body = post_view.post.body.clone().unwrap_or_default();
        let url_line = post_view
            .post
            .url
            .as_deref()
            .map(|u| format!("<p><a href=\"{}\">{}</a></p>", html_escape(u), html_escape(u)))
            .unwrap_or_default();
        let sanitized_html = format!(
            "<h3>{}</h3>{}<p>{}</p>",
            html_escape(&post_view.post.name),
            url_line,
            html_escape(&body),
        );

        Ok(ViewDetail {
            body_block: CustomBlock {
                sanitized_html,
                stylesheet: None,
                max_height_px: None,
            },
            comments_section: Some(TreeSpec {
                root_page_size: 25,
                max_depth: 8,
            }),
        })
    }
}
