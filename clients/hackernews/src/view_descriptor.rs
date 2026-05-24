//! `ViewDescriptorBackend` implementation for `HackerNewsClient`.
//!
//! Describes the sidebar layout, account overview, per-channel view shape,
//! and row/detail fetching for HN story feeds.

use async_trait::async_trait;
use poly_client::*;

use crate::HackerNewsClient;
use crate::api::HnApiClient;
use crate::mapping::{hn_item_to_overview_row, hn_item_to_view_row};
use crate::types::HnFeed;

/// B.5 — Open/Closed-friendly classification of incoming `get_view_*` requests.
///
/// `get_view_rows` and `get_view_detail` previously dispatched ad-hoc on
/// `channel_id` shape. Routing through `HnViewKind` keeps the trait impl tiny
/// and makes adding a new view kind (e.g. a user-profile view) a single
/// variant + its `fetch_*` arm rather than surgery on a giant `match`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HnViewKind {
    /// Account-level overview (empty `channel_id`). Renders the Top feed
    /// with overview-style rows (author · domain).
    Overview,
    /// Per-feed channel (e.g. `hn-top`, `hn-best`, …). Renders the feed
    /// with standard story rows.
    Feed(HnFeed),
}

impl HnViewKind {
    /// Classify an incoming `channel_id` into a `HnViewKind`. Returns
    /// `ClientError::NotFound` when the channel is neither empty nor a known
    /// feed channel id.
    fn from_channel_id(channel_id: &str) -> ClientResult<Self> {
        if channel_id.is_empty() {
            Ok(Self::Overview)
        } else {
            HnFeed::from_channel_id(channel_id)
                .map(Self::Feed)
                .ok_or_else(|| {
                    ClientError::NotFound(format!("unknown channel: {channel_id}"))
                })
        }
    }

    /// Underlying feed to query against the HN Firebase API.
    fn feed(self) -> HnFeed {
        match self {
            Self::Overview => HnFeed::Top,
            Self::Feed(f) => f,
        }
    }

    /// Map an `HnItem` to a `ViewRow` using the layout appropriate for this
    /// view kind (overview vs feed).
    fn map_row(self, item: &crate::types::HnItem) -> ViewRow {
        match self {
            Self::Overview => hn_item_to_overview_row(item),
            Self::Feed(_) => hn_item_to_view_row(item),
        }
    }
}

/// Parse a cursor's offset value, defaulting to 0.
fn parse_offset(cursor: Option<&Cursor>) -> usize {
    cursor
        .and_then(|c| {
            if c.kind == CursorKind::Offset {
                c.value.parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

/// Fetch one paged slice of IDs from a feed, given offset+page_size.
async fn fetch_feed_page(
    api: &HnApiClient,
    feed: HnFeed,
    offset: usize,
    page_size: usize,
) -> ClientResult<(Vec<u64>, Option<Cursor>)> {
    let ids = api.get_feed_ids(feed).await?;
    let slice: Vec<u64> = ids.into_iter().skip(offset).take(page_size).collect();

    let next_cursor = if slice.len() == page_size {
        Some(Cursor {
            kind: CursorKind::Offset,
            value: offset.saturating_add(page_size).to_string(),
        })
    } else {
        None
    };
    Ok((slice, next_cursor))
}

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
        let kind = HnViewKind::from_channel_id(channel_id)?;
        let offset = parse_offset(cursor.as_ref());
        let page_size: usize = 30;

        let (ids, next_cursor) = fetch_feed_page(&self.api, kind.feed(), offset, page_size).await?;
        let items = self.api.get_items_batch(&ids).await?;

        let rows = items
            .iter()
            .filter(|item| !item.deleted.unwrap_or(false) && !item.dead.unwrap_or(false))
            .map(|item| kind.map_row(item))
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
