//! # poly-cli
//!
//! Dynamic MCP-to-CLI translator for Poly chat backends.
//!
//! Connects to a running `poly-chat-mcp` server via HTTP, discovers
//! available tools, and exposes them as CLI subcommands.
//!
//! ## Usage
//!
//! ```bash
//! poly-cli tools                                    # list available tools
//! poly-cli call login --backend matrix --url ...    # call a tool
//! poly-cli call list_servers --backend matrix       # call another tool
//! poly-cli --url http://localhost:3001/mcp call ... # target different server
//! ```
//!
//! ## Watch mode
//!
//! ```bash
//! poly-cli --watch 5 call meta_persona_audit_query --slug broker-bob --since auto
//! ```
//!
//! `--watch <N>` re-runs the call every N seconds. When the result is an
//! array of objects, only rows with an `id` field not seen before are printed.
//!
//! `--since auto` (only meaningful with `--watch`) initialises the `since`
//! argument to the current UTC timestamp and advances it to the latest
//! `occurred_at` value seen after each poll. This makes the tool behave like
//! a live-tail: you see only rows that arrive after the watch started.
//!
//! Exit cleanly with Ctrl+C.

mod mcp_client;

use clap::{Parser, Subcommand};
use mcp_client::McpClient;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Parser)]
#[command(name = "poly-cli", about = "Poly MCP CLI — dynamic tool interface")]
struct Cli {
    /// MCP server URL (default: http://localhost:3010/mcp)
    #[arg(long, default_value = "http://localhost:3010/mcp")]
    url: String,

    /// Output format
    #[arg(long, default_value = "pretty")]
    format: OutputFormat,

    /// Watch mode: re-run the call every N seconds and print only new rows.
    /// Requires the tool to return a JSON array of objects with an `id` field.
    /// Exit with Ctrl+C.
    #[arg(long, value_name = "SECONDS")]
    watch: Option<u64>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    Json,
    Pretty,
}

#[derive(Subcommand)]
enum Command {
    /// List available MCP tools and their descriptions
    Tools,

    /// Call an MCP tool by name
    Call {
        /// Tool name (e.g. login, list_servers, send_message)
        tool: String,

        /// Tool arguments as --key value pairs. Values are passed as strings
        /// unless they look like JSON (start with { or [).
        /// Special value: --since auto (with --watch) sets since to the current
        /// UTC timestamp and advances it after each poll.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Check if the MCP server is reachable
    Health,
}

#[tokio::main]
// poly-cli is a user-facing CLI binary; println!/eprintln! is the production output channel.
#[allow(clippy::print_stdout, clippy::print_stderr)]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = McpClient::new(&cli.url);

    match cli.command {
        Command::Health => {
            // Just check connectivity by calling initialize
            match client.initialize().await {
                Ok(info) => {
                    print_value(
                        &json!({
                            "status": "connected",
                            "server": info.get("serverInfo"),
                        }),
                        cli.format,
                    );
                }
                Err(e) => {
                    eprintln!("Cannot reach MCP server at {}: {e}", cli.url);
                    std::process::exit(1);
                }
            }
        }

        Command::Tools => {
            let tools = client.list_tools().await?;
            for tool in &tools {
                let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                let desc = tool
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                // Truncate description for display
                let short_desc: String = desc.chars().take(80).collect();
                println!("  {name:<20} {short_desc}");
            }
            println!("\nUse: poly-cli call <tool> --key value ...");
            println!("     poly-cli call <tool> --help for tool schema");
        }

        Command::Call { tool, args } => {
            // If user passes --help, show the tool schema
            if args.iter().any(|a| a == "--help" || a == "-h") {
                return show_tool_help(&client, &tool).await;
            }

            match cli.watch {
                None => {
                    // Normal one-shot call.
                    let arguments = parse_tool_args(&args);
                    let result = client.call_tool(&tool, arguments).await?;
                    let text = extract_tool_result(&result);
                    print_result_text(&text, cli.format);

                    if result
                        .get("isError")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                    {
                        std::process::exit(1);
                    }
                }

                Some(interval_secs) => {
                    // Watch mode: poll every interval_secs seconds, dedupe by id.
                    run_watch(&client, &tool, &args, interval_secs, cli.format).await?;
                }
            }
        }
    }

    Ok(())
}

/// Watch-mode loop.
///
/// Behaviour:
/// - Polls the tool every `interval_secs` seconds.
/// - Deduplicates rows by their `id` field; only prints rows not seen before.
/// - If `--since auto` is in `raw_args`, replaces the `since` value with the
///   current UTC timestamp on the first call, then advances it to the latest
///   `occurred_at` seen after each successful poll.
/// - Runs until SIGINT (Ctrl+C).
// poly-cli is a user-facing CLI binary; println!/eprintln! is the production output channel.
#[allow(clippy::print_stdout, clippy::print_stderr, clippy::cognitive_complexity)]
async fn run_watch(
    client: &McpClient,
    tool: &str,
    raw_args: &[String],
    interval_secs: u64,
    fmt: OutputFormat,
) -> anyhow::Result<()> {
    use tokio::signal;

    // Detect `--since auto` in the raw args.
    let has_since_auto = raw_args
        .windows(2)
        .any(|w| w.first().map(String::as_str) == Some("--since")
              && w.get(1).map(String::as_str) == Some("auto"));

    // Start `since` at the current UTC timestamp.
    let mut since_value: String = current_utc_iso8601();

    let mut seen_ids: HashSet<i64> = HashSet::new();

    eprintln!("[watch] Polling '{tool}' every {interval_secs}s. Press Ctrl+C to stop.");

    let ctrl_c = signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        // Build arguments, substituting `since auto` if present.
        let call_args = if has_since_auto {
            replace_since_auto(raw_args, &since_value)
        } else {
            raw_args.to_vec()
        };

        let arguments = parse_tool_args(&call_args);

        match client.call_tool(tool, arguments).await {
            Ok(result) => {
                let text = extract_tool_result(&result);
                // Try to parse as a JSON array of objects.
                match serde_json::from_str::<Value>(&text) {
                    Ok(Value::Array(rows)) => {
                        let mut latest_occurred_at: Option<String> = None;

                        for row in &rows {
                            // Dedupe by id field.
                            let id = row.get("id").and_then(serde_json::Value::as_i64);
                            if let Some(id_val) = id {
                                if seen_ids.contains(&id_val) {
                                    continue;
                                }
                                seen_ids.insert(id_val);
                            }

                            // Track latest occurred_at for --since auto advance.
                            if let Some(oa) = row.get("occurred_at").and_then(|v| v.as_str()) {
                                match &latest_occurred_at {
                                    None => latest_occurred_at = Some(oa.to_string()),
                                    Some(prev) if oa > prev.as_str() => {
                                        latest_occurred_at = Some(oa.to_string());
                                    }
                                    _ => {}
                                }
                            }

                            // Print the new row.
                            print_result_text(
                                &serde_json::to_string_pretty(row).unwrap_or_default(),
                                fmt,
                            );
                        }

                        // Advance `since` to the latest occurred_at seen.
                        if has_since_auto
                            && let Some(lat) = latest_occurred_at
                        {
                            since_value = lat;
                        }
                    }
                    _ => {
                        // Non-array result: print as-is (first time only if
                        // it looks like an error).
                        print_result_text(&text, fmt);
                    }
                }
            }
            Err(e) => {
                eprintln!("[watch] call failed: {e}");
            }
        }

        // Wait for either the interval or Ctrl+C.
        tokio::select! {
            () = tokio::time::sleep(std::time::Duration::from_secs(interval_secs)) => {},
            _ = &mut ctrl_c => {
                eprintln!("[watch] interrupted.");
                break;
            }
        }
    }

    Ok(())
}

/// Replace the value of `--since auto` with `replacement` in a raw arg list.
// Index loop over `args.len()`; arithmetic on `i` is bounded by the loop guard.
#[allow(clippy::arithmetic_side_effects)]
fn replace_since_auto(args: &[String], replacement: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let cur = args.get(i).map(String::as_str);
        let next = args.get(i + 1).map(String::as_str);
        if cur == Some("--since") && next == Some("auto") {
            out.push("--since".to_string());
            out.push(replacement.to_string());
            i += 2;
        } else if let Some(arg) = args.get(i) {
            out.push(arg.clone());
            i += 1;
        } else {
            break;
        }
    }
    out
}

/// Return the current UTC time as an ISO-8601 string (seconds precision).
// Integer-arithmetic time decomposition; modular reductions cannot overflow.
#[allow(clippy::integer_division, clippy::arithmetic_side_effects)]
fn current_utc_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hr  = (secs / 3600) % 24;
    let days = secs / 86400;
    let (yr, mo, dy) = days_to_ymd(days);
    format!("{yr:04}-{mo:02}-{dy:02}T{hr:02}:{min:02}:{sec:02}Z")
}

/// Integer-arithmetic Gregorian calendar conversion (same algorithm as memory.rs).
// Howard Hinnant's date algorithm requires exact integer arithmetic.
#[allow(clippy::integer_division, clippy::arithmetic_side_effects)]
const fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z   = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y   = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let mo  = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

/// Parse --key value pairs into a JSON object.
/// Values that look like JSON objects/arrays are parsed as JSON.
/// Boolean strings "true"/"false" become JSON booleans.
/// Numeric strings become JSON numbers.
// Index loop bounded by args.get; overflow only at usize::MAX args (unreachable).
#[allow(clippy::arithmetic_side_effects)]
fn parse_tool_args(args: &[String]) -> Value {
    let mut map = serde_json::Map::new();
    let mut i = 0;

    while let Some(arg) = args.get(i) {
        if let Some(key) = arg.strip_prefix("--") {
            i += 1;
            let Some(val) = args.get(i) else {
                // Flag without value = true
                map.insert(key.to_string(), json!(true));
                continue;
            };
            // Try to parse as JSON
            let json_val = if val.starts_with('{') || val.starts_with('[') {
                serde_json::from_str(val).unwrap_or_else(|_| json!(val))
            } else if val == "true" {
                json!(true)
            } else if val == "false" {
                json!(false)
            } else if let Ok(n) = val.parse::<i64>() {
                json!(n)
            } else {
                json!(val)
            };
            map.insert(key.to_string(), json_val);
        }
        i += 1;
    }

    Value::Object(map)
}

/// Extract the text content from an MCP tool result.
fn extract_tool_result(result: &Value) -> String {
    result
        .get("content")
        .and_then(|c| c.as_array())
        .map_or_else(
            || serde_json::to_string_pretty(result).unwrap_or_default(),
            |content| {
                content
                    .iter()
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        )
}

/// Show the schema/help for a specific tool.
// poly-cli is a user-facing CLI binary; println!/eprintln! is the production output channel.
#[allow(clippy::print_stdout, clippy::print_stderr)]
async fn show_tool_help(client: &McpClient, tool_name: &str) -> anyhow::Result<()> {
    let tools = client.list_tools().await?;
    let tool = tools
        .iter()
        .find(|t| t.get("name").and_then(|n| n.as_str()) == Some(tool_name));

    if let Some(t) = tool {
        let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("?");
        let desc = t
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("");
        println!("Tool: {name}");
        println!("Description: {desc}");
        println!();

        if let Some(schema) = t.get("inputSchema")
            && let Some(props) = schema.get("properties").and_then(|p| p.as_object())
        {
            let required: Vec<&str> = schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect()
                })
                .unwrap_or_default();

            println!("Arguments:");
            for (key, prop) in props {
                let ptype = prop
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("string");
                let pdesc = prop
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                let req = if required.contains(&key.as_str()) {
                    " (required)"
                } else {
                    ""
                };
                println!("  --{key:<16} {ptype:<10} {pdesc}{req}");
            }
        }
    } else {
        eprintln!("Unknown tool: {tool_name}");
        eprintln!("Run `poly-cli tools` to see available tools.");
        std::process::exit(1);
    }

    Ok(())
}

// poly-cli is a user-facing CLI binary; println! is the production output channel.
#[allow(clippy::print_stdout)]
fn print_result_text(text: &str, fmt: OutputFormat) {
    match fmt {
        OutputFormat::Pretty => {
            // Try to parse as JSON and pretty-print
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                let pretty = serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| text.to_string());
                println!("{pretty}");
            } else {
                println!("{text}");
            }
        }
        OutputFormat::Json => {
            println!("{text}");
        }
    }
}

// poly-cli is a user-facing CLI binary; println! is the production output channel.
#[allow(clippy::print_stdout)]
fn print_value(value: &Value, fmt: OutputFormat) {
    match fmt {
        OutputFormat::Pretty => {
            println!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        }
        OutputFormat::Json => {
            let s = serde_json::to_string(value).unwrap_or_default();
            println!("{s}");
        }
    }
}
