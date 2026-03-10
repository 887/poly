//! Configuration: resolves the data directory from environment or defaults.

use std::path::PathBuf;

/// Resolved configuration for the memory MCP server.
pub struct Config {
    /// Root directory for all stored data.
    pub data_dir: PathBuf,
}

impl Config {
    /// Resolve configuration from environment variables and defaults.
    ///
    /// Priority:
    /// 1. `POLY_MEMORY_DIR` environment variable
    /// 2. `~/.poly-memory/` (Linux/macOS) or `%APPDATA%\poly-memory\` (Windows)
    pub fn resolve() -> anyhow::Result<Self> {
        let data_dir = if let Ok(dir) = std::env::var("POLY_MEMORY_DIR") {
            PathBuf::from(dir)
        } else {
            default_data_dir()?
        };
        Ok(Self { data_dir })
    }
}

/// Return the platform-default data directory.
fn default_data_dir() -> anyhow::Result<PathBuf> {
    let home = home_dir()?;
    Ok(home.join(".poly-memory"))
}

/// Resolve the home directory cross-platform.
fn home_dir() -> anyhow::Result<PathBuf> {
    // Try HOME (Unix), then USERPROFILE (Windows), then HOMEPATH.
    if let Ok(h) = std::env::var("HOME") {
        return Ok(PathBuf::from(h));
    }
    if let Ok(h) = std::env::var("USERPROFILE") {
        return Ok(PathBuf::from(h));
    }
    anyhow::bail!(
        "Cannot determine home directory. \
         Set POLY_MEMORY_DIR environment variable to specify the data directory."
    )
}
