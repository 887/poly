//! `impl DiscoverBackend for LemmyClient` — community search (H.4.c).
//!
//! Split out of `lib.rs` for Single Responsibility (B.1).

use async_trait::async_trait;
use poly_client::*;

use crate::LemmyClient;
use crate::api::map_community_to_server;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DiscoverBackend for LemmyClient {
    async fn search_communities(
        &self,
        query: &str,
        scope: CommunityScope,
        cursor: Option<String>,
    ) -> ClientResult<CommunityPage> {
        let listing_type = match scope {
            CommunityScope::Subscribed => "Subscribed",
            CommunityScope::Local => "Local",
            CommunityScope::All => "All",
        };
        let session = self.http.session().ok_or_else(|| {
            ClientError::AuthFailed("Lemmy: not authenticated".to_string())
        })?;
        let account_id = session.user_id.to_string();
        let account_display_name = session.user_display_name.clone();
        let resp = self.http.search_communities(
            query,
            listing_type,
            cursor.as_deref(),
        ).await?;

        // Lemmy returns exactly `limit` items (50) when a full page exists.
        // Next page cursor is the 1-based page number incremented as a string.
        let current_page: u32 = cursor
            .as_deref()
            .and_then(|c| c.parse().ok())
            .unwrap_or(1u32);
        let next_cursor = if resp.communities.len() == 50 {
            Some((current_page + 1).to_string())
        } else {
            None
        };

        let items = resp
            .communities
            .iter()
            .map(|view| map_community_to_server(view, &account_id, &account_display_name))
            .collect();

        Ok(CommunityPage { items, next_cursor })
    }
}
