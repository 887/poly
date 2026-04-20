//! # poly-chat-mcp
//!
//! MCP server exposing all Poly chat backends as tools.
//!
//! ## Modes
//!
//! - **HTTP** (default): Listens on a port, accepts JSON-RPC at `POST /mcp`.
//!   poly-cli and other HTTP clients connect here.
//!
//! - **stdio**: Reads JSON-RPC from stdin, writes to stdout.
//!   For future mcp.json / Claude Code integration.
//!
//! ## Usage
//!
//! ```bash
//! # HTTP mode (default, port 3010)
//! poly-chat-mcp
//! poly-chat-mcp --port 3010
//!
//! # stdio mode (for mcp.json)
//! poly-chat-mcp --stdio
//! ```

use poly_chat_mcp::{memory::MemoryDb, state, tools};

use std::sync::Arc;

use axum::extract::State as AxumState;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Json;
use clap::Parser;
use serde_json::{Value, json};
use tokio::sync::Mutex;

#[derive(Parser)]
#[command(name = "poly-chat-mcp", about = "MCP server for Poly chat backends")]
struct Args {
    /// Port for HTTP mode (default: 3010)
    #[arg(long, default_value = "3010")]
    port: u16,

    /// Use stdio mode instead of HTTP (for mcp.json integration)
    #[arg(long)]
    stdio: bool,
}

type SharedPool = Arc<Mutex<state::BackendPool>>;

/// Shared state threaded through Axum handlers.
#[derive(Clone)]
struct AppState {
    pool: SharedPool,
    mem:  Arc<MemoryDb>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();

    if args.stdio {
        run_stdio().await
    } else {
        run_http(args.port).await
    }
}

// ─── HTTP mode ───────────────────────────────────────────────────────────────

async fn run_http(port: u16) -> anyhow::Result<()> {
    let pool: SharedPool = Arc::new(Mutex::new(state::BackendPool::new()));
    let mem = Arc::new(open_memory_db()?);
    let state = AppState { pool, mem };

    let app = axum::Router::new()
        .route("/mcp", post(handle_mcp_http))
        .route("/health", get(handle_health))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    tracing::info!("poly-chat-mcp listening on http://{addr}");
    tracing::info!("  POST http://{addr}/mcp  — JSON-RPC endpoint");
    tracing::info!("  GET  http://{addr}/health");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_health() -> impl IntoResponse {
    Json(json!({"status": "ok", "server": "poly-chat-mcp"}))
}

async fn handle_mcp_http(
    AxumState(state): AxumState<AppState>,
    Json(req): Json<Value>,
) -> impl IntoResponse {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = req.get("id").cloned();
    let params = req.get("params").cloned().unwrap_or(json!({}));

    let result = match method {
        "initialize" => {
            let result = json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "poly-chat-mcp", "version": "0.1.0" }
            });
            mcp_response(id, result)
        }
        "notifications/initialized" => return (StatusCode::OK, Json(json!({}))),
        "tools/list" => {
            let result = json!({ "tools": tools::tool_list() });
            mcp_response(id, result)
        }
        "tools/call" => {
            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let mut pool = state.pool.lock().await;
            let result = tools::dispatch(tool_name, &args, &mut pool, &state.mem).await;
            mcp_response(id, result)
        }
        _ => mcp_error(id, -32601, &format!("Method not found: {method}")),
    };

    (StatusCode::OK, Json(result))
}

// ─── stdio mode ──────────────────────────────────────────────────────────────

async fn run_stdio() -> anyhow::Result<()> {
    use tokio::io::AsyncBufReadExt as _;
    use tokio::io::AsyncWriteExt as _;

    tracing::info!("poly-chat-mcp running in stdio mode");

    let mut pool = state::BackendPool::new();
    let mem = open_memory_db()?;
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut lines = stdin.lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = mcp_error(None, -32700, &format!("Parse error: {e}"));
                let s = serde_json::to_string(&resp).unwrap_or_default();
                stdout.write_all(s.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
                continue;
            }
        };

        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(json!({}));

        let resp = match method {
            "initialize" => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "poly-chat-mcp", "version": "0.1.0" }
                });
                mcp_response(id, result)
            }
            "notifications/initialized" => continue,
            "tools/list" => {
                let result = json!({ "tools": tools::tool_list() });
                mcp_response(id, result)
            }
            "tools/call" => {
                let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let result = tools::dispatch(tool_name, &args, &mut pool, &mem).await;
                mcp_response(id, result)
            }
            _ => mcp_error(id, -32601, &format!("Method not found: {method}")),
        };

        let s = serde_json::to_string(&resp).unwrap_or_default();
        stdout.write_all(s.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

// ─── Memory DB initialisation ────────────────────────────────────────────────

/// Open the memory DB in the same `storage.sqlite3` as the rest of Poly.
///
/// The data directory is resolved via `POLY_DATA_DIR` env var (override) or
/// the platform-default path (`~/.local/share/poly/` on Linux, etc.).
fn open_memory_db() -> anyhow::Result<MemoryDb> {
    let data_dir: std::path::PathBuf = if let Ok(d) = std::env::var("POLY_DATA_DIR") {
        std::path::PathBuf::from(d)
    } else {
        #[cfg(target_os = "linux")]
        {
            let base: std::path::PathBuf = std::env::var("XDG_DATA_HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                    std::path::PathBuf::from(home).join(".local").join("share")
                });
            base.join("poly")
        }
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            std::path::Path::new(&home)
                .join("Library")
                .join("Application Support")
                .join("poly")
        }
        #[cfg(target_os = "windows")]
        {
            let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
            std::path::Path::new(&appdata).join("poly")
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            std::path::PathBuf::from(".poly")
        }
    };
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("storage.sqlite3");
    MemoryDb::open(db_path.to_str().unwrap_or(":memory:"))
        .map_err(|e| anyhow::anyhow!("failed to open memory DB: {e}"))
}

// ─── JSON-RPC helpers ────────────────────────────────────────────────────────

fn mcp_response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn mcp_error(id: Option<Value>, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}
