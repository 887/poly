//! High-level operations — all business logic on top of the storage layer.
//!
//! Each public function here is called by both the MCP handler and CLI handler.
//! All results are plain strings (markdown for multi-line, single-line for simple).

use std::path::Path;

use crate::store;
use crate::types::{ChecklistItem, Task, TaskStatus};

// ─── Task operations ──────────────────────────────────────────────────────────

/// Create a new task and return a confirmation string.
///
/// The next task ID is read from `poly-memory.json` (or derived from existing
/// task files on first use). After creation the counter is bumped atomically.
pub async fn create_task(
    data_dir: &Path,
    title: &str,
    description: Option<&str>,
) -> anyhow::Result<String> {
    let next_id = store::load_next_id(data_dir).await?;
    let task = Task::new(next_id, title, description);
    let line = task.summary_line();
    store::save_task(data_dir, &task).await?;
    store::save_next_id(data_dir, next_id.saturating_add(1)).await?;
    Ok(format!("✅ Created task: {line}"))
}

/// Return a formatted markdown list of all tasks.
pub async fn list_tasks(data_dir: &Path) -> anyhow::Result<String> {
    let tasks = store::load_tasks(data_dir).await?;
    if tasks.is_empty() {
        return Ok(
            "No tasks yet. Create one with: tasks create \"My task title\"\n\
             \n⚠️  AGENT REMINDER: Use `task_start_reminders` before starting any task."
                .to_string(),
        );
    }
    let mut lines = vec!["# Task List\n".to_string()];
    for task in &tasks {
        let memories = format!("💾{}m {}f", task.memory_count, task.finding_count);
        lines.push(format!("{} [{memories}]", task.summary_line()));
    }
    lines.push(String::new());
    lines.push(
        "Legend: ⬜todo 🔵in-progress ✅completed 🔄redo | \
         💾<n>m=memories <n>f=findings"
            .to_string(),
    );
    lines.push(String::new());
    lines.push(
        "⚠️  AGENT RULES:\
         \n• Call `task_start_reminders <id>` before starting any task.\
         \n• Store findings immediately as you discover them (prevents data loss on crash).\
         \n• Check off items as you complete them."
            .to_string(),
    );
    Ok(lines.join("\n"))
}

/// Find a task by numeric ID or by name/slug (partial match).
pub fn find_task<'a>(tasks: &'a [Task], id_or_name: &str) -> Option<&'a Task> {
    // Try numeric ID first.
    if let Ok(id) = id_or_name.parse::<u32>() {
        return tasks.iter().find(|t| t.id == id);
    }
    // Exact title match.
    if let Some(t) = tasks
        .iter()
        .find(|t| t.title.eq_ignore_ascii_case(id_or_name))
    {
        return Some(t);
    }
    // Slug match.
    let target_slug = crate::types::slugify(id_or_name);
    tasks.iter().find(|t| t.slug.contains(&target_slug))
}

/// Find a task (mutable) by numeric ID or name.
pub fn find_task_mut<'a>(tasks: &'a mut [Task], id_or_name: &str) -> Option<&'a mut Task> {
    // Try numeric ID first.
    if let Ok(id) = id_or_name.parse::<u32>() {
        return tasks.iter_mut().find(|t| t.id == id);
    }
    // Exact title match.
    if let Some(pos) = tasks
        .iter()
        .position(|t| t.title.eq_ignore_ascii_case(id_or_name))
    {
        return tasks.get_mut(pos);
    }
    // Slug match.
    let target_slug = crate::types::slugify(id_or_name);
    let pos = tasks.iter().position(|t| t.slug.contains(&target_slug))?;
    tasks.get_mut(pos)
}

/// Get detailed info about a task, including checklist and memory/finding counts.
pub async fn get_task(data_dir: &Path, id_or_name: &str) -> anyhow::Result<String> {
    let tasks = store::load_tasks(data_dir).await?;
    let task = find_task(&tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?;
    let mut lines = vec![
        format!("# Task [{}] {}", task.id, task.title),
        format!("**Status:** {} {}", task.status.emoji(), task.status),
        format!(
            "**Created:** {}",
            task.created_at.format("%Y-%m-%d %H:%M UTC")
        ),
    ];
    if let Some(desc) = &task.description {
        lines.push(format!("**Description:** {desc}"));
    }
    lines.push(format!(
        "**Memories:** {} | **Findings:** {}",
        task.memory_count, task.finding_count
    ));
    lines.push(String::new());
    lines.push("## Checklist".to_string());
    if task.checklist.is_empty() {
        lines.push("*(no items)*".to_string());
    } else {
        for item in &task.checklist {
            let tick = if item.done { "✅" } else { "⬜" };
            lines.push(format!("{tick} [{}] {}", item.id, item.text));
        }
    }
    Ok(lines.join("\n"))
}

/// Change the status of a task.
pub async fn set_task_status(
    data_dir: &Path,
    id_or_name: &str,
    new_status: TaskStatus,
) -> anyhow::Result<String> {
    let mut tasks = store::load_tasks(data_dir).await?;
    let task = find_task_mut(&mut tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?;
    let old = task.status.clone();
    task.status = new_status.clone();
    task.updated_at = chrono::Utc::now();
    if new_status == TaskStatus::Completed {
        task.completed_at = Some(chrono::Utc::now());
    }
    let title = task.title.clone();
    store::save_task(data_dir, task).await?;
    Ok(format!(
        "Task '{title}': {old} → {new_status} {}",
        new_status.emoji()
    ))
}

/// Reset a task for redo: status→Todo, uncheck all checklist items. Keeps memories.
pub async fn redo_task(data_dir: &Path, id_or_name: &str) -> anyhow::Result<String> {
    let mut tasks = store::load_tasks(data_dir).await?;
    let task = find_task_mut(&mut tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?;
    task.status = TaskStatus::Todo;
    task.completed_at = None;
    task.updated_at = chrono::Utc::now();
    // Uncheck all items but keep them.
    for item in &mut task.checklist {
        item.done = false;
    }
    let title = task.title.clone();
    store::save_task(data_dir, task).await?;
    Ok(format!(
        "🔄 Task '{title}' reset for redo. \
         Status → todo, all checklist items unchecked. \
         Memories and findings retained."
    ))
}

/// Add an item to a task's checklist.
pub async fn add_task_item(
    data_dir: &Path,
    id_or_name: &str,
    text: &str,
) -> anyhow::Result<String> {
    let mut tasks = store::load_tasks(data_dir).await?;
    let task = find_task_mut(&mut tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?;
    let next_item_id = task
        .checklist
        .iter()
        .map(|i| i.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let item = ChecklistItem {
        id: next_item_id,
        text: text.to_string(),
        done: false,
    };
    task.checklist.push(item);
    task.updated_at = chrono::Utc::now();
    let title = task.title.clone();
    store::save_task(data_dir, task).await?;
    Ok(format!(
        "⬜ Added item [{next_item_id}] to '{title}': {text}"
    ))
}

/// Mark a checklist item as done (or undo if already done).
pub async fn check_task_item(
    data_dir: &Path,
    task_id_or_name: &str,
    item_id: u32,
) -> anyhow::Result<String> {
    let mut tasks = store::load_tasks(data_dir).await?;
    let task = find_task_mut(&mut tasks, task_id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {task_id_or_name}"))?;
    let item = task
        .checklist
        .iter_mut()
        .find(|i| i.id == item_id)
        .ok_or_else(|| anyhow::anyhow!("Item {item_id} not found in task"))?;
    item.done = !item.done;
    let tick = if item.done { "✅" } else { "⬜" };
    let text = item.text.clone();
    task.updated_at = chrono::Utc::now();
    let task_title = task.title.clone();
    store::save_task(data_dir, task).await?;
    Ok(format!("{tick} Item [{item_id}] in '{task_title}': {text}"))
}

// ─── Memory + Finding operations ───────────────────────────────────────────────

/// Store a memory for a task. Returns a confirmation with file path.
pub async fn store_memory(
    data_dir: &Path,
    id_or_name: &str,
    title: &str,
    content: &str,
) -> anyhow::Result<String> {
    let mut tasks = store::load_tasks(data_dir).await?;
    let task = find_task(&tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?
        .clone();
    let path = store::store_memory(data_dir, &task, title, content).await?;
    let task_mut = find_task_mut(&mut tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?;
    task_mut.memory_count = task_mut.memory_count.saturating_add(1);
    task_mut.updated_at = chrono::Utc::now();
    store::save_task(data_dir, task_mut).await?;
    Ok(format!(
        "💾 Memory stored for task '{}' → {}",
        task.title,
        path.display()
    ))
}

/// Store a research finding for a task (appended to findings.md).
pub async fn store_finding(
    data_dir: &Path,
    id_or_name: &str,
    content: &str,
) -> anyhow::Result<String> {
    let mut tasks = store::load_tasks(data_dir).await?;
    let task = find_task(&tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?
        .clone();
    store::append_finding(data_dir, &task, content).await?;
    let task_title = task.title.clone();
    // Mutate and save only the affected task (no need to rewrite all task files).
    let task_mut = find_task_mut(&mut tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?;
    task_mut.finding_count = task_mut.finding_count.saturating_add(1);
    task_mut.updated_at = chrono::Utc::now();
    let count = task_mut.finding_count;
    store::save_task(data_dir, task_mut).await?;
    Ok(format!(
        "🔍 Finding #{count} stored for task '{task_title}'.\n\
         ⚠️  AGENT: Continue storing findings as you research — prevents data loss on crash."
    ))
}

/// Load all memories for a task.
pub async fn load_memories(data_dir: &Path, id_or_name: &str) -> anyhow::Result<String> {
    let tasks = store::load_tasks(data_dir).await?;
    let task = find_task(&tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?;
    let memories = store::list_memories(data_dir, task).await?;
    if memories.is_empty() {
        return Ok(format!("No memories stored for task '{}'.", task.title));
    }
    let mut out = vec![format!(
        "# Memories for Task [{}] {}\n({} files)\n",
        task.id,
        task.title,
        memories.len()
    )];
    for (name, content) in &memories {
        out.push(format!("---\n**File:** {name}\n\n{content}"));
    }
    Ok(out.join("\n"))
}

/// Load all findings for a task.
pub async fn load_findings(data_dir: &Path, id_or_name: &str) -> anyhow::Result<String> {
    let tasks = store::load_tasks(data_dir).await?;
    let task = find_task(&tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?;
    let findings = store::load_findings(data_dir, task).await?;
    if findings.is_empty() {
        return Ok(format!(
            "No findings stored for task '{}' yet.\n\
             Use `store_finding` to save research findings before they are lost.",
            task.title
        ));
    }
    Ok(format!(
        "# Findings for Task [{}] {}\n({} finding entries)\n\n{}",
        task.id, task.title, task.finding_count, findings
    ))
}

// ─── Workflow operations ───────────────────────────────────────────────────────

/// Return the next incomplete task (todo or redo), if any.
pub async fn next_task(data_dir: &Path) -> anyhow::Result<String> {
    let tasks = store::load_tasks(data_dir).await?;
    let next = tasks
        .iter()
        .find(|t| t.status == TaskStatus::Todo || t.status == TaskStatus::Redo);
    match next {
        None => Ok("✅ All tasks are completed! No pending tasks.".to_string()),
        Some(task) => Ok(format!(
            "Next task: {}\n\nCall `task_start_reminders {}` before starting.",
            task.summary_line(),
            task.id
        )),
    }
}

/// Return reminders and context for starting work on a specific task.
///
/// This should ALWAYS be called before working on a task.
pub async fn task_start_reminders(data_dir: &Path, id_or_name: &str) -> anyhow::Result<String> {
    let tasks = store::load_tasks(data_dir).await?;
    let task = find_task(&tasks, id_or_name)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {id_or_name}"))?;
    let mem_count = task.memory_count;
    let finding_count = task.finding_count;
    let task_id = task.id;
    let task_title = task.title.clone();
    let checklist_md = format_checklist(task);
    drop(tasks);
    Ok(format!(
        "# 🚀 Starting Task [{task_id}]: {task_title}\n\
         \n\
         ## Existing context\n\
         - **{mem_count}** memory note(s) already stored\n\
         - **{finding_count}** research finding(s) already stored\n\
         \n\
         {}\
         \n\
         ## ⚠️  MANDATORY AGENT RULES for this task:\n\
         1. **Read existing memories first**: call `load_memories {task_id}` now.\n\
         2. **Read existing findings first**: call `load_findings {task_id}` now.\n\
         3. **Store findings IMMEDIATELY** when you discover something important.\n\
            Do NOT wait until the end — if the session crashes, findings are lost.\n\
         4. **Store intermediate progress** as memories every few steps.\n\
         5. **Check off items** as you complete them: `check_task_item {task_id} <item_id>`.\n\
         6. **Follow agents.md** rules: run `cargo cranky --workspace` before declaring done.\n\
         7. **Mark task complete** when done: `set_task_status {task_id} completed`.\n\
         \n\
         Start by loading memories and findings, then begin work.",
        checklist_md
    ))
}

/// Build a work plan for N tasks.
pub async fn work_plan(data_dir: &Path, count: usize) -> anyhow::Result<String> {
    let tasks = store::load_tasks(data_dir).await?;
    let pending: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Todo || t.status == TaskStatus::Redo)
        .take(count)
        .collect();
    if pending.is_empty() {
        return Ok("✅ No pending tasks. All tasks are completed!\n\
             Create new tasks with: `tasks create \"My task title\"`"
            .to_string());
    }
    let mut lines = vec![format!(
        "# Work Plan — {} task(s)\n\
         \nWork on these tasks **one at a time, in order**:\n",
        pending.len()
    )];
    for (i, task) in pending.iter().enumerate() {
        lines.push(format!("{}. {}", i.saturating_add(1), task.summary_line()));
    }
    lines.push(String::new());
    lines.push(
        "## Procedure for each task:\n\
         1. Call `task_start_reminders <id>` — loads context + prints mandatory rules.\n\
         2. Call `load_memories <id>` — read previously stored memories.\n\
         3. Call `load_findings <id>` — read previously stored research findings.\n\
         4. Do the work (follow agents.md: cargo cranky, cargo check, etc.).\n\
         5. As you research: call `store_finding <id> \"...\"` — do NOT wait until the end.\n\
         6. As you implement: call `store_memory <id> \"title\" \"...\"` for key decisions.\n\
         7. Check off items: `check_task_item <id> <item_id>` as each is completed.\n\
         8. Mark done: `set_task_status <id> completed`.\n\
         9. Move to next task.\n\
         \n\
         ⚠️  Never skip step 1 (start reminders) or step 5 (immediate finding storage)."
            .to_string(),
    );
    Ok(lines.join("\n"))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Format the checklist portion of a task for display.
fn format_checklist(task: &Task) -> String {
    if task.checklist.is_empty() {
        return "## Checklist\n*(no items — add with `add_task_item`)*\n\n".to_string();
    }
    let done = task.checklist.iter().filter(|i| i.done).count();
    let total = task.checklist.len();
    let mut lines = vec![format!("## Checklist ({done}/{total} done)\n")];
    for item in &task.checklist {
        let tick = if item.done { "✅" } else { "⬜" };
        lines.push(format!("{tick} [{}] {}", item.id, item.text));
    }
    lines.push(String::new());
    lines.join("\n")
}
