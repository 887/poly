//! Baseline JSON — grandfathered violations that don't fail the build.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Violation {
    pub rule: String,
    pub path: String,
    pub line: u32,
    pub detail: String,
}

impl Violation {
    pub fn to_error_line(&self) -> String {
        format!(
            "[{rule}] {path}:{line}: {detail}",
            rule = self.rule,
            path = self.path,
            line = self.line,
            detail = self.detail,
        )
    }

    pub fn key(&self) -> (&str, &str, u32, &str) {
        (&self.rule, &self.path, self.line, &self.detail)
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Baseline {
    pub violations: Vec<Violation>,
}

impl Baseline {
    pub fn empty() -> Self {
        Self {
            violations: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                println!("cargo::warning=lint-gate: baseline.json parse failed: {e}");
                Self::empty()
            }),
            Err(_) => Self::empty(),
        }
    }

    pub fn save(&self, path: &Path) {
        let mut sorted = self.violations.clone();
        sorted.sort_by(|a, b| a.key().cmp(&b.key()));
        let Ok(json) = serde_json::to_string_pretty(&Self { violations: sorted }) else {
            println!("cargo::warning=lint-gate: failed to serialize baseline");
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path, json) {
            println!("cargo::warning=lint-gate: failed to write baseline: {e}");
        }
    }

    pub fn insert(&mut self, v: Violation) {
        if !self.contains(&v) {
            self.violations.push(v);
        }
    }

    pub fn contains(&self, v: &Violation) -> bool {
        self.violations
            .iter()
            .any(|existing| existing.key() == v.key())
    }
}
