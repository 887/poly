//! In-memory TTL cache for HN items and feed ID lists.
//!
//! The clock is `chrono::Utc::now()` rather than `std::time::Instant`: this
//! crate's primary build target is wasm32 (the bundle the Wry / Electron
//! shells load), where `Instant::now()` is unimplemented. `chrono` is
//! compiled with `wasmbind`, so `Utc::now()` works on every target and the
//! TTLs below are honoured everywhere.
//!
//! Capacity caps are enforced independently of the TTLs: the desktop shells
//! stay open for days, and `get_items_batch` inserts every item the user
//! scrolls past, so an uncapped map grows monotonically for the life of the
//! process.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::types::{HnFeed, HnItem, HnUser};

const FEED_TTL: Duration = Duration::from_secs(120);
const STORY_TTL: Duration = Duration::from_secs(300);
const COMMENT_TTL: Duration = Duration::from_secs(600);
const USER_TTL: Duration = Duration::from_secs(1800);

/// Maximum number of cached items (stories + comments).
const MAX_ITEMS: usize = 2_000;
/// Maximum number of cached user profiles.
const MAX_USERS: usize = 500;

struct Entry<T> {
    value: T,
    inserted_at: DateTime<Utc>,
    ttl: Duration,
}

impl<T> Entry<T> {
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            inserted_at: Utc::now(),
            ttl,
        }
    }

    fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        let age = now.signed_duration_since(self.inserted_at);
        // A negative age means the wall clock moved backwards; treat that as
        // "not expired" rather than as an enormous age.
        age.to_std().is_ok_and(|elapsed| elapsed > self.ttl)
    }

    fn is_expired(&self) -> bool {
        self.is_expired_at(Utc::now())
    }
}

/// Drop expired entries, then — if still at or above `capacity` — the
/// oldest-inserted entries until one free slot remains.
fn evict<K: Clone + std::hash::Hash + Eq, V>(map: &mut HashMap<K, Entry<V>>, capacity: usize) {
    let now = Utc::now();
    map.retain(|_, entry| !entry.is_expired_at(now));
    while map.len() >= capacity {
        let oldest = map
            .iter()
            .min_by_key(|(_, entry)| entry.inserted_at)
            .map(|(key, _)| key.clone());
        match oldest {
            Some(key) => {
                map.remove(&key);
            }
            None => break,
        }
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
            crate::types::HnItemType::Story
            | crate::types::HnItemType::Job
            | crate::types::HnItemType::Poll
            | crate::types::HnItemType::PollOpt => STORY_TTL,
        };
        if !self.items.contains_key(&id) {
            evict(&mut self.items, MAX_ITEMS);
        }
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
    ///
    /// Not capacity-capped: `HnFeed` is a small closed enum.
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
        if !self.users.contains_key(&key) {
            evict(&mut self.users, MAX_USERS);
        }
        self.users.insert(key, Entry::new(user, USER_TTL));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::{evict, Entry, HnCache, MAX_ITEMS};
    use crate::types::{HnItem, HnItemType};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::time::Duration;

    fn story(id: u64) -> HnItem {
        HnItem {
            id,
            item_type: HnItemType::Story,
            by: None,
            time: None,
            text: None,
            url: None,
            title: None,
            score: None,
            descendants: None,
            kids: None,
            parent: None,
            dead: None,
            deleted: None,
        }
    }

    /// TTL expiry must work on every target — the old wasm arm returned
    /// `false` unconditionally, freezing the front page for the life of the
    /// (long-lived) desktop shell.
    #[test]
    fn entry_expires_once_past_its_ttl() {
        let entry = Entry::new(1_u32, Duration::from_secs(60));
        let now = Utc::now();
        assert!(!entry.is_expired_at(now));
        assert!(!entry.is_expired_at(now + chrono::Duration::seconds(59)));
        assert!(entry.is_expired_at(now + chrono::Duration::seconds(61)));
    }

    #[test]
    fn backwards_clock_does_not_expire_an_entry() {
        let entry = Entry::new(1_u32, Duration::from_secs(60));
        let now = Utc::now();
        assert!(!entry.is_expired_at(now - chrono::Duration::seconds(3600)));
    }

    #[test]
    fn evict_drops_expired_entries_first() {
        let mut map: HashMap<u64, Entry<u32>> = HashMap::new();
        map.insert(1, Entry::new(1_u32, Duration::from_secs(0)));
        map.insert(2, Entry::new(2_u32, Duration::from_secs(3600)));
        // ttl == 0 means "already older than its ttl" as soon as any time
        // passes; force the comparison by evicting against a capacity that
        // cannot be hit, so only the expiry sweep runs.
        std::thread::sleep(Duration::from_millis(2));
        evict(&mut map, usize::MAX);
        assert!(!map.contains_key(&1), "expired entry must be swept");
        assert!(map.contains_key(&2), "live entry must survive");
    }

    /// The item map must stay bounded no matter how much the user scrolls.
    #[test]
    fn put_item_is_capacity_bounded() {
        let mut cache = HnCache::new();
        for id in 0..u64::try_from(MAX_ITEMS).unwrap().saturating_add(50) {
            cache.put_item(story(id));
        }
        assert!(
            cache.items.len() <= MAX_ITEMS,
            "cache must stay under the cap, got {}",
            cache.items.len()
        );
    }

    #[test]
    fn put_item_updating_an_existing_key_does_not_evict() {
        let mut cache = HnCache::new();
        cache.put_item(story(7));
        cache.put_item(story(8));
        cache.put_item(story(7));
        assert_eq!(cache.items.len(), 2);
        assert!(cache.get_item(8).is_some(), "unrelated entry must survive an update");
    }
}
