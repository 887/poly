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

use poly_chat_mcp::{state, tools};

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

    let app = axum::Router::new()
        .route("/mcp", post(handle_mcp_http))
        .route("/health", get(handle_health))
        .with_state(pool);

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
    AxumState(pool): AxumState<SharedPool>,
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
            let mut pool = pool.lock().await;
            let result = tools::dispatch(tool_name, &args, &mut pool).await;
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
                let result = tools::dispatch(tool_name, &args, &mut pool).await;
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

// ─── JSON-RPC helpers ────────────────────────────────────────────────────────

fn mcp_response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn mcp_error(id: Option<Value>, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}
