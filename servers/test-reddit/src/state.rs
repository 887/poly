//! In-memory state for the mock Reddit server.
//!
//! Reddit fixtures live in `clients/reddit/tests/fixtures/` and are
//! `include_str!`'d at compile time. Per-request mutations
//! (subscribe, compose, comment, vote) live here.

#![allow(clippy::module_name_repetitions)]

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// All mock Reddit state.
#[derive(Clone, Default)]
pub struct RedditState {
    /// session-cookie → username
    pub sessions: Arc<DashMap<String, String>>,
    /// username → password (mock — always accepts "testpass123")
    pub users: Arc<DashMap<String, MockUser>>,
    /// Per-user subscriptions: username → set of subreddit slugs.
    pub subscriptions: Arc<DashMap<String, Vec<String>>>,
    /// Per-user inbox: username → ordered Vec of mock DMs (newest first).
    pub inboxes: Arc<DashMap<String, Vec<MockDm>>>,
    /// Per-user sent: username → ordered Vec.
    pub sent: Arc<DashMap<String, Vec<MockDm>>>,
    /// Per-(post, user) → vote (-1, 0, 1).
    pub votes: Arc<DashMap<(String, String), i8>>,
    /// New comments POSTed to /api/comment. Keyed by parent thing id.
    pub comments: Arc<DashMap<String, Vec<MockComment>>>,
    /// Auto-incremented base for synthesised DM ids.
    pub dm_seq: Arc<AtomicU64>,
    /// Auto-incremented base for synthesised comment ids.
    pub comment_seq: Arc<AtomicU64>,
}

#[derive(Clone, Debug)]
pub struct MockUser {
    pub name: String,
    pub user_id: String,
    pub avatar_animal: String,
}

#[derive(Clone, Debug)]
pub struct MockDm {
    /// `t4_<id>` without the prefix.
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub when: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct MockComment {
    pub id: String,
    pub parent_id: String,
    pub author: String,
    pub body: String,
    pub when: DateTime<Utc>,
}

impl RedditState {
    /// Pre-populate with the canonical 🐱 cat + 🐶 dog test users.
    /// Both share the throwaway test password `testpass123`.
    #[must_use]
    pub fn seeded() -> Self {
        let s = Self::default();
        s.users.insert(
            "cat".to_string(),
            MockUser {
                name: "cat".to_string(),
                user_id: "t2_testcat".to_string(),
                avatar_animal: "cat".to_string(),
            },
        );
        s.users.insert(
            "dog".to_string(),
            MockUser {
                name: "dog".to_string(),
                user_id: "t2_testdog".to_string(),
                avatar_animal: "dog".to_string(),
            },
        );
        // Pre-subscribe both to r/rust + r/programming.
        s.subscriptions.insert(
            "cat".to_string(),
            vec!["rust".to_string(), "programming".to_string()],
        );
        s.subscriptions.insert(
            "dog".to_string(),
            vec!["rust".to_string(), "programming".to_string()],
        );
        // Pre-issue deterministic cookies so persisted KV tokens survive
        // restart. The signup flow stores `mock_session_<name>_0` as the
        // first issued cookie; mirror that here so restore_native_accounts
        // can replay the same value successfully.
        s.sessions
            .insert("mock_session_cat_0".to_string(), "cat".to_string());
        s.sessions
            .insert("mock_session_dog_0".to_string(), "dog".to_string());
        s
    }

    /// Look up the username associated with a `reddit_session` cookie.
    #[must_use]
    pub fn user_for_cookie(&self, cookie: &str) -> Option<String> {
        self.sessions.get(cookie).map(|r| r.clone())
    }

    /// Issue a fresh session cookie for a user. Returns the cookie value
    /// the test client should set on subsequent requests.
    pub fn issue_session(&self, username: &str) -> String {
        // Mock cookie format: `mock_session_<username>_<random>`. Real
        // Reddit issues a JWT; for the mock the only guarantee is
        // uniqueness + reverse-lookup.
        let suffix = self.dm_seq.fetch_add(1, Ordering::SeqCst);
        let cookie = format!("mock_session_{username}_{suffix}");
        self.sessions.insert(cookie.clone(), username.to_string());
        cookie
    }

    /// Add a DM to the recipient's inbox + sender's sent folder.
    /// Returns the synthesised t4_ id (without prefix).
    pub fn record_dm(&self, from: &str, to: &str, subject: &str, body: &str) -> String {
        let id = format!("dm{}", self.dm_seq.fetch_add(1, Ordering::SeqCst));
        let dm = MockDm {
            id: id.clone(),
            from: from.to_string(),
            to: to.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            when: Utc::now(),
        };
        self.inboxes
            .entry(to.to_string())
            .or_default()
            .insert(0, dm.clone());
        self.sent.entry(from.to_string()).or_default().insert(0, dm);
        id
    }

    /// Record a vote.
    pub fn record_vote(&self, post_id: &str, user: &str, dir: i8) {
        self.votes.insert((post_id.to_string(), user.to_string()), dir);
    }

    /// Record a comment under a parent (`t1_` or `t3_`). Returns the
    /// synthesised t1_ id (without prefix).
    pub fn record_comment(&self, parent_id: &str, author: &str, body: &str) -> String {
        let id = format!("c{}", self.comment_seq.fetch_add(1, Ordering::SeqCst));
        let c = MockComment {
            id: id.clone(),
            parent_id: parent_id.to_string(),
            author: author.to_string(),
            body: body.to_string(),
            when: Utc::now(),
        };
        self.comments
            .entry(parent_id.to_string())
            .or_default()
            .push(c);
        id
    }
}
