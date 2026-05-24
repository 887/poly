//! `ViewDescriptorBackend` implementation for `HackerNewsClient`.
//!
//! Describes the sidebar layout, account overview, per-channel view shape,
//! and row/detail fetching for HN story feeds.

use async_trait::async_trait;
use poly_client::*;

use crate::HackerNewsClient;
use crate::mapping::{hn_item_to_overview_row, hn_item_to_view_row};
use crate::types::HnFeed;

// ── C.1 — ViewDescriptorBackend ──────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for HackerNewsClient {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Feed,
            sections: Vec::new(),
            header_block: None,
        })
    }

    /// HN account overview — show the top stories as a curated welcome view.
    /// HN has no concept of multiple servers/accounts beyond "the front page",
    /// so the overview is simply the current Top feed rendered as a ListBody.
    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::FlatList,
            header: Some(ViewHeader {
                title_key: Some("plugin-hackernews-overview-title".to_string()),
                subtitle_key: Some("plugin-hackernews-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::ListBody(ListSpec {
                row_template: RowTemplate {
                    primary_field: "title".to_string(),
                    secondary_field: Some("author-domain".to_string()),
                    meta_field: Some("points-comments-age".to_string()),
                    icon_field: None,
                },
                page_size: 30,
            }),
        })
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::FlatList,
            header: Some(ViewHeader {
                title_key: Some("plugin-hackernews-view-stories-title".to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::ListBody(ListSpec {
                row_template: RowTemplate {
                    primary_field: "title".to_string(),
                    secondary_field: Some("url".to_string()),
                    meta_field: Some("score-comments-age".to_string()),
                    icon_field: None,
                },
                page_size: 30,
            }),
        })
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        let (feed, is_overview) = if channel_id.is_empty() {
            (HnFeed::Top, true)
        } else {
            let f = HnFeed::from_channel_id(channel_id).ok_or_else(|| {
                ClientError::NotFound(format!("unknown channel: {channel_id}"))
            })?;
            (f, false)
        };

        let offset: usize = cursor
            .as_ref()
            .and_then(|c| {
                if c.kind == CursorKind::Offset {
                    c.value.parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let page_size: usize = 30;

        let ids = self.api.get_feed_ids(feed).await?;
        let slice: Vec<u64> = ids
            .into_iter()
            .skip(offset)
            .take(page_size)
            .collect();

        let next_cursor = if slice.len() == page_size {
            Some(Cursor {
                kind: CursorKind::Offset,
                value: offset.saturating_add(page_size).to_string(),
            })
        } else {
            None
        };

        let items = self.api.get_items_batch(&slice).await?;

        let rows = items
            .iter()
            .filter(|item| !item.deleted.unwrap_or(false) && !item.dead.unwrap_or(false))
            .map(|item| {
                if is_overview {
                    hn_item_to_overview_row(item)
                } else {
                    hn_item_to_view_row(item)
                }
            })
            .collect();

        Ok(ViewRowsPage { rows, next_cursor })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        let story_id: u64 = row_id.parse().map_err(|_e| {
            ClientError::NotFound(format!("invalid story id: {row_id}"))
        })?;

        let story = self
            .api
            .get_item(story_id)
            .await?
            .ok_or_else(|| ClientError::NotFound(format!("story not found: {story_id}")))?;

        let body_html = if let Some(ref text) = story.text {
            format!("<p>{text}</p>")
        } else if let Some(ref url) = story.url {
            let title = story.title.as_deref().unwrap_or("Link");
            format!("<p><a href=\"{url}\">{title}</a></p>")
        } else {
            let title = story.title.as_deref().unwrap_or("(no title)");
            format!("<p>{title}</p>")
        };

        let has_comments = story.kids.as_ref().is_some_and(|k| !k.is_empty());
        let comments_section = if has_comments {
            Some(poly_client::TreeSpec {
                root_page_size: 30,
                max_depth: 8,
            })
        } else {
            None
        };

        Ok(ViewDetail {
            body_block: CustomBlock {
                sanitized_html: body_html,
                stylesheet: None,
                max_height_px: None,
            },
            comments_section,
        })
    }
}
