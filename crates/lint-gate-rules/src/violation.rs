//! Shared `Violation` type used by all scanners.

use serde::{Deserialize, Serialize};

/// A single lint violation found by a scanner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Violation {
    /// Lint rule identifier (e.g. `"allow_ban"`, `"forbid_signal_write"`).
    pub rule: String,
    /// Workspace-relative path to the offending file.
    pub path: String,
    /// 1-based line number of the violation.
    pub line: u32,
    /// Human-readable description of the violation.
    pub detail: String,
}

impl Violation {
    /// Format as a `cargo::error=` line for build-script output.
    #[must_use] 
    pub fn to_error_line(&self) -> String {
        format!(
            "[{rule}] {path}:{line}: {detail}",
            rule = self.rule,
            path = self.path,
            line = self.line,
            detail = self.detail,
        )
    }

    #[must_use] 
    pub fn key(&self) -> (&str, &str, u32, &str) {
        (&self.rule, &self.path, self.line, &self.detail)
    }
}
