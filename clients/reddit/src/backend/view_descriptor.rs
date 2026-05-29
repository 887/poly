//! `ViewDescriptorBackend` impl for [`super::RedditBackend`].
//!
//! Owns the sidebar declaration (sort modes), the sort-action invocation,
//! and the view-row / view-detail rendering for subreddit channels.

use async_trait::async_trait;
use poly_client::{
    ActionOutcome, ClientError, ClientResult, Cursor, CustomBlock, ListSpec, RowTemplate,
    SettingsScope, SidebarDeclaration, SidebarItem, SidebarLayoutKind, SidebarRouteKind,
    SidebarSection, ViewBody, ViewDescriptor, ViewDetail, ViewKind, ViewRowsPage,
};

use super::ids::sub_from_channel_id;
use super::mapping::{raw_post_to_viewrow, render_comments_to_html, sort_kind_to_str};
use super::RedditBackend;
use crate::SortKind;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for RedditBackend {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        let items = vec![
            SidebarItem {
                id: "sort-reddit-hot".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-hot".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-new".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-new".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-rising".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-rising".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-controversial".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-controversial".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top".to_string(),
                parent_id: None,
                label_key: "ui-sidebar-sort-top".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-hour".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-hour".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-day".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-day".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-week".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-week".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-month".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-month".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-year".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-year".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
            SidebarItem {
                id: "sort-reddit-top-all".to_string(),
                parent_id: Some("sort-reddit-top".to_string()),
                label_key: "ui-sidebar-sort-top-all".to_string(),
                icon: None,
                badge: None,
                route_kind: SidebarRouteKind::Channel,
            },
        ];
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::SortModes,
            sections: vec![SidebarSection {
                header_key: None,
                collapsible: false,
                default_collapsed: false,
                items,
            }],
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        let sort = match action_id {
            "sort-reddit-hot" => SortKind::Hot,
            "sort-reddit-new" => SortKind::New,
            "sort-reddit-rising" => SortKind::Rising,
            "sort-reddit-controversial" => SortKind::Controversial,
            "sort-reddit-top" => SortKind::Top,
            "sort-reddit-top-hour" => SortKind::TopHour,
            "sort-reddit-top-day" => SortKind::TopDay,
            "sort-reddit-top-week" => SortKind::TopWeek,
            "sort-reddit-top-month" => SortKind::TopMonth,
            "sort-reddit-top-year" => SortKind::TopYear,
            "sort-reddit-top-all" => SortKind::TopAll,
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
            sort_kind_to_str(sort),
        )?;
        Ok(ActionOutcome::RefreshTarget)
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::FlatList,
            header: None,
            toolbar: None,
            body: ViewBody::ListBody(ListSpec {
                row_template: RowTemplate {
                    primary_field: "title".to_string(),
                    secondary_field: Some("author".to_string()),
                    meta_field: Some("score-comments-age".to_string()),
                    icon_field: None,
                },
                page_size: 25,
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
        let sub = sub_from_channel_id(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("channel not found: {channel_id}")))?;

        let posts = self
            .client
            .list_subreddit(sub, self.current_sort())
            .await
            .map_err(ClientError::from)?;

        let show_previews = self.media_previews_enabled();

        let rows = posts
            .iter()
            .map(|p| raw_post_to_viewrow(p, show_previews))
            .collect();

        Ok(ViewRowsPage { rows, next_cursor: None })
    }

    // lint-allow-unused: single-responsibility HTML render fn; splitting it into
    // sub-fns would scatter the CSS and gallery/comment logic with no clear seam.
    #[allow(clippy::too_many_lines)]
    async fn get_view_detail(
        &self,
        _channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        use std::fmt::Write as _;

        fn html_escape(s: &str) -> String {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        }

        let post_id = row_id
            .strip_prefix("t3_")
            .ok_or_else(|| ClientError::NotFound(format!("get_view_detail: not a t3_ row: {row_id}")))?;

        let (post, comments) = self.client.get_post(post_id).await.map_err(ClientError::from)?;

        let gallery_from_json = self
            .client
            .get_gallery_urls(post_id)
            .await
            .unwrap_or_default();

        let gallery_urls: Vec<String> = if gallery_from_json.len() >= 2 {
            gallery_from_json
        } else if let Some(ref preview) = post.preview_url {
            vec![preview.clone()]
        } else {
            Vec::new()
        };
        let is_real_gallery = gallery_urls.len() >= 2;

        let mut html = String::new();
        write!(html, "<h3>{}</h3>", html_escape(&post.title)).ok();
        write!(
            html,
            "<p class=\"reddit-post-meta\">by u/{} · {} points · {} comments</p>",
            html_escape(&post.author),
            post.score,
            post.comment_count,
        ).ok();
        if let Some(ref url) = post.url
            && gallery_urls.is_empty()
        {
            let escaped = html_escape(url);
            write!(html, "<p class=\"reddit-post-link\"><a href=\"{escaped}\">{escaped}</a></p>").ok();
        }
        if let Some(ref body) = post.body
            && !body.is_empty()
        {
            write!(html, "<div class=\"reddit-post-body\">{body}</div>").ok();
        }
        if !gallery_urls.is_empty() {
            let wrapper_class = if is_real_gallery {
                "reddit-gallery reddit-gallery-carousel"
            } else {
                "reddit-gallery"
            };
            write!(html, "<div class=\"{wrapper_class}\">").ok();
            let gallery_len = gallery_urls.len();
            for (i, url) in gallery_urls.iter().enumerate() {
                let alt = if is_real_gallery {
                    // lint-allow-unused: display index uses saturating_add for safety
                    format!("Gallery image {}/{gallery_len}", i.saturating_add(1))
                } else {
                    "Post image".to_string()
                };
                write!(
                    html,
                    "<img class=\"reddit-gallery-item\" src=\"{}\" alt=\"{}\" loading=\"lazy\" />",
                    html_escape(url),
                    html_escape(&alt),
                ).ok();
            }
            html.push_str("</div>");
            if is_real_gallery {
                write!(
                    html,
                    "<p class=\"reddit-gallery-count\">{gallery_len} images — swipe / scroll to view</p>",
                ).ok();
            }
        }

        if !comments.is_empty() {
            write!(
                html,
                "<h4 class=\"reddit-comments-heading\">Comments ({})</h4>",
                post.comment_count.min(9999),
            ).ok();
            html.push_str("<div class=\"reddit-comments\">");
            render_comments_to_html(&mut html, &comments, 0, 8);
            html.push_str("</div>");
        }

        let stylesheet = Some(
            ".reddit-post-meta { color: var(--text-muted, #888); font-size: 0.85rem; }
             .reddit-post-body { margin: 12px 0; line-height: 1.5; }
             .reddit-post-link a { color: var(--text-link, #60a5fa); word-break: break-all; }
             .reddit-gallery {
                 display: flex;
                 gap: 8px;
                 margin-top: 12px;
                 align-items: flex-start;
             }
             .reddit-gallery-carousel {
                 overflow-x: auto;
                 scroll-snap-type: x mandatory;
                 scroll-behavior: smooth;
                 padding-bottom: 8px;
             }
             .reddit-gallery-carousel .reddit-gallery-item {
                 scroll-snap-align: center;
                 flex: 0 0 auto;
             }
             .reddit-gallery-item {
                 max-width: min(100%, 480px);
                 max-height: 540px;
                 object-fit: contain;
                 border-radius: 6px;
                 background: rgba(0, 0, 0, 0.3);
             }
             .reddit-gallery-count {
                 color: var(--text-muted, #888);
                 font-size: 0.8rem;
                 margin: 4px 0 0;
             }
             .reddit-comments-heading {
                 margin-top: 24px;
                 padding-top: 12px;
                 border-top: 1px solid var(--border-primary, #333);
             }
             .reddit-comments { display: flex; flex-direction: column; gap: 12px; }
             .reddit-comment {
                 padding: 8px 12px;
                 border-left: 2px solid var(--border-primary, #333);
                 background: rgba(255, 255, 255, 0.02);
                 border-radius: 0 4px 4px 0;
             }
             .reddit-comment-meta {
                 color: var(--text-muted, #888);
                 font-size: 0.78rem;
                 margin-bottom: 4px;
             }
             .reddit-comment-body { line-height: 1.45; }
             .reddit-comment-body p { margin: 4px 0; }"
                .to_string(),
        );

        Ok(ViewDetail {
            body_block: CustomBlock {
                sanitized_html: html,
                stylesheet,
                max_height_px: None,
            },
            comments_section: None,
        })
    }
}
