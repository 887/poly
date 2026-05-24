//! `DiscoverBackend` impl for [`super::RedditBackend`].

use async_trait::async_trait;
use poly_client::ClientError;

use super::mapping::build_sub_server;
use super::RedditBackend;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DiscoverBackend for RedditBackend {
    async fn search_communities(
        &self,
        query: &str,
        _scope: poly_client::CommunityScope,
        cursor: Option<String>,
    ) -> poly_client::ClientResult<poly_client::CommunityPage> {
        let (subs, next_after) = self
            .client
            .search_subreddits(query, cursor.as_deref())
            .await
            .map_err(ClientError::from)?;

        let account_id = self.account_id().to_string();
        let account_display_name = self.account_display_name().to_string();
        let bt = Self::backend_type();

        let items = subs
            .into_iter()
            .map(|sub| {
                let mut server = build_sub_server(&sub.name, &account_id, &account_display_name, &bt);
                if let Some(url) = sub.icon_url {
                    server.icon_url = Some(url);
                }
                server
            })
            .collect();

        Ok(poly_client::CommunityPage {
            items,
            next_cursor: next_after,
        })
    }
}
