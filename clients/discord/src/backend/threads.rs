//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::DiscordClient;
use async_trait::async_trait;
use poly_client::{ClientResult, ThreadInfo};

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ThreadsBackend for DiscordClient {
    async fn get_active_threads(&self, server_id: &str) -> ClientResult<Vec<ThreadInfo>> {
        let resp = self.http.get_active_threads(server_id).await?;
        Ok(resp.threads.into_iter().map(|t| Self::discord_thread_to_thread_info(&t)).collect())
    }

    async fn get_archived_threads(
        &self,
        parent_channel_id: &str,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ThreadInfo>> {
        let resp = self.http.get_archived_threads_public(parent_channel_id, limit).await?;
        Ok(resp.threads.into_iter().map(|t| Self::discord_thread_to_thread_info(&t)).collect())
    }
}
