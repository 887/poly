//! HTTP client for the HN Firebase API.

use std::sync::{Arc, Mutex};

use futures::future;
use poly_client::{ClientError, ClientResult};

use crate::cache::HnCache;
use crate::types::{HnFeed, HnItem, HnUpdates, HnUser};

const MAX_CONCURRENT: usize = 10;

/// HTTP client for the Hacker News Firebase API.
#[derive(Clone)]
pub struct HnApiClient {
    http: reqwest::Client,
    base_url: String,
    cache: Arc<Mutex<HnCache>>,
}

impl HnApiClient {
    /// Create a new client pointing at the official HN API.
    pub fn new() -> Self {
        Self::with_base_url("https://hacker-news.firebaseio.com/v0".to_string())
    }

    /// Create a new client pointing at a custom base URL (useful for tests).
    pub fn with_base_url(base_url: String) -> Self {
        let builder = reqwest::Client::builder();
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder.timeout(std::time::Duration::from_secs(10));
        let http = builder.build().unwrap_or_default();

        Self {
            http,
            base_url,
            cache: Arc::new(Mutex::new(HnCache::new())),
        }
    }

    fn item_url(&self, id: u64) -> String {
        format!("{}/item/{}.json", self.base_url, id)
    }

    fn user_url(&self, username: &str) -> String {
        format!("{}/user/{}.json", self.base_url, username)
    }

    fn feed_url(&self, feed: HnFeed) -> String {
        format!("{}/{}", self.base_url, feed.path())
    }

    fn updates_url(&self) -> String {
        format!("{}/updates.json", self.base_url)
    }

    /// Fetch the list of story IDs for a feed. Uses cache when available.
    pub async fn get_feed_ids(&self, feed: HnFeed) -> ClientResult<Vec<u64>> {
        {
            let cache = self.cache.lock().map_err(|_| {
                ClientError::Internal("cache lock poisoned".to_string())
            })?;
            if let Some(ids) = cache.get_feed(feed) {
                return Ok(ids.clone());
            }
        }

        let url = self.feed_url(feed);
        let ids: Vec<u64> = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;

        {
            let mut cache = self.cache.lock().map_err(|_| {
                ClientError::Internal("cache lock poisoned".to_string())
            })?;
            cache.put_feed(feed, ids.clone());
        }

        Ok(ids)
    }

    /// Fetch a single item by ID. Uses cache when available.
    pub async fn get_item(&self, id: u64) -> ClientResult<Option<HnItem>> {
        {
            let cache = self.cache.lock().map_err(|_| {
                ClientError::Internal("cache lock poisoned".to_string())
            })?;
            if let Some(item) = cache.get_item(id) {
                return Ok(Some(item.clone()));
            }
        }

        let url = self.item_url(id);
        let response: Option<HnItem> = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;

        if let Some(ref item) = response {
            let mut cache = self.cache.lock().map_err(|_| {
                ClientError::Internal("cache lock poisoned".to_string())
            })?;
            cache.put_item(item.clone());
        }

        Ok(response)
    }

    /// Fetch multiple items in parallel, up to `MAX_CONCURRENT` at a time.
    pub async fn get_items_batch(&self, ids: &[u64]) -> ClientResult<Vec<HnItem>> {
        let chunks: Vec<&[u64]> = ids.chunks(MAX_CONCURRENT).collect();
        let mut results = Vec::new();

        for chunk in chunks {
            let futures: Vec<_> = chunk.iter().map(|&id| self.get_item(id)).collect();
            let chunk_results = future::join_all(futures).await;
            for result in chunk_results {
                match result? {
                    Some(item) => results.push(item),
                    None => {} // null items (deleted/not found) are skipped
                }
            }
        }

        Ok(results)
    }

    /// Fetch a user profile by username.
    pub async fn get_user(&self, username: &str) -> ClientResult<Option<HnUser>> {
        {
            let cache = self.cache.lock().map_err(|_| {
                ClientError::Internal("cache lock poisoned".to_string())
            })?;
            if let Some(user) = cache.get_user(username) {
                return Ok(Some(user.clone()));
            }
        }

        let url = self.user_url(username);
        let response: Option<HnUser> = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;

        if let Some(ref user) = response {
            let mut cache = self.cache.lock().map_err(|_| {
                ClientError::Internal("cache lock poisoned".to_string())
            })?;
            cache.put_user(user.clone());
        }

        Ok(response)
    }

    /// Poll for recently updated items and profiles.
    pub async fn get_updates(&self) -> ClientResult<HnUpdates> {
        let url = self.updates_url();
        let updates: HnUpdates = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;

        Ok(updates)
    }

    /// Invalidate a cached item (e.g. after receiving an update notification).
    pub fn invalidate_item(&self, id: u64) -> ClientResult<()> {
        let mut cache = self.cache.lock().map_err(|_| {
            ClientError::Internal("cache lock poisoned".to_string())
        })?;
        cache.invalidate_item(id);
        Ok(())
    }
}

impl Default for HnApiClient {
    fn default() -> Self {
        Self::new()
    }
}
