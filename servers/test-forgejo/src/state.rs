//! In-memory state for the mock Forgejo server.

use base64::Engine as _;
use dashmap::DashMap;
use poly_test_common::{AuthState, HeaderInspectBuffer};
use std::sync::Arc;

/// All mock Forgejo state: users, repos, issues, comments, contents.
pub struct ForgejoState {
    pub auth: AuthState,
    pub users: DashMap<String, User>,
    pub passwords: DashMap<String, String>,
    /// owner/repo → Repo
    pub repos: DashMap<String, Repo>,
    /// repo full_name → Vec<Issue>
    pub issues: DashMap<String, Vec<Issue>>,
    /// "repo_full_name/issue_number" → Vec<Comment>
    pub comments: DashMap<String, Vec<Comment>>,
    /// repo full_name → Vec<ContentEntry> (root dir listing)
    pub contents: DashMap<String, Vec<ContentEntry>>,
    /// "repo_full_name/path" → ContentEntry (file content)
    pub file_contents: DashMap<String, ContentEntry>,
    /// username → Set of starred repo full_names
    pub starred: DashMap<String, std::collections::HashSet<String>>,
    /// Ring buffer of recent inbound request headers (Phase E inspection endpoint).
    pub inspect: Arc<HeaderInspectBuffer>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: i64,
    pub login: String,
    pub full_name: String,
    pub avatar_url: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Repo {
    pub id: i64,
    pub full_name: String,
    pub name: String,
    pub description: Option<String>,
    pub owner: User,
    pub private: bool,
    pub archived: bool,
    pub updated_at: String,
    pub default_branch: String,
    pub html_url: String,
    #[serde(default)]
    pub stars_count: u64,
    #[serde(default)]
    pub forks_count: u64,
    #[serde(default)]
    pub open_issues_count: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Issue {
    pub id: i64,
    pub number: i64,
    pub title: String,
    pub body: Option<String>,
    pub user: User,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub html_url: String,
    pub comments: i32,
    pub pull_request: Option<serde_json::Value>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Comment {
    pub id: i64,
    pub user: User,
    pub body: String,
    pub created_at: String,
    pub html_url: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ContentEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
}

impl ForgejoState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            passwords: DashMap::new(),
            repos: DashMap::new(),
            issues: DashMap::new(),
            comments: DashMap::new(),
            contents: DashMap::new(),
            file_contents: DashMap::new(),
            starred: DashMap::new(),
            inspect: Arc::new(HeaderInspectBuffer::new()),
        }
    }

    /// Seed demo data (idempotent).
    pub fn seed(&self) {
        if !self.users.is_empty() {
            return;
        }

        // --- Users ---
        let otter = User {
            id: 1,
            login: "otter".to_string(),
            full_name: "Otter".to_string(),
            avatar_url: "http://localhost:9106/avatars/otter".to_string(),
        };
        let flamingo = User {
            id: 2,
            login: "flamingo".to_string(),
            full_name: "Flamingo".to_string(),
            avatar_url: "http://localhost:9106/avatars/flamingo".to_string(),
        };
        let testuser = User {
            id: 3,
            login: "testuser".to_string(),
            full_name: "Test User".to_string(),
            // axolotl for cross-backend recognition: same asset as Lemmy's testuser
            avatar_url: "http://localhost:9106/avatars/axolotl".to_string(),
        };

        self.users.insert("otter".to_string(), otter.clone());
        self.users.insert("flamingo".to_string(), flamingo.clone());
        self.users.insert("testuser".to_string(), testuser);

        self.passwords.insert("otter".to_string(), "testpass123".to_string());
        self.passwords.insert("flamingo".to_string(), "testpass123".to_string());
        self.passwords.insert("testuser".to_string(), "testpass123".to_string());

        // --- Repos ---
        let dam_builder = Repo {
            id: 1,
            full_name: "otter/dam-builder".to_string(),
            name: "dam-builder".to_string(),
            description: Some("A structural engineering toolkit for aquatic habitats".to_string()),
            owner: otter.clone(),
            private: false,
            archived: false,
            updated_at: "2026-01-15T12:00:00Z".to_string(),
            default_branch: "main".to_string(),
            html_url: "https://forgejo.example.com/otter/dam-builder".to_string(),
            stars_count: 42,
            forks_count: 7,
            open_issues_count: 2,
        };
        let fish_finder = Repo {
            id: 2,
            full_name: "otter/fish-finder".to_string(),
            name: "fish-finder".to_string(),
            description: Some("ML-powered fish detection for rivers and streams".to_string()),
            owner: otter.clone(),
            private: false,
            archived: false,
            updated_at: "2026-01-10T08:00:00Z".to_string(),
            default_branch: "main".to_string(),
            html_url: "https://forgejo.example.com/otter/fish-finder".to_string(),
            stars_count: 15,
            forks_count: 3,
            open_issues_count: 0,
        };
        let pink_css = Repo {
            id: 3,
            full_name: "flamingo/pink-css".to_string(),
            name: "pink-css".to_string(),
            description: Some("A pink-themed CSS framework".to_string()),
            owner: flamingo.clone(),
            private: false,
            archived: false,
            updated_at: "2026-01-05T16:00:00Z".to_string(),
            default_branch: "main".to_string(),
            html_url: "https://forgejo.example.com/flamingo/pink-css".to_string(),
            stars_count: 8,
            forks_count: 1,
            open_issues_count: 1,
        };

        // Test arena repo — shared between otter (owner) and flamingo (collaborator)
        // Used by back-and-forth tests for issue-comment chat
        let test_arena = Repo {
            id: 10,
            full_name: "otter/test-arena".to_string(),
            name: "test-arena".to_string(),
            description: Some("Shared test arena repo for back-and-forth integration tests".to_string()),
            owner: otter.clone(),
            private: false,
            archived: false,
            updated_at: "2026-04-01T00:00:00Z".to_string(),
            default_branch: "main".to_string(),
            html_url: "https://forgejo.example.com/otter/test-arena".to_string(),
            stars_count: 0,
            forks_count: 0,
            open_issues_count: 0,
        };
        self.repos.insert("otter/test-arena".to_string(), test_arena);
        // Empty issue list for the test arena repo
        self.issues.insert("otter/test-arena".to_string(), vec![]);

        self.repos.insert("otter/dam-builder".to_string(), dam_builder);
        self.repos.insert("otter/fish-finder".to_string(), fish_finder);
        self.repos.insert("flamingo/pink-css".to_string(), pink_css);

        // --- Issues for otter/dam-builder ---
        let dam_issues = vec![
            Issue {
                id: 101,
                number: 1,
                title: "Support curved dam designs".to_string(),
                body: Some("It would be great to support curved dam designs for better water flow.".to_string()),
                user: otter.clone(),
                state: "open".to_string(),
                created_at: "2026-01-01T10:00:00Z".to_string(),
                updated_at: "2026-01-03T10:00:00Z".to_string(),
                html_url: "https://forgejo.example.com/otter/dam-builder/issues/1".to_string(),
                comments: 2,
                pull_request: None,
            },
            Issue {
                id: 102,
                number: 2,
                title: "Water pressure calculations are off".to_string(),
                body: Some("The current calculations do not account for seasonal pressure variance.".to_string()),
                user: flamingo.clone(),
                state: "open".to_string(),
                created_at: "2026-01-02T11:00:00Z".to_string(),
                updated_at: "2026-01-02T11:00:00Z".to_string(),
                html_url: "https://forgejo.example.com/otter/dam-builder/issues/2".to_string(),
                comments: 1,
                pull_request: None,
            },
            Issue {
                id: 103,
                number: 3,
                title: "Add beaver collaboration mode".to_string(),
                body: Some("Beavers should be able to collaborate on dam designs in real time.".to_string()),
                user: flamingo.clone(),
                state: "open".to_string(),
                created_at: "2026-01-04T09:00:00Z".to_string(),
                updated_at: "2026-01-04T09:00:00Z".to_string(),
                html_url: "https://forgejo.example.com/otter/dam-builder/pulls/3".to_string(),
                comments: 0,
                pull_request: Some(serde_json::json!({})),
            },
        ];
        self.issues.insert("otter/dam-builder".to_string(), dam_issues);

        // --- Issues for flamingo/pink-css ---
        let pink_issues = vec![
            Issue {
                id: 201,
                number: 1,
                title: "Add hot pink variant".to_string(),
                body: Some("We need a hot pink (#FF69B4) variant for the button classes.".to_string()),
                user: otter.clone(),
                state: "open".to_string(),
                created_at: "2026-01-06T14:00:00Z".to_string(),
                updated_at: "2026-01-06T14:00:00Z".to_string(),
                html_url: "https://forgejo.example.com/flamingo/pink-css/issues/1".to_string(),
                comments: 0,
                pull_request: None,
            },
        ];
        self.issues.insert("flamingo/pink-css".to_string(), pink_issues);

        // --- Comments for otter/dam-builder issue #1 ---
        let dam1_comments = vec![
            Comment {
                id: 1001,
                user: flamingo.clone(),
                body: "I've been thinking about this too — curves would improve water flow".to_string(),
                created_at: "2026-01-02T09:00:00Z".to_string(),
                html_url: "https://forgejo.example.com/otter/dam-builder/issues/1#issuecomment-1001".to_string(),
            },
            Comment {
                id: 1002,
                user: otter.clone(),
                body: "Agreed, let me prototype something".to_string(),
                created_at: "2026-01-03T10:00:00Z".to_string(),
                html_url: "https://forgejo.example.com/otter/dam-builder/issues/1#issuecomment-1002".to_string(),
            },
        ];
        self.comments.insert("otter/dam-builder/1".to_string(), dam1_comments);

        // --- Comments for otter/dam-builder issue #2 ---
        let dam2_comments = vec![
            Comment {
                id: 1003,
                user: otter.clone(),
                body: "This is critical for dam safety".to_string(),
                created_at: "2026-01-02T12:00:00Z".to_string(),
                html_url: "https://forgejo.example.com/otter/dam-builder/issues/2#issuecomment-1003".to_string(),
            },
        ];
        self.comments.insert("otter/dam-builder/2".to_string(), dam2_comments);

        // --- Starred repos (otter has starred fish-finder but NOT dam-builder) ---
        let mut otter_starred = std::collections::HashSet::new();
        otter_starred.insert("otter/fish-finder".to_string());
        self.starred.insert("otter".to_string(), otter_starred);

        // --- Contents for otter/dam-builder (root listing) ---
        let root_listing = vec![
            ContentEntry {
                name: "README.md".to_string(),
                path: "README.md".to_string(),
                kind: "file".to_string(),
                size: 256,
                content: None,
                encoding: None,
            },
            ContentEntry {
                name: "src".to_string(),
                path: "src".to_string(),
                kind: "dir".to_string(),
                size: 0,
                content: None,
                encoding: None,
            },
            ContentEntry {
                name: "Cargo.toml".to_string(),
                path: "Cargo.toml".to_string(),
                kind: "file".to_string(),
                size: 128,
                content: None,
                encoding: None,
            },
        ];
        self.contents.insert("otter/dam-builder".to_string(), root_listing);

        // --- File content for otter/dam-builder/README.md ---
        let readme_text = "# Dam Builder\n\nA structural engineering toolkit for aquatic habitats.\n";
        let readme_b64 = base64::engine::general_purpose::STANDARD.encode(readme_text);
        let readme_entry = ContentEntry {
            name: "README.md".to_string(),
            path: "README.md".to_string(),
            kind: "file".to_string(),
            size: u64::try_from(readme_text.len()).unwrap_or(u64::MAX),
            content: Some(readme_b64),
            encoding: Some("base64".to_string()),
        };
        self.file_contents.insert("otter/dam-builder/README.md".to_string(), readme_entry);
    }

    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.passwords.clear();
        self.repos.clear();
        self.issues.clear();
        self.comments.clear();
        self.contents.clear();
        self.file_contents.clear();
        self.starred.clear();
        self.inspect.clear();
    }
}

impl Default for ForgejoState {
    fn default() -> Self {
        Self::new()
    }
}
