//! In-memory state for the mock GitHub API server.

use base64::Engine as _;
use dashmap::DashMap;
use poly_test_common::AuthState;

/// All mock GitHub state: users, repos, issues, comments, contents.
pub struct GitHubState {
    pub auth: AuthState,
    pub users: DashMap<String, User>,
    /// owner/repo → Repo
    pub repos: DashMap<String, Repo>,
    /// repo full_name → Vec<Issue>
    pub issues: DashMap<String, Vec<Issue>>,
    /// "repo_full_name/issue_number" → Vec<Comment>
    pub comments: DashMap<String, Vec<Comment>>,
    /// repo full_name → Vec<ContentEntry> (root dir listing)
    pub contents: DashMap<String, Vec<ContentEntry>>,
    /// "repo_full_name/subdir_path" → Vec<ContentEntry> (subdir listings)
    pub subdir_contents: DashMap<String, Vec<ContentEntry>>,
    /// "repo_full_name/path" → ContentEntry (file content)
    pub file_contents: DashMap<String, ContentEntry>,
    /// "{user}/{owner}/{repo}" keys for starred repos
    pub starred: dashmap::DashSet<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: i64,
    pub login: String,
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
    pub pushed_at: Option<String>,
    pub default_branch: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub stargazers_count: u64,
    #[serde(default)]
    pub language: Option<String>,
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
    /// Total reaction count (thumbs up etc.) — surfaced in `reactions.total_count` JSON field.
    #[serde(default)]
    pub reactions_total: i64,
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

impl GitHubState {
    pub fn new() -> Self {
        Self {
            auth: AuthState::new(),
            users: DashMap::new(),
            repos: DashMap::new(),
            issues: DashMap::new(),
            comments: DashMap::new(),
            contents: DashMap::new(),
            subdir_contents: DashMap::new(),
            file_contents: DashMap::new(),
            starred: dashmap::DashSet::new(),
        }
    }

    /// Seed demo data (idempotent).
    pub fn seed(&self) {
        if !self.users.is_empty() {
            return;
        }

        // --- Users ---
        let penguin = User {
            id: 1,
            login: "penguin".to_string(),
            avatar_url: "https://github.com/penguin.png".to_string(),
        };
        let chameleon = User {
            id: 2,
            login: "chameleon".to_string(),
            avatar_url: "https://github.com/chameleon.png".to_string(),
        };

        self.users.insert("penguin".to_string(), penguin.clone());
        self.users.insert("chameleon".to_string(), chameleon.clone());

        // --- Repos (owned by penguin) ---
        let iceberg_os = Repo {
            id: 101,
            full_name: "penguin/iceberg-os".to_string(),
            name: "iceberg-os".to_string(),
            description: Some(
                "An operating system designed for extremely cold environments".to_string(),
            ),
            owner: penguin.clone(),
            private: false,
            archived: false,
            pushed_at: Some("2026-04-01T00:00:00Z".to_string()),
            default_branch: Some("main".to_string()),
            html_url: "https://github.com/penguin/iceberg-os".to_string(),
            stargazers_count: 42,
            language: Some("Rust".to_string()),
        };
        let fish_tracker = Repo {
            id: 102,
            full_name: "penguin/fish-tracker".to_string(),
            name: "fish-tracker".to_string(),
            description: Some(
                "GPS tracking system for Antarctic fish populations".to_string(),
            ),
            owner: penguin.clone(),
            private: false,
            archived: false,
            pushed_at: Some("2026-04-01T00:00:00Z".to_string()),
            default_branch: Some("main".to_string()),
            html_url: "https://github.com/penguin/fish-tracker".to_string(),
            stargazers_count: 7,
            language: Some("Python".to_string()),
        };

        // --- Repos (owned by chameleon) ---
        let color_shift = Repo {
            id: 103,
            full_name: "chameleon/color-shift".to_string(),
            name: "color-shift".to_string(),
            description: Some(
                "Dynamic color palette generator inspired by nature".to_string(),
            ),
            owner: chameleon.clone(),
            private: false,
            archived: false,
            pushed_at: Some("2026-04-01T00:00:00Z".to_string()),
            default_branch: Some("main".to_string()),
            html_url: "https://github.com/chameleon/color-shift".to_string(),
            stargazers_count: 128,
            language: Some("TypeScript".to_string()),
        };

        self.repos
            .insert("penguin/iceberg-os".to_string(), iceberg_os);
        self.repos
            .insert("penguin/fish-tracker".to_string(), fish_tracker);
        self.repos
            .insert("chameleon/color-shift".to_string(), color_shift);

        // --- Issues for penguin/iceberg-os ---
        let iceberg_issues = vec![
            Issue {
                id: 1001,
                number: 1,
                title: "Add thermal regulation module".to_string(),
                body: Some(
                    "We need better heat management for the kernel".to_string(),
                ),
                user: penguin.clone(),
                state: "open".to_string(),
                created_at: "2026-03-01T10:00:00Z".to_string(),
                updated_at: "2026-03-03T10:00:00Z".to_string(),
                html_url: "https://github.com/penguin/iceberg-os/issues/1".to_string(),
                comments: 2,
                pull_request: None,
                reactions_total: 5,
            },
            Issue {
                id: 1002,
                number: 2,
                title: "Memory leak in snowflake allocator".to_string(),
                body: Some(
                    "The snowflake allocator leaks under pressure".to_string(),
                ),
                user: chameleon.clone(),
                state: "open".to_string(),
                created_at: "2026-03-02T11:00:00Z".to_string(),
                updated_at: "2026-03-02T11:00:00Z".to_string(),
                html_url: "https://github.com/penguin/iceberg-os/issues/2".to_string(),
                comments: 1,
                pull_request: None,
                reactions_total: 3,
            },
            Issue {
                id: 1003,
                number: 3,
                title: "Implement ice crystal caching".to_string(),
                body: Some(
                    "This PR adds caching based on ice crystal patterns".to_string(),
                ),
                user: chameleon.clone(),
                state: "open".to_string(),
                created_at: "2026-03-04T09:00:00Z".to_string(),
                updated_at: "2026-03-04T09:00:00Z".to_string(),
                html_url: "https://github.com/penguin/iceberg-os/pull/3".to_string(),
                comments: 0,
                pull_request: Some(serde_json::json!({})),
                reactions_total: 1,
            },
        ];
        self.issues
            .insert("penguin/iceberg-os".to_string(), iceberg_issues);

        // --- Issues for chameleon/color-shift ---
        let color_issues = vec![Issue {
            id: 2001,
            number: 1,
            title: "Support UV spectrum colors".to_string(),
            body: Some(
                "Chameleons can see UV — we should support it".to_string(),
            ),
            user: penguin.clone(),
            state: "open".to_string(),
            created_at: "2026-03-06T14:00:00Z".to_string(),
            updated_at: "2026-03-06T14:00:00Z".to_string(),
            html_url: "https://github.com/chameleon/color-shift/issues/1".to_string(),
            comments: 0,
            pull_request: None,
            reactions_total: 0,
        }];

        // --- Seed a starred repo for penguin: penguin/iceberg-os ---
        self.starred
            .insert("penguin/penguin/iceberg-os".to_string());
        self.issues
            .insert("chameleon/color-shift".to_string(), color_issues);

        // --- Comments for penguin/iceberg-os issue #1 ---
        let iceberg1_comments = vec![
            Comment {
                id: 5001,
                user: chameleon.clone(),
                body: "Great idea! The thermal module should integrate with the cooling subsystem."
                    .to_string(),
                created_at: "2026-03-02T09:00:00Z".to_string(),
                html_url: "https://github.com/penguin/iceberg-os/issues/1#issuecomment-5001"
                    .to_string(),
            },
            Comment {
                id: 5002,
                user: penguin.clone(),
                body: "I'll start with a prototype in the next sprint.".to_string(),
                created_at: "2026-03-03T10:00:00Z".to_string(),
                html_url: "https://github.com/penguin/iceberg-os/issues/1#issuecomment-5002"
                    .to_string(),
            },
        ];
        self.comments
            .insert("penguin/iceberg-os/1".to_string(), iceberg1_comments);

        // --- Comments for penguin/iceberg-os issue #2 ---
        let iceberg2_comments = vec![Comment {
            id: 5003,
            user: penguin.clone(),
            body: "I can reproduce this consistently under heavy load.".to_string(),
            created_at: "2026-03-02T12:00:00Z".to_string(),
            html_url: "https://github.com/penguin/iceberg-os/issues/2#issuecomment-5003"
                .to_string(),
        }];
        self.comments
            .insert("penguin/iceberg-os/2".to_string(), iceberg2_comments);

        // --- Contents for penguin/iceberg-os (root listing) ---
        let root_listing = vec![
            ContentEntry {
                name: "README.md".to_string(),
                path: "README.md".to_string(),
                kind: "file".to_string(),
                size: 312,
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
                size: 198,
                content: None,
                encoding: None,
            },
            ContentEntry {
                name: "LICENSE".to_string(),
                path: "LICENSE".to_string(),
                kind: "file".to_string(),
                size: 1065,
                content: None,
                encoding: None,
            },
        ];
        self.contents
            .insert("penguin/iceberg-os".to_string(), root_listing);

        // --- Contents for penguin/iceberg-os/src (subdir listing) ---
        let src_listing = vec![
            ContentEntry {
                name: "main.rs".to_string(),
                path: "src/main.rs".to_string(),
                kind: "file".to_string(),
                size: 89,
                content: None,
                encoding: None,
            },
            ContentEntry {
                name: "thermal.rs".to_string(),
                path: "src/thermal.rs".to_string(),
                kind: "file".to_string(),
                size: 156,
                content: None,
                encoding: None,
            },
        ];
        self.subdir_contents
            .insert("penguin/iceberg-os/src".to_string(), src_listing);

        // --- File content for penguin/iceberg-os/README.md ---
        let readme_text = "# Iceberg OS\n\nAn operating system designed for extremely cold environments.\n\n## Features\n\n- Sub-zero thermal management\n- Ice crystal pattern caching\n- Snowflake memory allocator\n";
        let readme_b64 =
            base64::engine::general_purpose::STANDARD.encode(readme_text);
        self.file_contents.insert(
            "penguin/iceberg-os/README.md".to_string(),
            ContentEntry {
                name: "README.md".to_string(),
                path: "README.md".to_string(),
                kind: "file".to_string(),
                size: readme_text.len() as u64,
                content: Some(readme_b64),
                encoding: Some("base64".to_string()),
            },
        );

        // --- File content for penguin/iceberg-os/src/main.rs ---
        let main_text = "fn main() {\n    println!(\"Welcome to Iceberg OS!\");\n}\n";
        let main_b64 = base64::engine::general_purpose::STANDARD.encode(main_text);
        self.file_contents.insert(
            "penguin/iceberg-os/src/main.rs".to_string(),
            ContentEntry {
                name: "main.rs".to_string(),
                path: "src/main.rs".to_string(),
                kind: "file".to_string(),
                size: main_text.len() as u64,
                content: Some(main_b64),
                encoding: Some("base64".to_string()),
            },
        );
    }

    pub fn reset(&self) {
        self.auth.clear();
        self.users.clear();
        self.repos.clear();
        self.issues.clear();
        self.comments.clear();
        self.contents.clear();
        self.subdir_contents.clear();
        self.file_contents.clear();
        self.starred.clear();
    }
}

impl Default for GitHubState {
    fn default() -> Self {
        Self::new()
    }
}
