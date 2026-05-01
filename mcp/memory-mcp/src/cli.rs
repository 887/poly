//! CLI argument parsing and dispatch.
//!
//! Preferred over MCP access for shell scripts and VS Code tasks.
//!
//! All output goes via `write!` on `stdout()` — never `println!`.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::ops;
use crate::store;
use crate::types::TaskStatus;

/// Print the CLI help text to stdout.
pub async fn print_help() -> anyhow::Result<()> {
    let help = "\
poly-memory-mcp — Persistent task list + memory + knowledge-base for AI agents.

PREFERRED MODE: CLI (this mode) — faster, scriptable, no MCP overhead.

USAGE:
  poly-memory-mcp <SUBCOMMAND> [ARGS...]

TASK COMMANDS:
  tasks list                          List all tasks with status
  tasks create <title>                Create a new task
  tasks create <title> --desc <desc>  Create a task with description
  tasks get <id|name>                 Show full task details + checklist
  tasks status <id|name> <status>     Set task status (todo, in-progress, completed, redo)
  tasks redo <id|name>                Reset task for redo (uncheck all items, keep memories)
  tasks next                          Show the next pending task
  tasks reminders <id|name>           Show start-of-task reminders + mandatory rules
  tasks plan [--count N]              Work plan for N pending tasks (default: 3)
  tasks item add <id|name> <text>     Add a checklist item to a task
  tasks item check <id|name> <item>   Toggle a checklist item done/undone

MEMORY COMMANDS:
  memory store <id|name> <title> <content>   Store a memory note for a task
  memory list <id|name>                      List all memories for a task

FINDING COMMANDS (use these constantly during research!):
  finding store <id|name> <content>          Store a research finding (CALL OFTEN!)
  finding list <id|name>                     Show all findings for a task

KNOWLEDGE COMMANDS:
  knowledge store <topic> <content>          Store/update a knowledge entry
  knowledge search <query>                   Search knowledge base
  knowledge list                             List all knowledge topics
  knowledge get <topic>                      Show a specific knowledge entry

WORK COMMANDS (agent workflow):
  work                                       Plan for next 3 pending tasks
  work --count N                             Plan for next N pending tasks

DATA:
  Default dir: ~/.poly-memory/
  Override:    POLY_MEMORY_DIR=<path> poly-memory-mcp ...

";
    write_stdout(help)?;
    Ok(())
}

/// Dispatch CLI arguments to the appropriate operation.
pub async fn run(data_dir: PathBuf, args: &[String]) -> anyhow::Result<()> {
    let subcmd = args.first().map_or("", String::as_str);
    let rest = args.get(1..).unwrap_or(&[]);
    let result = match subcmd {
        "tasks" => dispatch_tasks(&data_dir, rest).await,
        "memory" => dispatch_memory(&data_dir, rest).await,
        "finding" => dispatch_finding(&data_dir, rest).await,
        "knowledge" => dispatch_knowledge(&data_dir, rest).await,
        "work" => dispatch_work(&data_dir, rest).await,
        "--help" | "-h" | "help" => {
            print_help().await?;
            return Ok(());
        }
        other => Err(anyhow::anyhow!(
            "Unknown subcommand: '{other}'. Run with --help for usage."
        )),
    };
    match result {
        Ok(msg) => {
            write_stdout_line(&msg)?;
        }
        Err(e) => {
            write_stderr_line(&format!("Error: {e}"))?;
            std::process::exit(1);
        }
    }
    Ok(())
}

// ─── Sub-dispatchers ──────────────────────────────────────────────────────────

async fn dispatch_tasks(data_dir: &Path, args: &[String]) -> anyhow::Result<String> {
    let sub = args.first().map_or("", String::as_str);
    let rest = args.get(1..).unwrap_or(&[]);
    match sub {
        "list" | "" => ops::list_tasks(data_dir).await,
        "create" => dispatch_task_create(data_dir, rest).await,
        "get" => {
            let id = require_arg(rest, 0, "tasks get <id|name>")?;
            ops::get_task(data_dir, id).await
        }
        "status" => dispatch_task_status(data_dir, rest).await,
        "redo" => {
            let id = require_arg(rest, 0, "tasks redo <id|name>")?;
            ops::redo_task(data_dir, id).await
        }
        "next" => ops::next_task(data_dir).await,
        "reminders" => {
            let id = require_arg(rest, 0, "tasks reminders <id|name>")?;
            ops::task_start_reminders(data_dir, id).await
        }
        "plan" => dispatch_work(data_dir, rest).await,
        "item" => dispatch_task_item(data_dir, rest).await,
        other => Err(anyhow::anyhow!("Unknown tasks subcommand: '{other}'")),
    }
}

async fn dispatch_task_create(data_dir: &Path, args: &[String]) -> anyhow::Result<String> {
    let title = require_arg(args, 0, "tasks create <title> [--desc <desc>]")?;
    let desc = extract_flag_value(args, "--desc");
    ops::create_task(data_dir, title, desc.as_deref()).await
}

async fn dispatch_task_status(data_dir: &Path, args: &[String]) -> anyhow::Result<String> {
    let id = require_arg(args, 0, "tasks status <id|name> <status>")?;
    let status_str = require_arg(args, 1, "tasks status <id|name> <status>")?;
    let status = TaskStatus::parse(status_str)?;
    ops::set_task_status(data_dir, id, status).await
}

async fn dispatch_task_item(data_dir: &Path, args: &[String]) -> anyhow::Result<String> {
    let sub = args.first().map_or("", String::as_str);
    let rest = args.get(1..).unwrap_or(&[]);
    match sub {
        "add" => {
            let id = require_arg(rest, 0, "tasks item add <id|name> <text>")?;
            let text = require_arg(rest, 1, "tasks item add <id|name> <text>")?;
            ops::add_task_item(data_dir, id, text).await
        }
        "check" | "toggle" => {
            let id = require_arg(rest, 0, "tasks item check <id|name> <item_id>")?;
            let item_id_str = require_arg(rest, 1, "tasks item check <id|name> <item_id>")?;
            let item_id: u32 = item_id_str.parse().map_err(|err| {
                anyhow::anyhow!("item_id must be a number, got: {item_id_str} ({err})")
            })?;
            ops::check_task_item(data_dir, id, item_id).await
        }
        other => Err(anyhow::anyhow!("Unknown tasks item subcommand: '{other}'")),
    }
}

async fn dispatch_memory(data_dir: &Path, args: &[String]) -> anyhow::Result<String> {
    let sub = args.first().map_or("", String::as_str);
    let rest = args.get(1..).unwrap_or(&[]);
    match sub {
        "store" => {
            let id = require_arg(rest, 0, "memory store <id|name> <title> <content>")?;
            let title = require_arg(rest, 1, "memory store <id|name> <title> <content>")?;
            let content = require_arg(rest, 2, "memory store <id|name> <title> <content>")?;
            ops::store_memory(data_dir, id, title, content).await
        }
        "list" => {
            let id = require_arg(rest, 0, "memory list <id|name>")?;
            ops::load_memories(data_dir, id).await
        }
        other => Err(anyhow::anyhow!("Unknown memory subcommand: '{other}'")),
    }
}

async fn dispatch_finding(data_dir: &Path, args: &[String]) -> anyhow::Result<String> {
    let sub = args.first().map_or("", String::as_str);
    let rest = args.get(1..).unwrap_or(&[]);
    match sub {
        "store" => {
            let id = require_arg(rest, 0, "finding store <id|name> <content>")?;
            let content = require_arg(rest, 1, "finding store <id|name> <content>")?;
            ops::store_finding(data_dir, id, content).await
        }
        "list" => {
            let id = require_arg(rest, 0, "finding list <id|name>")?;
            ops::load_findings(data_dir, id).await
        }
        other => Err(anyhow::anyhow!("Unknown finding subcommand: '{other}'")),
    }
}

async fn dispatch_knowledge(data_dir: &Path, args: &[String]) -> anyhow::Result<String> {
    let sub = args.first().map_or("", String::as_str);
    let rest = args.get(1..).unwrap_or(&[]);
    match sub {
        "store" => {
            let topic = require_arg(rest, 0, "knowledge store <topic> <content>")?;
            let content = require_arg(rest, 1, "knowledge store <topic> <content>")?;
            let path = store::store_knowledge(data_dir, topic, content).await?;
            Ok(format!("📚 Knowledge stored: {}", path.display()))
        }
        "search" => {
            let query = require_arg(rest, 0, "knowledge search <query>")?;
            let results = store::search_knowledge(data_dir, query).await?;
            if results.is_empty() {
                return Ok(format!("No knowledge entries found for query: '{query}'"));
            }
            let mut out = vec![format!("# Knowledge Search: '{query}'\n")];
            for (slug, content) in &results {
                out.push(format!("---\n**Topic:** {slug}\n\n{content}"));
            }
            Ok(out.join("\n"))
        }
        "list" => {
            let topics = store::list_knowledge(data_dir).await?;
            if topics.is_empty() {
                return Ok(
                    "No knowledge entries yet. Add with: knowledge store <topic> <content>"
                        .to_string(),
                );
            }
            Ok(format!(
                "📚 Knowledge topics ({}):\n{}",
                topics.len(),
                topics
                    .iter()
                    .map(|t| format!("  • {t}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        }
        "get" => {
            let topic = require_arg(rest, 0, "knowledge get <topic>")?;
            match store::load_knowledge(data_dir, topic).await? {
                Some(content) => Ok(content),
                None => Err(anyhow::anyhow!("No knowledge entry found for '{topic}'")),
            }
        }
        other => Err(anyhow::anyhow!("Unknown knowledge subcommand: '{other}'")),
    }
}

async fn dispatch_work(data_dir: &Path, args: &[String]) -> anyhow::Result<String> {
    let count = extract_flag_value(args, "--count")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(3);
    ops::work_plan(data_dir, count).await
}

// ─── Argument helpers ─────────────────────────────────────────────────────────

/// Get the nth positional argument or return an error with usage hint.
fn require_arg<'a>(args: &'a [String], index: usize, usage: &str) -> anyhow::Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("Missing argument. Usage: {usage}"))
}

/// Extract the value of a `--flag <value>` pair from the args list.
fn extract_flag_value(args: &[String], flag: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos.saturating_add(1)).cloned()
}

// ─── Output helpers ───────────────────────────────────────────────────────────

/// Write a line (with newline) to stdout. Does NOT use `println!`.
fn write_stdout_line(text: &str) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{text}")?;
    Ok(())
}

/// Write raw text to stdout. Does NOT use `print!`.
fn write_stdout(text: &str) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    write!(handle, "{text}")?;
    Ok(())
}

/// Write a line to stderr. Does NOT use `eprintln!`.
fn write_stderr_line(text: &str) -> anyhow::Result<()> {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    writeln!(handle, "{text}")?;
    Ok(())
}
