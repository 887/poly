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

mod mcp_client;

use clap::{Parser, Subcommand};
use mcp_client::McpClient;
use serde_json::{Value, json};

#[derive(Parser)]
#[command(name = "poly-cli", about = "Poly MCP CLI — dynamic tool interface")]
struct Cli {
    /// MCP server URL (default: http://localhost:3010/mcp)
    #[arg(long, default_value = "http://localhost:3010/mcp")]
    url: String,

    /// Output format
    #[arg(long, default_value = "pretty")]
    format: OutputFormat,

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
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Check if the MCP server is reachable
    Health,
}

#[tokio::main]
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

            let arguments = parse_tool_args(&args)?;
            let result = client.call_tool(&tool, arguments).await?;

            // Extract the text content from MCP response
            let text = extract_tool_result(&result);
            match cli.format {
                OutputFormat::Pretty => {
                    // Try to parse as JSON and pretty-print
                    if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&parsed).unwrap_or(text)
                        );
                    } else {
                        println!("{text}");
                    }
                }
                OutputFormat::Json => {
                    println!("{text}");
                }
            }

            // Check for error
            if result
                .get("isError")
                .and_then(|e| e.as_bool())
                .unwrap_or(false)
            {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

/// Parse --key value pairs into a JSON object.
/// Values that look like JSON objects/arrays are parsed as JSON.
/// Boolean strings "true"/"false" become JSON booleans.
/// Numeric strings become JSON numbers.
fn parse_tool_args(args: &[String]) -> anyhow::Result<Value> {
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

    Ok(Value::Object(map))
}

/// Extract the text content from an MCP tool result.
fn extract_tool_result(result: &Value) -> String {
    if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
        content
            .iter()
            .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
}

/// Show the schema/help for a specific tool.
async fn show_tool_help(client: &McpClient, tool_name: &str) -> anyhow::Result<()> {
    let tools = client.list_tools().await?;
    let tool = tools
        .iter()
        .find(|t| t.get("name").and_then(|n| n.as_str()) == Some(tool_name));

    match tool {
        Some(t) => {
            let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            let desc = t
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            println!("Tool: {name}");
            println!("Description: {desc}");
            println!();

            if let Some(schema) = t.get("inputSchema") {
                if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
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
            }
        }
        None => {
            eprintln!("Unknown tool: {tool_name}");
            eprintln!("Run `poly-cli tools` to see available tools.");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn print_value(value: &Value, fmt: OutputFormat) {
    match fmt {
        OutputFormat::Pretty => {
            println!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(value).unwrap_or_default());
        }
    }
}
