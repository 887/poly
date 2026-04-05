//! In-memory TTL cache for HN items and feed ID lists.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::types::{HnFeed, HnItem, HnUser};

const FEED_TTL: Duration = Duration::from_secs(120);
const STORY_TTL: Duration = Duration::from_secs(300);
const COMMENT_TTL: Duration = Duration::from_secs(600);
const USER_TTL: Duration = Duration::from_secs(1800);

struct Entry<T> {
    value: T,
    inserted_at: Instant,
    ttl: Duration,
}

impl<T> Entry<T> {
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            inserted_at: Instant::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() > self.ttl
    }
}

/// In-memory TTL cache for HN API responses.
#[derive(Default)]
pub struct HnCache {
    items: HashMap<u64, Entry<HnItem>>,
    feeds: HashMap<HnFeed, Entry<Vec<u64>>>,
    users: HashMap<String, Entry<HnUser>>,
}

impl HnCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a cached item by ID. Returns `None` if missing or expired.
    pub fn get_item(&self, id: u64) -> Option<&HnItem> {
        self.items.get(&id).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(&entry.value)
            }
        })
    }

    /// Insert or update an item in the cache.
    pub fn put_item(&mut self, item: HnItem) {
        let id = item.id;
        let ttl = match item.item_type {
            crate::types::HnItemType::Comment => COMMENT_TTL,
            _ => STORY_TTL,
        };
        self.items.insert(id, Entry::new(item, ttl));
    }

    /// Look up a cached feed ID list. Returns `None` if missing or expired.
    pub fn get_feed(&self, feed: HnFeed) -> Option<&Vec<u64>> {
        self.feeds.get(&feed).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(&entry.value)
            }
        })
    }

    /// Insert or update a feed ID list in the cache.
    pub fn put_feed(&mut self, feed: HnFeed, ids: Vec<u64>) {
        self.feeds.insert(feed, Entry::new(ids, FEED_TTL));
    }

    /// Look up a cached user profile.
    pub fn get_user(&self, username: &str) -> Option<&HnUser> {
        self.users.get(username).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(&entry.value)
            }
        })
    }

    /// Insert or update a user in the cache.
    pub fn put_user(&mut self, user: HnUser) {
        let key = user.id.clone();
        self.users.insert(key, Entry::new(user, USER_TTL));
    }

    /// Remove an item from the cache (e.g. after an update notification).
    pub fn invalidate_item(&mut self, id: u64) {
        self.items.remove(&id);
    }
}
