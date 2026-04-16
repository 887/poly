//! Single workspace walk shared by every scan — .gitignore-aware,
//! skips target/, limits to *.rs.

use std::path::{Path, PathBuf};

pub struct WorkspaceWalker {
    pub files: Vec<PathBuf>,
    pub root: PathBuf,
}

impl WorkspaceWalker {
    pub fn new(root: &Path) -> Self {
        let mut files = Vec::new();
        let walker = ignore::WalkBuilder::new(root)
            .follow_links(false)
            .standard_filters(true)
            .build();
        for entry in walker.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // Skip the lint-gate crate itself — it contains no lint violations
            // to check, and its build.rs runs inside it.
            if p.components().any(|c| c.as_os_str() == "lint-gate") {
                continue;
            }
            files.push(p.to_path_buf());
        }
        Self {
            files,
            root: root.to_path_buf(),
        }
    }

    pub fn relative(&self, p: &Path) -> String {
        p.strip_prefix(&self.root)
            .unwrap_or(p)
            .to_string_lossy()
            .into_owned()
    }
}
