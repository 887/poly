//! File-system storage layer.
//!
//! All I/O is async via `tokio::fs`. Each function operates on a `data_dir`
//! root path and derives task/knowledge paths from it.
//!
//! No `unwrap`, no direct indexing — all errors bubble up via `?`.

use std::path::{Path, PathBuf};

use chrono::Utc;
use tokio::io::AsyncWriteExt as _;

use crate::types::Task;

// ─── Directory helpers ────────────────────────────────────────────────────────

/// Ensure a directory and all its parents exist.
async fn ensure_dir(path: &Path) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

/// Path to `tasks.json` in the data directory.
pub fn tasks_json_path(data_dir: &Path) -> PathBuf {
    data_dir.join("tasks.json")
}

/// Path to the task's subdirectory under `tasks/`.
pub fn task_dir(data_dir: &Path, task: &Task) -> PathBuf {
    data_dir.join("tasks").join(task.dir_name())
}

/// Path to `findings.md` inside a task directory.
pub fn findings_path(task_dir: &Path) -> PathBuf {
    task_dir.join("findings.md")
}

/// Path to the `memories/` subdirectory inside a task directory.
pub fn memories_dir(task_dir: &Path) -> PathBuf {
    task_dir.join("memories")
}

/// Path to the `knowledge/` subdirectory of the data dir.
pub fn knowledge_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("knowledge")
}

// ─── Tasks storage ────────────────────────────────────────────────────────────

/// Load all tasks from `tasks.json`. Returns empty vec if file missing.
pub async fn load_tasks(data_dir: &Path) -> anyhow::Result<Vec<Task>> {
    let path = tasks_json_path(data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = tokio::fs::read(&path).await?;
    let tasks: Vec<Task> = serde_json::from_slice(&bytes)?;
    Ok(tasks)
}

/// Persist the entire task list to `tasks.json`.
pub async fn save_tasks(data_dir: &Path, tasks: &[Task]) -> anyhow::Result<()> {
    ensure_dir(data_dir).await?;
    let path = tasks_json_path(data_dir);
    let json = serde_json::to_string_pretty(tasks)?;
    tokio::fs::write(&path, json.as_bytes()).await?;
    Ok(())
}

// ─── Findings storage ─────────────────────────────────────────────────────────

/// Load the full findings markdown for a task. Returns empty string if none.
pub async fn load_findings(data_dir: &Path, task: &Task) -> anyhow::Result<String> {
    let path = findings_path(&task_dir(data_dir, task));
    if !path.exists() {
        return Ok(String::new());
    }
    let bytes = tokio::fs::read(&path).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Append a new finding entry to `findings.md`.
///
/// Creates the file (and directories) if they don't exist.
pub async fn append_finding(
    data_dir: &Path,
    task: &Task,
    content: &str,
) -> anyhow::Result<()> {
    let tdir = task_dir(data_dir, task);
    ensure_dir(&tdir).await?;
    let path = findings_path(&tdir);
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let entry = format!("\n\n## Finding {timestamp}\n\n{content}\n\n---\n");
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;
    // Write header if file was just created (empty).
    let meta = file.metadata().await?;
    if meta.len() == 0 {
        let header = format!(
            "# Research Findings — Task: {}\n\n\
             *Auto-updated by poly-memory-mcp. Add findings via CLI or MCP tool.*\n\n---\n",
            task.title
        );
        file.write_all(header.as_bytes()).await?;
    }
    file.write_all(entry.as_bytes()).await?;
    Ok(())
}

// ─── Memory storage ───────────────────────────────────────────────────────────

/// Store a new memory note in the task's `memories/` directory.
///
/// Returns the path of the written file (relative to `data_dir`).
pub async fn store_memory(
    data_dir: &Path,
    task: &Task,
    title: &str,
    content: &str,
) -> anyhow::Result<PathBuf> {
    let mdir = memories_dir(&task_dir(data_dir, task));
    ensure_dir(&mdir).await?;
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let slug = crate::types::slugify(title);
    let filename = format!("{timestamp}-{slug}.md");
    let path = mdir.join(&filename);
    let stored_at = Utc::now().to_rfc3339();
    let body = format!(
        "# Memory: {title}\n\n*Stored: {stored_at}*\n\n---\n\n{content}\n"
    );
    tokio::fs::write(&path, body.as_bytes()).await?;
    Ok(path)
}

/// List all memory files for a task. Returns `(filename, content)` pairs.
pub async fn list_memories(data_dir: &Path, task: &Task) -> anyhow::Result<Vec<(String, String)>> {
    let mdir = memories_dir(&task_dir(data_dir, task));
    if !mdir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(&mdir).await?;
    let mut files: Vec<String> = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".md") {
            files.push(name);
        }
    }
    files.sort();
    let mut result = Vec::new();
    for name in files {
        let path = mdir.join(&name);
        let bytes = tokio::fs::read(&path).await?;
        result.push((name, String::from_utf8_lossy(&bytes).into_owned()));
    }
    Ok(result)
}

// ─── Knowledge base ───────────────────────────────────────────────────────────

/// Store or overwrite a knowledge entry at `knowledge/<slug>.md`.
pub async fn store_knowledge(
    data_dir: &Path,
    topic: &str,
    content: &str,
) -> anyhow::Result<PathBuf> {
    let kdir = knowledge_dir(data_dir);
    ensure_dir(&kdir).await?;
    let slug = crate::types::slugify(topic);
    let path = kdir.join(format!("{slug}.md"));
    let updated_at = Utc::now().to_rfc3339();
    let body = format!(
        "# Knowledge: {topic}\n\n*Last Updated: {updated_at}*\n\n---\n\n{content}\n"
    );
    tokio::fs::write(&path, body.as_bytes()).await?;
    Ok(path)
}

/// Search knowledge files for entries whose file name or content contains `query`.
///
/// Returns `(topic_slug, content)` pairs for all matches.
pub async fn search_knowledge(
    data_dir: &Path,
    query: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let kdir = knowledge_dir(data_dir);
    if !kdir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(&kdir).await?;
    let mut matches = Vec::new();
    let q_lower = query.to_lowercase();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") {
            continue;
        }
        let bytes = tokio::fs::read(entry.path()).await?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if name.to_lowercase().contains(&q_lower)
            || text.to_lowercase().contains(&q_lower)
        {
            matches.push((name.trim_end_matches(".md").to_string(), text));
        }
    }
    matches.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(matches)
}

/// List all knowledge topics.
pub async fn list_knowledge(data_dir: &Path) -> anyhow::Result<Vec<String>> {
    let kdir = knowledge_dir(data_dir);
    if !kdir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(&kdir).await?;
    let mut topics = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".md") {
            topics.push(name.trim_end_matches(".md").to_string());
        }
    }
    topics.sort();
    Ok(topics)
}

/// Load a specific knowledge entry by topic slug.
pub async fn load_knowledge(data_dir: &Path, topic: &str) -> anyhow::Result<Option<String>> {
    let slug = crate::types::slugify(topic);
    let path = knowledge_dir(data_dir).join(format!("{slug}.md"));
    if !path.exists() {
        return Ok(None);
    }
    let bytes = tokio::fs::read(&path).await?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}
