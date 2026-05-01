//! CLI argument parser for test server binaries.

use std::path::PathBuf;

/// Command-line arguments shared by all test servers.
pub struct CliArgs {
    /// Port to bind on (0 = random free port).
    pub port: u16,
    /// Auto-seed demo data on startup.
    pub seed: bool,
    /// Enable verbose tracing output.
    pub verbose: bool,
    /// Wipe persisted auth + state for this backend before starting.
    pub reset: bool,
}

impl CliArgs {
    /// Parse from `std::env::args()`. Simple hand-rolled parser (no clap dependency).
    #[must_use]
    pub fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut port = 0u16;
        let mut seed = false;
        let mut verbose = false;
        let mut reset = false;

        let mut i = 1;
        while i < args.len() {
            match args.get(i).map(String::as_str) {
                Some("--port") => {
                    // lint-allow-unused: index walk over argv, max args.len() < usize::MAX
                    #[allow(clippy::arithmetic_side_effects)]
                    {
                        i += 1;
                    }
                    if let Some(p) = args.get(i) {
                        port = p.parse().unwrap_or(0);
                    }
                }
                Some("--seed") => seed = true,
                Some("--verbose" | "-v") => verbose = true,
                Some("--reset") => reset = true,
                _ => {}
            }
            // lint-allow-unused: index walk over argv, max args.len() < usize::MAX
            #[allow(clippy::arithmetic_side_effects)]
            {
                i += 1;
            }
        }

        Self {
            port,
            seed,
            verbose,
            reset,
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

    /// Directory where this backend persists state across restarts.
    ///
    /// `$POLY_TEST_DATA_DIR/{backend}/` if set, else
    /// `$XDG_DATA_HOME/poly/test-servers/{backend}/` or
    /// `~/.local/share/poly/test-servers/{backend}/`.
    pub fn persist_dir(&self, backend: &str) -> PathBuf {
        if let Ok(root) = std::env::var("POLY_TEST_DATA_DIR") {
            return PathBuf::from(root).join(backend);
        }
        let base = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .ok()
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".local/share"))
            })
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("poly/test-servers").join(backend)
    }

    /// Absolute path to the persisted auth-token file for this backend.
    #[must_use]
    pub fn auth_path(&self, backend: &str) -> PathBuf {
        self.persist_dir(backend).join("auth.json")
    }
}
