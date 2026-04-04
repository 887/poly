//! CLI argument parser for test server binaries.

/// Command-line arguments shared by all test servers.
pub struct CliArgs {
    /// Port to bind on (0 = random free port).
    pub port: u16,
    /// Auto-seed demo data on startup.
    pub seed: bool,
    /// Enable verbose tracing output.
    pub verbose: bool,
}

impl CliArgs {
    /// Parse from `std::env::args()`. Simple hand-rolled parser (no clap dependency).
    pub fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut port = 0u16;
        let mut seed = false;
        let mut verbose = false;

        let mut i = 1;
        while i < args.len() {
            match args.get(i).map(|s| s.as_str()) {
                Some("--port") => {
                    i += 1;
                    if let Some(p) = args.get(i) {
                        port = p.parse().unwrap_or(0);
                    }
                }
                Some("--seed") => seed = true,
                Some("--verbose") | Some("-v") => verbose = true,
                _ => {}
            }
            i += 1;
        }

        Self {
            port,
            seed,
            verbose,
        }
    }

    /// Initialize tracing based on verbose flag.
    pub fn init_tracing(&self) {
        use tracing_subscriber::EnvFilter;

        let filter = if self.verbose {
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("info")
        };

        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}
