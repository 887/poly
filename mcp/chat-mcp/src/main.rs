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
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Json;
use futures_util::StreamExt;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::CorsLayer;
use clap::Parser;
use poly_client::MessageContent;
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

    // B.3 — Auto-send engine: poll every 2 seconds for overdue drafts.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(run_autosend_engine(
        Arc::clone(&pool),
        Arc::clone(&mem),
        shutdown_rx,
    ));

    // H.3 — Daily audit prune: delete persona_audit rows older than 30 days.
    tokio::spawn(poly_chat_mcp::persona_audit_prune::run_forever(
        mem.as_ref().clone(),
    ));

    let state = AppState { pool, mem };

    let app = axum::Router::new()
        .route("/mcp", post(handle_mcp_http))
        .route("/mcp/sse", get(handle_mcp_sse))
        .route("/health", get(handle_health))
        .with_state(state)
        // Permissive CORS so the WASM UI (poly-web on :3000) and any
        // other local client can POST JSON to /mcp without preflight 405s.
        .layer(CorsLayer::very_permissive());

    let addr = format!("127.0.0.1:{port}");
    tracing::info!("poly-chat-mcp listening on http://{addr}");
    tracing::info!("  POST http://{addr}/mcp      — JSON-RPC endpoint (request/response)");
    tracing::info!("  GET  http://{addr}/mcp/sse  — Server-Sent Events stream (server-push)");
    tracing::info!("  GET  http://{addr}/health");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    // Shutdown the auto-send engine when the HTTP server exits.
    shutdown_tx.send(()).ok();
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

// ─── SSE event-stream endpoint (C.4) ─────────────────────────────────────────
//
// MCP server-push transport. Wraps each `McpEvent` from the broadcast channel
// in a JSON-RPC `notifications/event` frame and writes it as a Server-Sent
// Event. Compatible with MCP spec 2025-11-25's Streamable HTTP transport.
//
// Hosts that consume this endpoint do NOT need to call `poll_events`. Hosts
// that don't (incl. Claude Desktop today — see anthropics/claude-code#4118
// & #13646) keep using the existing `poll_events` tool over POST /mcp.
//
// The polling tool stays the source of truth for the event ring buffer; SSE
// is a thin push wrapper on the same broadcast channel, so both paths see
// the same event ordering.
async fn handle_mcp_sse(
    AxumState(state): AxumState<AppState>,
) -> Sse<impl futures_core::Stream<Item = Result<SseEvent, Infallible>>> {
    // Subscribe to the broadcast channel. The receiver is owned by the
    // returned stream; when the SSE connection closes, the receiver drops
    // and the broadcast slot is reclaimed.
    let rx = {
        let pool = state.pool.lock().await;
        let store = pool.events.lock().await;
        store.subscribe_broadcast()
    };

    let stream = BroadcastStream::new(rx).filter_map(|res| async move {
        match res {
            Ok(ev) => {
                let frame = json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/event",
                    "params": ev,
                });
                Some(Ok(SseEvent::default().data(frame.to_string())))
            }
            // Lagged receiver — broadcast ring overran. Skip silently;
            // the host can re-sync via `poll_events` with the cursor it
            // last saw if it cares about gap detection.
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ─── Auto-send engine (B.3) ──────────────────────────────────────────────────

/// Background task that polls the `drafts` table every 2 seconds and sends
/// any pending drafts whose `auto_send_at` has passed.
///
/// Shuts down cleanly when `shutdown` fires or is dropped.
async fn run_autosend_engine(
    pool: SharedPool,
    mem:  Arc<MemoryDb>,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) {
    loop {
        // Wait 2s or until shutdown.
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}
            _ = &mut shutdown => {
                tracing::debug!("auto-send engine: shutdown signal received");
                return;
            }
        }

        let due = match mem.draft_pending_autosend() {
            Ok(d)  => d,
            Err(e) => {
                tracing::warn!("auto-send engine: draft_pending_autosend failed: {e}");
                continue;
            }
        };

        for draft in due {
            let draft_id = match draft.get("id").and_then(serde_json::Value::as_i64) {
                Some(id) => id,
                None     => continue,
            };
            let account_id = match draft.get("account_id").and_then(|v| v.as_str()) {
                Some(a) => a.to_string(),
                None    => continue,
            };
            let chat_id = match draft.get("chat_id").and_then(|v| v.as_str()) {
                Some(c) => c.to_string(),
                None    => continue,
            };
            let body = match draft.get("body").and_then(|v| v.as_str()) {
                Some(b) => b.to_string(),
                None    => continue,
            };

            // Look up backend and attempt send.
            let send_result = {
                let locked = pool.lock().await;
                if locked.find_by_account(&account_id).is_some() {
                    // We need to call async send_message. The lock prevents us from
                    // holding it across an await, so extract what we need first.
                    // Since `ClientBackend` is `Send + Sync` we can clone the channel/body.
                    drop(locked);
                    // Re-acquire after drop to call async method.
                    let pool2 = Arc::clone(&pool);
                    let account_id2 = account_id.clone();
                    let chat_id2 = chat_id.clone();
                    let body2 = body.clone();
                    
                    async move {
                        let locked = pool2.lock().await;
                        if let Some(e) = locked.find_by_account(&account_id2) {
                            e.backend.send_message(&chat_id2, MessageContent::Text(body2)).await
                        } else {
                            Err(poly_client::ClientError::NotSupported(
                                "no backend for account".to_string()
                            ))
                        }
                    }.await
                } else {
                    drop(locked);
                    Err(poly_client::ClientError::NotSupported(
                        "no backend for account".to_string()
                    ))
                }
            };

            match send_result {
                Ok(_) => {
                    if let Err(e) = mem.draft_set_status(draft_id, "sent") {
                        tracing::warn!("auto-send engine: sent draft {draft_id} but status update failed: {e}");
                    } else {
                        tracing::info!("auto-send engine: draft {draft_id} auto-sent to {chat_id}");
                    }
                }
                Err(e) => {
                    tracing::warn!("auto-send engine: draft {draft_id} send failed: {e}; marking expired");
                    drop(mem.draft_set_status(draft_id, "expired"));
                }
            }
        }
    }
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
            let base: std::path::PathBuf = std::env::var("XDG_DATA_HOME").map_or_else(
                |_| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                    std::path::PathBuf::from(home).join(".local").join("share")
                },
                std::path::PathBuf::from,
            );
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

// poly-lint: id and result pass through json! by value into JSON-RPC wire format.
#[allow(clippy::needless_pass_by_value)]
fn mcp_response(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

#[allow(clippy::needless_pass_by_value)]
fn mcp_error(id: Option<Value>, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}
