//! Core data types: tasks, checklist items, statuses.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── TaskStatus ───────────────────────────────────────────────────────────────

/// Lifecycle status of a task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    /// Not yet started.
    Todo,
    /// Currently being worked on.
    InProgress,
    /// All checklist items done, task resolved.
    Completed,
    /// Marked for redo — checklist items reset but memories preserved.
    Redo,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Todo => write!(f, "todo"),
            Self::InProgress => write!(f, "in-progress"),
            Self::Completed => write!(f, "completed"),
            Self::Redo => write!(f, "redo"),
        }
    }
}

impl TaskStatus {
    /// Visual emoji indicator for this status.
    pub const fn emoji(&self) -> &'static str {
        match self {
            Self::Todo => "⬜",
            Self::InProgress => "🔵",
            Self::Completed => "✅",
            Self::Redo => "🔄",
        }
    }

    /// Parse a status string (case-insensitive).
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        match s.to_lowercase().as_str() {
            "todo" => Ok(Self::Todo),
            "in-progress" | "inprogress" | "wip" => Ok(Self::InProgress),
            "completed" | "done" | "complete" => Ok(Self::Completed),
            "redo" => Ok(Self::Redo),
            other => {
                anyhow::bail!("Unknown status '{other}'. Use: todo, in-progress, completed, redo")
            }
        }
    }
}

// ─── ChecklistItem ────────────────────────────────────────────────────────────

/// A single item in a task's checklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    /// Sequential ID within the task (0-based, stable after creation).
    pub id: u32,
    /// The item description.
    pub text: String,
    /// Whether this item has been completed.
    pub done: bool,
}

// ─── Task ─────────────────────────────────────────────────────────────────────

/// A numbered task with metadata, checklist, and associated memory files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Globally unique auto-incrementing ID.
    pub id: u32,
    /// URL-safe slug derived from the title.
    pub slug: String,
    /// Human-readable task title.
    pub title: String,
    /// Optional longer description.
    pub description: Option<String>,
    /// Current lifecycle status.
    pub status: TaskStatus,
    /// When this task was first created.
    pub created_at: DateTime<Utc>,
    /// When this task was last modified.
    pub updated_at: DateTime<Utc>,
    /// When this task transitioned to Completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Number of memory files stored for this task.
    pub memory_count: u32,
    /// Number of findings entries stored for this task.
    pub finding_count: u32,
    /// Ordered checklist items.
    pub checklist: Vec<ChecklistItem>,
}

impl Task {
    /// Create a new task with the next available ID.
    pub fn new(id: u32, title: &str, description: Option<&str>) -> Self {
        let now = Utc::now();
        Self {
            id,
            slug: slugify(title),
            title: title.to_string(),
            description: description.map(str::to_string),
            status: TaskStatus::Todo,
            created_at: now,
            updated_at: now,
            completed_at: None,
            memory_count: 0,
            finding_count: 0,
            checklist: Vec::new(),
        }
    }

    /// Directory name for this task's findings/memories subdirectory.
    ///
    /// Uses dash-separated slug for backward compatibility with existing on-disk directories.
    /// e.g. `"001-add-cli-to-mcp"`
    pub fn dir_name(&self) -> String {
        format!("{:03}-{}", self.id, self.slug)
    }

    /// Base name for this task's individual JSON file (and its subdir alias).
    ///
    /// Format: `{id:03}_{slug_with_underscores}`, capped at 50 characters total.
    ///
    /// Examples:
    /// - `001_implement_poly_server_client_backend`
    /// - `002_e2e_poly_server_via_poly_web_visual_test`
    pub fn file_name(&self) -> String {
        let prefix = format!("{:03}_", self.id);
        let max_slug_len = 50_usize.saturating_sub(prefix.len());
        let slug = self.slug.replace('-', "_");
        let slug = if slug.len() > max_slug_len {
            // Truncate then strip any dangling trailing underscore.
            slug.get(..max_slug_len)
                .unwrap_or(&slug)
                .trim_end_matches('_')
                .to_string()
        } else {
            slug
        };
        format!("{prefix}{slug}")
    }

    /// One-line status summary: `"✅ [3] My Task (2/4 items)"`.
    pub fn summary_line(&self) -> String {
        let done = self.checklist.iter().filter(|i| i.done).count();
        let total = self.checklist.len();
        let check = if total > 0 {
            format!(" ({done}/{total} items)")
        } else {
            String::new()
        };
        format!(
            "{} [{}] {}{}",
            self.status.emoji(),
            self.id,
            self.title,
            check
        )
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a title to a URL-safe slug by lowercasing and replacing non-alphanum
/// characters with hyphens.
pub fn slugify(title: &str) -> String {
    let raw: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    raw.split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}
