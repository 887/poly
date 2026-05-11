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

use poly_test_common::HeaderInspectBuffer;

/// All mock Reddit state.
#[derive(Clone)]
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
    /// New top-level posts POSTed to /api/submit. Keyed by subreddit slug.
    pub submissions: Arc<DashMap<String, Vec<MockSubmission>>>,
    /// Auto-incremented base for synthesised DM ids.
    pub dm_seq: Arc<AtomicU64>,
    /// Auto-incremented base for synthesised comment ids.
    pub comment_seq: Arc<AtomicU64>,
    /// Auto-incremented base for synthesised submission ids.
    pub submission_seq: Arc<AtomicU64>,
    /// Header-inspect ring buffer used by the shared `BackendHarness`
    /// middleware to expose `/test/inspect/last-headers`.
    pub inspect: Arc<HeaderInspectBuffer>,
}

impl Default for RedditState {
    fn default() -> Self {
        Self {
            sessions: Arc::default(),
            users: Arc::default(),
            subscriptions: Arc::default(),
            inboxes: Arc::default(),
            sent: Arc::default(),
            votes: Arc::default(),
            comments: Arc::default(),
            submissions: Arc::default(),
            dm_seq: Arc::default(),
            comment_seq: Arc::default(),
            submission_seq: Arc::default(),
            inspect: Arc::new(HeaderInspectBuffer::new()),
        }
    }
}

impl poly_test_common::BackendHarness for RedditState {
    const BACKEND: &'static str = "reddit";

    fn new(_auth: poly_test_common::AuthState) -> Self {
        // Reddit's mock state has no persisted-token concept (sessions live
        // inline as a DashMap), so the auth-state argument is intentionally
        // discarded. Lifecycle (seed/reset/reseed) flows through the
        // BackendHarness defaults.
        Self::default()
    }

    fn seed(&self) { RedditState::seed(self); }
    fn reset(&self) { RedditState::reset(self); }
    // reseed() uses the default: reset() + seed()

    fn routes(state: Arc<Self>) -> axum::Router<Arc<Self>> {
        crate::routes_only(state)
    }

    fn inspect_buf(&self) -> Arc<HeaderInspectBuffer> {
        Arc::clone(&self.inspect)
    }
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
pub struct MockSubmission {
    pub id: String,
    pub sub: String,
    pub author: String,
    pub title: String,
    pub text: String,
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
    /// Idempotent — safe to call repeatedly. Used both by the legacy
    /// `seeded()` constructor and by `BackendHarness::seed()`.
    pub fn seed(&self) {
        if !self.users.is_empty() {
            return;
        }
        self.users.insert(
            "cat".to_string(),
            MockUser {
                name: "cat".to_string(),
                user_id: "t2_testcat".to_string(),
                avatar_animal: "cat".to_string(),
            },
        );
        self.users.insert(
            "dog".to_string(),
            MockUser {
                name: "dog".to_string(),
                user_id: "t2_testdog".to_string(),
                avatar_animal: "dog".to_string(),
            },
        );
        // Pre-subscribe both to r/rust + r/programming.
        self.subscriptions.insert(
            "cat".to_string(),
            vec!["rust".to_string(), "programming".to_string()],
        );
        self.subscriptions.insert(
            "dog".to_string(),
            vec!["rust".to_string(), "programming".to_string()],
        );
        // Pre-issue deterministic cookies so persisted KV tokens survive
        // restart. The signup flow stores `mock_session_<name>_0` as the
        // first issued cookie; mirror that here so restore_native_accounts
        // can replay the same value successfully.
        self.sessions
            .insert("mock_session_cat_0".to_string(), "cat".to_string());
        self.sessions
            .insert("mock_session_dog_0".to_string(), "dog".to_string());
    }

    /// Wipe all in-memory state to empty. Used by
    /// `BackendHarness::reset()` (and indirectly via `reseed()`).
    pub fn reset(&self) {
        self.sessions.clear();
        self.users.clear();
        self.subscriptions.clear();
        self.inboxes.clear();
        self.sent.clear();
        self.votes.clear();
        self.comments.clear();
        self.submissions.clear();
        self.dm_seq.store(0, Ordering::SeqCst);
        self.comment_seq.store(0, Ordering::SeqCst);
        self.submission_seq.store(0, Ordering::SeqCst);
    }

    /// Construct + seed in one call. Kept for callers that don't go
    /// through the harness (existing tests, `router_default()`).
    #[must_use]
    pub fn seeded() -> Self {
        let s = Self::default();
        s.seed();
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

    /// Record a delete request. Stored as a no-op sentinel — callers
    /// just need the 200-with-empty-errors-array response.
    pub fn record_delete(&self, _id: &str, _author: &str) {}

    /// Record an edit. Stored as a no-op sentinel.
    pub fn record_edit(&self, _id: &str, _author: &str, _new_text: &str) {}

    /// Mark a DM read. Stored as a no-op sentinel.
    pub fn mark_read(&self, _id: &str, _user: &str) {}

    /// Record a top-level submission. Returns the synthesised t3_ id
    /// (without prefix).
    pub fn record_submission(
        &self,
        sub: &str,
        author: &str,
        title: &str,
        text: &str,
    ) -> String {
        let id = format!("p{}", self.submission_seq.fetch_add(1, Ordering::SeqCst));
        let s = MockSubmission {
            id: id.clone(),
            sub: sub.to_string(),
            author: author.to_string(),
            title: title.to_string(),
            text: text.to_string(),
            when: Utc::now(),
        };
        self.submissions
            .entry(sub.to_string())
            .or_default()
            .insert(0, s);
        id
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
