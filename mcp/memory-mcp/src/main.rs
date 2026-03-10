//! # poly-memory-mcp
//!
//! **Persistent task list + memory + knowledge-base** MCP/CLI server for Poly agents.
//!
//! ## Data Layout (on disk)
//!
//! All data lives under `POLY_MEMORY_DIR` (default: `~/.poly-memory/`):
//!
//! ```text
//! ~/.poly-memory/
//! ├── poly-memory.json    # global counter: { "next_id": N }
//! ├── knowledge/          # general knowledge base
//! │   └── <topic>.md
//! └── tasks/
//!     ├── 001_my_task_title.json          # per-task metadata + checklist
//!     ├── 001-my-task-title/              # per-task findings + memories
//!     │   ├── findings.md                 # accumulated research (append-only)
//!     │   └── memories/
//!     │       └── <timestamp>-<slug>.md
//!     └── 002_another_task.json
//!         002-another-task/
//!         ...
//! ```
//!
//! **Migration:** If a legacy monolithic `tasks.json` is present it is
//! automatically migrated to the per-task file layout on first load and
//! renamed to `tasks.json.bak`.
//!
//! ## Modes
//!
//! - **MCP** (default): JSON-RPC 2.0 over stdio — used by GitHub Copilot / VS Code.
//! - **CLI** (preferred): direct command invocation from shell or VS Code tasks.
//!
//! **Prefer CLI over MCP access.** CLI is faster, easier to script, and testable
//! without an MCP client. See `--help` or the README for CLI commands.
//!
//! ## Usage
//!
//! ```bash
//! # MCP mode (stdio, for VS Code mcp.json)
//! cargo run --bin poly-memory-mcp
//!
//! # CLI mode (preferred for shell/agent scripts)
//! cargo run --bin poly-memory-mcp -- tasks list
//! cargo run --bin poly-memory-mcp -- tasks create "Add feature X"
//! cargo run --bin poly-memory-mcp -- tasks get 1
//! cargo run --bin poly-memory-mcp -- finding store 1 "Key finding: ..."
//! cargo run --bin poly-memory-mcp -- memory list 1
//! cargo run --bin poly-memory-mcp -- knowledge store "dioxus-routing" "..."
//! cargo run --bin poly-memory-mcp -- work --count 3
//! ```

mod cli;
mod config;
mod mcp;
mod ops;
mod store;
mod types;

use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let config = Config::resolve()?;

    // First non-binary arg that isn't a flag selects the mode.
    // Any subcommand other than "mcp" triggers CLI mode.
    let mode_arg = args.get(1).map(String::as_str);
    match mode_arg {
        None | Some("mcp") | Some("--mcp") => {
            tracing::info!("Starting poly-memory-mcp in MCP/stdio mode");
            mcp::run_server(config.data_dir).await
        }
        Some("--help") | Some("-h") | Some("help") => cli::print_help().await,
        _ => {
            // All other subcommands go to CLI mode.
            // Skip the binary name (args[0]); pass remaining args.
            let cli_args = args.get(1..).unwrap_or(&[]).to_vec();
            cli::run(config.data_dir, &cli_args).await
        }
    }
}
