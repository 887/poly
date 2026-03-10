//! Configuration: resolves the data directory from environment or defaults.

use std::path::{Path, PathBuf};

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
///
/// Preference order when resolving defaults (unless overridden by POLY_MEMORY_DIR):
/// 1. A repository-local `./.poly-memory/` when running from a workspace root (detects
///    `Cargo.toml` or `.git` in the current working directory).
/// 2. Fallback to the user's home directory `~/.poly-memory/`.
fn default_data_dir() -> anyhow::Result<PathBuf> {
    // Prefer a repository-local `.poly-memory` when the process is running from
    // anywhere inside the repository (for example when launched by VS Code or via
    // other tools). To be robust against differing working directories, search
    // upward from both the current working directory and the running executable
    // path for a repository root marker (`Cargo.toml` or `.git`). If found, use
    // `<repo>/.poly-memory` as the default. Otherwise fall back to `~/.poly-memory`.
    fn find_repo_root(start: &Path) -> Option<PathBuf> {
        let mut p: Option<PathBuf> = Some(start.to_path_buf());
        while let Some(cur) = p {
            if cur.join("Cargo.toml").exists() || cur.join(".git").exists() {
                return Some(cur);
            }
            p = cur.parent().map(|d| d.to_path_buf());
        }
        None
    }

    // Check current working directory first
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(root) = find_repo_root(&cwd) {
            return Ok(root.join(".poly-memory"));
        }
    }

    // Then check the executable's parent directory (covers some launcher cases)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exedir) = exe.parent() {
            if let Some(root) = find_repo_root(exedir) {
                return Ok(root.join(".poly-memory"));
            }
        }
    }

    // Otherwise fall back to the conventional home-location.
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
