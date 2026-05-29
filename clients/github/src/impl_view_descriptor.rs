use async_trait::async_trait;
use poly_client::{ClientResult, SidebarDeclaration, SidebarLayoutKind, ViewDescriptor, ViewKind, ViewHeader, ViewBody, CardSpec, ViewToolbar, ToolbarOption, SplitSpec, ListSpec, RowTemplate, Cursor, ViewRowsPage, ViewRow, MenuTargetKind, CursorKind, ViewDetail, ClientError};

use crate::mapping;
use crate::GitHubClient;
use crate::forum::parse_forum_channel;
use crate::types;

// ── C.1 — ViewDescriptorBackend ──────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for GitHubClient {
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
                title_key: Some("plugin-github-overview-title".to_string()),
                subtitle_key: Some("plugin-github-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        let title_key = if channel_id.starts_with("gh-pulls-") {
            "plugin-github-view-pulls-title"
        } else if channel_id.starts_with("gh-discussions-") {
            "plugin-github-view-discussions-title"
        } else {
            "plugin-github-view-issues-title"
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
                    ToolbarOption { id: "open".to_string(), label_key: "plugin-github-filter-open".to_string(), icon: None, default_selected: true },
                    ToolbarOption { id: "closed".to_string(), label_key: "plugin-github-filter-closed".to_string(), icon: None, default_selected: false },
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
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        filter_id: Option<&str>,
        tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        if channel_id.is_empty() {
            let rows: Vec<ViewRow> = {
                let repos = self.repos.lock().await;
                repos.iter().map(|r| ViewRow {
                    id: mapping::server_id_for_repo(r),
                    primary_text: r.full_name.clone(),
                    secondary_text: r.description.clone(),
                    meta_text: Some(format!(
                        "★ {} · {} forks · {} open",
                        r.stargazers_count, r.forks_count, r.open_issues_count
                    )),
                    icon: None,
                    badge: r.language.clone(),
                    context_menu_target_kind: MenuTargetKind::Server,
                    preview_image_url: None,
                    is_video: false,
                }).collect()
            };
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }

        let (owner, repo) = parse_forum_channel(channel_id)?;

        if channel_id.starts_with("gh-discussions-") || tab_id == Some("discussions") {
            let (discussions, next_cursor) = self
                .cli
                .list_discussions(&owner, &repo, 50, None)
                .await
                .map_err(Self::convert_err)?;
            let rows = discussions
                .iter()
                .map(mapping::map_discussion_to_viewrow)
                .collect();
            return Ok(ViewRowsPage {
                rows,
                next_cursor: next_cursor.map(|v| Cursor {
                    kind: CursorKind::Opaque,
                    value: v,
                }),
            });
        }
        let state = filter_id.unwrap_or("open");

        let want_pulls = tab_id == Some("pulls")
            || channel_id.starts_with("gh-pulls-");
        let want_issues = tab_id == Some("issues")
            || channel_id.starts_with("gh-issues-");

        let endpoint = format!(
            "/repos/{owner}/{repo}/issues?state={state}&per_page=50&sort=updated"
        );
        let raw: Vec<types::GhIssue> = self
            .cli
            .api_get(&endpoint, &[])
            .await
            .map_err(Self::convert_err)?;

        let rows: Vec<_> = raw
            .iter()
            .filter(|i| {
                if want_pulls {
                    i.is_pull_request()
                } else if want_issues {
                    !i.is_pull_request()
                } else {
                    true
                }
            })
            .map(mapping::map_issue_to_viewrow)
            .collect();

        Ok(ViewRowsPage { rows, next_cursor: None })
    }

    async fn get_view_detail(
        &self,
        channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        if channel_id.starts_with("gh-discussions-") {
            return Err(ClientError::NotSupported(
                "GitHub discussions detail is not available via the REST API; \
                 open the discussion in your browser for the full view."
                    .to_string(),
            ));
        }
        let (owner, repo) = parse_forum_channel(channel_id)?;
        let number: u64 = row_id
            .parse()
            .map_err(|_e| ClientError::NotFound(format!("row_id must be an issue number: {row_id}")))?;
        let issue = self
            .cli
            .get_issue(&owner, &repo, number)
            .await
            .map_err(Self::convert_err)?;
        let comments = self
            .cli
            .list_issue_comments(&owner, &repo, number)
            .await
            .unwrap_or_default();
        Ok(mapping::issue_to_view_detail(&issue, &comments))
    }
}
