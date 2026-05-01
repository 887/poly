//! MCP JSON-RPC 2.0 server for the memory system.
//!
//! Reads JSON-RPC requests from stdin (one per line) and writes responses to
//! stdout. Each tool call dispatches to the corresponding operation in `ops`.
//!
//! **Prefer CLI access over MCP access.** See `cli.rs` and the README.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt as _;
use tokio::io::AsyncWriteExt as _;

use crate::ops;
use crate::store;
use crate::types::TaskStatus;

// ─── JSON-RPC helpers ─────────────────────────────────────────────────────────

fn ok_result(text: &str) -> Value {
    json!({ "content": [{"type": "text", "text": text}], "isError": false })
}

fn err_result(text: &str) -> Value {
    json!({ "content": [{"type": "text", "text": text}], "isError": true })
}

fn mcp_response(id: Option<&Value>, result: &Value) -> String {
    serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": id, "result": result }))
        .unwrap_or_default()
}

fn mcp_error(id: Option<&Value>, code: i64, msg: &str) -> String {
    serde_json::to_string(
        &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } }),
    )
    .unwrap_or_default()
}

// ─── Tool list ─────────────────────────────────────────────────────────────────

/// Return the full list of MCP tool definitions.
///
/// Descriptions include embedded agent instructions because the MCP client
/// shows these to the AI agent as context.
pub fn tool_list() -> Vec<Value> {
    vec![
        // ── Task management ──────────────────────────────────────────
        json!({
            "name": "list_tasks",
            "description": "List all tasks with status, memory count, and finding count. \
                            Always call this first to see what work exists. \
                            Response includes MANDATORY agent rules.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "create_task",
            "description": "Create a new numbered task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Task title (required)" },
                    "description": { "type": "string", "description": "Optional longer description" }
                },
                "required": ["title"]
            }
        }),
        json!({
            "name": "get_task",
            "description": "Get full details for a task: status, description, checklist, memory and finding counts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID (number) or title/slug (partial match)" }
                },
                "required": ["id_or_name"]
            }
        }),
        json!({
            "name": "set_task_status",
            "description": "Change the status of a task. Valid values: todo, in-progress, completed, redo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID or name" },
                    "status": { "type": "string", "enum": ["todo", "in-progress", "completed", "redo"] }
                },
                "required": ["id_or_name", "status"]
            }
        }),
        json!({
            "name": "redo_task",
            "description": "Reset a task for redo: status→todo, all checklist items unchecked. Memories and findings are PRESERVED.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID or name" }
                },
                "required": ["id_or_name"]
            }
        }),
        json!({
            "name": "add_task_item",
            "description": "Add a checklist item to a task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID or name" },
                    "text": { "type": "string", "description": "Item description" }
                },
                "required": ["id_or_name", "text"]
            }
        }),
        json!({
            "name": "check_task_item",
            "description": "Toggle a checklist item done/undone.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID or name" },
                    "item_id": { "type": "integer", "description": "Checklist item ID" }
                },
                "required": ["id_or_name", "item_id"]
            }
        }),
        // ── Memory operations ────────────────────────────────────────
        json!({
            "name": "store_memory",
            "description": "Store a memory note for a task. Persisted as a markdown file. \
                            Store key decisions, architectural notes, and intermediate progress here.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID or name" },
                    "title": { "type": "string", "description": "Short memory title" },
                    "content": { "type": "string", "description": "Memory content (markdown)" }
                },
                "required": ["id_or_name", "title", "content"]
            }
        }),
        json!({
            "name": "load_memories",
            "description": "Load all memory notes for a task. \
                            ALWAYS call this at the start of a task session to retrieve prior context.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID or name" }
                },
                "required": ["id_or_name"]
            }
        }),
        // ── Finding operations ───────────────────────────────────────
        json!({
            "name": "store_finding",
            "description": "CALL THIS CONSTANTLY during research. \
                            Stores a research finding to findings.md (append-only). \
                            If the session crashes, only findings already stored are preserved. \
                            Store every important discovery IMMEDIATELY — do not batch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID or name" },
                    "content": { "type": "string", "description": "Finding content (markdown)" }
                },
                "required": ["id_or_name", "content"]
            }
        }),
        json!({
            "name": "load_findings",
            "description": "Load all research findings for a task. \
                            ALWAYS call this at the start of a task session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID or name" }
                },
                "required": ["id_or_name"]
            }
        }),
        // ── Knowledge base ───────────────────────────────────────────
        json!({
            "name": "store_knowledge",
            "description": "Store or update a general knowledge entry (not task-specific). \
                            Use for reusable facts: library API notes, architecture decisions, etc.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "topic": { "type": "string", "description": "Topic name (becomes filename slug)" },
                    "content": { "type": "string", "description": "Knowledge content (markdown)" }
                },
                "required": ["topic", "content"]
            }
        }),
        json!({
            "name": "search_knowledge",
            "description": "Search the general knowledge base by keyword.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "list_knowledge",
            "description": "List all topics in the general knowledge base.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "get_knowledge",
            "description": "Get a specific knowledge entry by topic.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "topic": { "type": "string", "description": "Topic name or slug" }
                },
                "required": ["topic"]
            }
        }),
        // ── Workflow ─────────────────────────────────────────────────
        json!({
            "name": "next_task",
            "description": "Get the next pending (todo or redo) task.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "task_start_reminders",
            "description": "ALWAYS call this before starting work on any task. \
                            Returns: existing memory count, finding count, checklist status, \
                            and MANDATORY agent rules (store findings immediately, follow agents.md, etc.).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id_or_name": { "type": "string", "description": "Task ID or name" }
                },
                "required": ["id_or_name"]
            }
        }),
        json!({
            "name": "work_plan",
            "description": "Get a work plan for the next N pending tasks. \
                            Tells you which tasks to work on and the mandatory procedure for each. \
                            Use this when asked to 'work on N tasks'.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "count": { "type": "integer", "minimum": 1, "description": "Number of tasks to plan (default: 3)" }
                }
            }
        }),
    ]
}

// ─── Dispatch ─────────────────────────────────────────────────────────────────

/// Dispatch a single tool call to the appropriate operation.
async fn dispatch_tool(name: &str, params: &Value, data_dir: &Path) -> Value {
    let res = dispatch_inner(name, params, data_dir).await;
    match res {
        Ok(msg) => ok_result(&msg),
        Err(e) => err_result(&format!("Error in {name}: {e}")),
    }
}

/// Inner dispatch — returns `anyhow::Result<String>` so `?` works cleanly.
async fn dispatch_inner(name: &str, params: &Value, data_dir: &Path) -> anyhow::Result<String> {
    match name {
        "list_tasks" => ops::list_tasks(data_dir).await,
        "create_task" => {
            let title = str_param(params, "title")?;
            let desc = opt_str_param(params, "description");
            ops::create_task(data_dir, title, desc).await
        }
        "get_task" => {
            let id = str_param(params, "id_or_name")?;
            ops::get_task(data_dir, id).await
        }
        "set_task_status" => {
            let id = str_param(params, "id_or_name")?;
            let status = TaskStatus::parse(str_param(params, "status")?)?;
            ops::set_task_status(data_dir, id, status).await
        }
        "redo_task" => {
            let id = str_param(params, "id_or_name")?;
            ops::redo_task(data_dir, id).await
        }
        "add_task_item" => {
            let id = str_param(params, "id_or_name")?;
            let text = str_param(params, "text")?;
            ops::add_task_item(data_dir, id, text).await
        }
        "check_task_item" => {
            let id = str_param(params, "id_or_name")?;
            let item_id = u32_param(params, "item_id")?;
            ops::check_task_item(data_dir, id, item_id).await
        }
        "store_memory" => dispatch_store_memory(params, data_dir).await,
        "load_memories" => {
            let id = str_param(params, "id_or_name")?;
            ops::load_memories(data_dir, id).await
        }
        "store_finding" => {
            let id = str_param(params, "id_or_name")?;
            let content = str_param(params, "content")?;
            ops::store_finding(data_dir, id, content).await
        }
        "load_findings" => {
            let id = str_param(params, "id_or_name")?;
            ops::load_findings(data_dir, id).await
        }
        "store_knowledge" => dispatch_store_knowledge(params, data_dir).await,
        "search_knowledge" => {
            let query = str_param(params, "query")?;
            let results = store::search_knowledge(data_dir, query).await?;
            format_knowledge_results(query, &results)
        }
        "list_knowledge" => {
            let topics = store::list_knowledge(data_dir).await?;
            format_knowledge_list(&topics)
        }
        "get_knowledge" => {
            let topic = str_param(params, "topic")?;
            store::load_knowledge(data_dir, topic)
                .await?
                .ok_or_else(|| anyhow::anyhow!("No knowledge entry for '{topic}'"))
        }
        "next_task" => ops::next_task(data_dir).await,
        "task_start_reminders" => {
            let id = str_param(params, "id_or_name")?;
            ops::task_start_reminders(data_dir, id).await
        }
        "work_plan" => {
            let count = params
                .get("count")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(3_usize);
            ops::work_plan(data_dir, count).await
        }
        other => Err(anyhow::anyhow!("Unknown tool: {other}")),
    }
}

async fn dispatch_store_memory(params: &Value, data_dir: &Path) -> anyhow::Result<String> {
    let id = str_param(params, "id_or_name")?;
    let title = str_param(params, "title")?;
    let content = str_param(params, "content")?;
    ops::store_memory(data_dir, id, title, content).await
}

async fn dispatch_store_knowledge(params: &Value, data_dir: &Path) -> anyhow::Result<String> {
    let topic = str_param(params, "topic")?;
    let content = str_param(params, "content")?;
    let path = store::store_knowledge(data_dir, topic, content).await?;
    Ok(format!("📚 Knowledge stored: {}", path.display()))
}

// ─── Parameter extraction ─────────────────────────────────────────────────────

fn str_param<'a>(params: &'a Value, key: &str) -> anyhow::Result<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: '{key}'"))
}

fn opt_str_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}

fn u32_param(params: &Value, key: &str) -> anyhow::Result<u32> {
    let v = params
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid integer parameter: '{key}'"))?;
    u32::try_from(v).map_err(|err| {
        anyhow::anyhow!("Integer parameter '{key}' too large for u32: {v} ({err})")
    })
}

// ─── Formatting helpers ───────────────────────────────────────────────────────

fn format_knowledge_results(query: &str, results: &[(String, String)]) -> anyhow::Result<String> {
    if results.is_empty() {
        return Ok(format!("No knowledge entries found for: '{query}'"));
    }
    let mut out = vec![format!("# Knowledge Search: '{query}'\n")];
    for (slug, content) in results {
        out.push(format!("---\n**Topic:** {slug}\n\n{content}"));
    }
    Ok(out.join("\n"))
}

fn format_knowledge_list(topics: &[String]) -> anyhow::Result<String> {
    if topics.is_empty() {
        return Ok("No knowledge entries yet. Add with `store_knowledge`.".to_string());
    }
    let list = topics
        .iter()
        .map(|t| format!("  • {t}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!("📚 Knowledge topics ({}):\n{list}", topics.len()))
}

// ─── MCP server loop ──────────────────────────────────────────────────────────

/// Run the MCP JSON-RPC server, reading from stdin, writing to stdout.
///
/// Prefer CLI mode over this for agent scripts. This mode is for VS Code
/// Copilot integration via `.vscode/mcp.json`.
pub async fn run_server(data_dir: PathBuf) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = handle_request(trimmed, &data_dir).await;
        let out = format!("{response}\n");
        stdout.write_all(out.as_bytes()).await?;
        stdout.flush().await?;
    }
    Ok(())
}

/// Parse and handle a single JSON-RPC request line.
async fn handle_request(line: &str, data_dir: &Path) -> String {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return mcp_error(None, -32_700, &format!("Parse error: {e}"));
        }
    };
    let id = req.get("id").cloned();
    let method = match req.get("method").and_then(Value::as_str) {
        Some(m) => m.to_string(),
        None => {
            return mcp_error(id.as_ref(), -32_600, "Missing method field");
        }
    };
    let params = req.get("params").cloned().unwrap_or(json!({}));
    handle_method(&method, id, &params, data_dir).await
}

/// Dispatch by JSON-RPC method.
async fn handle_method(method: &str, id: Option<Value>, params: &Value, data_dir: &Path) -> String {
    let id_ref = id.as_ref();
    match method {
        "initialize" => mcp_response(
            id_ref,
            &json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "poly-memory-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "tools/list" => mcp_response(id_ref, &json!({ "tools": tool_list() })),
        "tools/call" => {
            let empty_obj = json!({});
            let tool_name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>");
            let tool_params = params.get("arguments").unwrap_or(&empty_obj);
            let result = dispatch_tool(tool_name, tool_params, data_dir).await;
            mcp_response(id_ref, &result)
        }
        "notifications/initialized" | "$/cancelRequest" => String::new(),
        other => mcp_error(id_ref, -32_601, &format!("Unknown method: {other}")),
    }
}
