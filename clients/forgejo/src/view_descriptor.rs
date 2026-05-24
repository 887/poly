//! `impl ViewDescriptorBackend for ForgejoClient` — sidebar layout, overview
//! card grid, issue/PR split views, paginated view rows, and view detail.

use crate::*;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for ForgejoClient {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::RepoTree,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-forgejo-overview-title".to_string()),
                subtitle_key: Some("plugin-forgejo-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        let title_key = if channel_id.starts_with("fj-pulls-") {
            "plugin-forgejo-view-pulls-title"
        } else if channel_id.starts_with("fj-discussions-") {
            "plugin-forgejo-view-discussions-title"
        } else {
            "plugin-forgejo-view-issues-title"
        };
        Ok(ViewDescriptor {
            kind: ViewKind::Split,
            header: Some(ViewHeader {
                title_key: Some(title_key.to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: Some(ViewToolbar {
                sort_options: vec![],
                filter_options: vec![
                    ToolbarOption { id: "open".to_string(), label_key: "plugin-forgejo-filter-open".to_string(), icon: None, default_selected: true },
                    ToolbarOption { id: "closed".to_string(), label_key: "plugin-forgejo-filter-closed".to_string(), icon: None, default_selected: false },
                ],
                tabs: vec![],
                action_items: vec![],
            }),
            body: ViewBody::SplitBody(SplitSpec {
                list_side: ListSpec {
                    row_template: RowTemplate {
                        primary_field: "title".to_string(),
                        secondary_field: Some("number".to_string()),
                        meta_field: Some("state-labels-author".to_string()),
                        icon_field: None,
                    },
                    page_size: 30,
                },
                detail_view_kind: ViewKind::FlatList,
            }),
        })
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        filter_id: Option<&str>,
        tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        if channel_id.is_empty() || channel_id == "fj-overview" {
            let repos = self.repos.lock().await;
            let page: u32 = cursor
                .as_ref()
                .and_then(|c| c.value.parse().ok())
                .unwrap_or(1);
            let page_size: usize = 30;
            let start = usize::try_from(page.saturating_sub(1))
                .unwrap_or(usize::MAX)
                .saturating_mul(page_size);
            let slice: Vec<_> = repos.iter().skip(start).take(page_size).collect();
            let rows: Vec<ViewRow> = slice
                .iter()
                .map(|r| ViewRow {
                    id: mapping::server_id_for_repo(r),
                    primary_text: r.full_name.clone(),
                    secondary_text: r.description.clone(),
                    meta_text: Some(format!(
                        "⭐ {} · 🍴 {} · {} open issues",
                        r.stars_count, r.forks_count, r.open_issues_count
                    )),
                    icon: None,
                    badge: None,
                    context_menu_target_kind: MenuTargetKind::Server,
                    preview_image_url: None,
                    is_video: false,
                })
                .collect();
            let next_cursor = if repos.len() > start.saturating_add(page_size) {
                Some(Cursor { kind: CursorKind::Offset, value: page.saturating_add(1).to_string() })
            } else {
                None
            };
            return Ok(ViewRowsPage { rows, next_cursor });
        }

        if tab_id == Some("discussions") || channel_id.starts_with("fj-discussions-") {
            return Ok(ViewRowsPage { rows: Vec::new(), next_cursor: None });
        }

        let (owner, repo) = channel_ids::parse_forum_channel(channel_id)?;
        let state = filter_id.unwrap_or("open");

        let want_pulls = tab_id == Some("pulls") || channel_id.starts_with("fj-pulls-");
        let issue_type = if want_pulls { "pulls" } else { "issues" };

        let page: u32 = cursor
            .as_ref()
            .and_then(|c| c.value.parse().ok())
            .unwrap_or(1);

        let raw = self
            .api
            .list_repo_issues_paged(&owner, &repo, state, issue_type, page)
            .await?;

        let rows: Vec<_> = raw.iter().map(mapping::map_issue_to_viewrow).collect();

        let next_cursor = if rows.len() == 30 {
            Some(Cursor { kind: CursorKind::Offset, value: page.saturating_add(1).to_string() })
        } else {
            None
        };

        Ok(ViewRowsPage { rows, next_cursor })
    }

    async fn get_view_detail(
        &self,
        channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        let (owner, repo) = channel_ids::parse_forum_channel(channel_id)?;
        let index: u64 = row_id
            .parse()
            .map_err(|_err| ClientError::NotFound(format!("row_id must be an issue number: {row_id}")))?;
        let issue = self.api.get_issue(&owner, &repo, index).await?;
        let comments = self
            .api
            .list_issue_comments(&owner, &repo, index)
            .await
            .unwrap_or_default();
        Ok(mapping::issue_to_view_detail(&issue, &comments))
    }
}
