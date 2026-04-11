//! In-memory state for the mock Lemmy server.

use dashmap::DashMap;
use poly_test_common::AuthState;

/// All mock Lemmy state: users, communities, posts, tokens.
#[derive(Clone)]
pub struct LemmyState {
    pub auth: AuthState,
    pub users: DashMap<String, User>,
    /// community_id → Community
    pub communities: DashMap<String, Community>,
    /// community_id → Vec<Post>
    pub posts: DashMap<String, Vec<Post>>,
    /// username → password
    pub passwords: DashMap<String, String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub actor_id: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Community {
    pub id: i64,
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub actor_id: String,
    pub subscribed: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Post {
    pub id: i64,
    pub name: String,
    pub body: Option<String>,
    pub url: Option<String>,
    pub creator_id: i64,
    pub creator_name: String,
    pub community_id: i64,
    pub published: String,
    pub score: i32,
    pub comment_count: i32,
}

impl LemmyState {
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            communities: DashMap::new(),
            posts: DashMap::new(),
            passwords: DashMap::new(),
        }
    }

    /// Seed demo data (idempotent).
    pub fn seed(&self) {
        if !self.users.is_empty() {
            return;
        }

        // Users — testuser for legacy integration tests; beaver/hedgehog are
        // the "animal" accounts exposed to poly-web via signup::get_test_accounts().
        self.users.insert(
            "testuser".to_string(),
            User {
                id: 1,
                name: "testuser".to_string(),
                display_name: Some("Test User".to_string()),
                avatar: None,
                actor_id: "https://lemmy.example.com/u/testuser".to_string(),
            },
        );
        self.passwords.insert("testuser".to_string(), "password123".to_string());

        self.users.insert(
            "beaver".to_string(),
            User {
                id: 2,
                name: "beaver".to_string(),
                display_name: Some("Beaver".to_string()),
                avatar: None,
                actor_id: "https://lemmy.example.com/u/beaver".to_string(),
            },
        );
        self.passwords.insert("beaver".to_string(), "testpass123".to_string());

        self.users.insert(
            "hedgehog".to_string(),
            User {
                id: 3,
                name: "hedgehog".to_string(),
                display_name: Some("Hedgehog".to_string()),
                avatar: None,
                actor_id: "https://lemmy.example.com/u/hedgehog".to_string(),
            },
        );
        self.passwords.insert("hedgehog".to_string(), "testpass123".to_string());

        // Communities
        let communities = vec![
            Community {
                id: 1,
                name: "rust".to_string(),
                title: "Rust Programming".to_string(),
                description: Some("All things Rust".to_string()),
                icon: None,
                actor_id: "https://lemmy.example.com/c/rust".to_string(),
                subscribed: true,
            },
            Community {
                id: 2,
                name: "programming".to_string(),
                title: "Programming".to_string(),
                description: Some("General programming discussion".to_string()),
                icon: None,
                actor_id: "https://lemmy.example.com/c/programming".to_string(),
                subscribed: true,
            },
        ];
        for c in communities {
            self.communities.insert(c.id.to_string(), c);
        }

        // Posts for community 1 (rust)
        self.posts.insert(
            "1".to_string(),
            vec![
                Post {
                    id: 1,
                    name: "Rust 2025 edition is here".to_string(),
                    body: Some("The new Rust edition brings exciting features.".to_string()),
                    url: None,
                    creator_id: 1,
                    creator_name: "testuser".to_string(),
                    community_id: 1,
                    published: "2025-01-01T00:00:00Z".to_string(),
                    score: 42,
                    comment_count: 7,
                },
                Post {
                    id: 2,
                    name: "Async traits stabilized in Rust".to_string(),
                    body: None,
                    url: Some("https://blog.rust-lang.org/async-traits".to_string()),
                    creator_id: 1,
                    creator_name: "testuser".to_string(),
                    community_id: 1,
                    published: "2025-01-02T00:00:00Z".to_string(),
                    score: 128,
                    comment_count: 23,
                },
            ],
        );

        // Posts for community 2 (programming)
        self.posts.insert(
            "2".to_string(),
            vec![Post {
                id: 3,
                name: "The best programming languages of 2025".to_string(),
                body: Some("A survey of popular languages.".to_string()),
                url: None,
                creator_id: 1,
                creator_name: "testuser".to_string(),
                community_id: 2,
                published: "2025-01-03T00:00:00Z".to_string(),
                score: 19,
                comment_count: 5,
            }],
        );
    }

    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.communities.clear();
        self.posts.clear();
        self.passwords.clear();
    }
}

impl Default for LemmyState {
    fn default() -> Self {
        Self::new()
    }
}
