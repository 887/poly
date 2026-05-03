//! File system and code repository types.

use serde::{Deserialize, Serialize};

/// Kind of a file system entry returned by [`crate::ClientBackend::list_files`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
    /// Git submodule pointer.
    Submodule,
}

/// One entry in a directory listing returned by [`crate::ClientBackend::list_files`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Repository-relative path of the entry (e.g. `"src/lib.rs"`).
    pub path: String,
    /// Display name (basename) of the entry.
    pub name: String,
    /// Kind of entry — file, directory, symlink, submodule.
    pub kind: FileKind,
    /// File size in bytes. `0` for directories.
    pub size: u64,
}

/// Raw file content returned by [`crate::ClientBackend::read_file`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileContent {
    /// Repository-relative path of the file.
    pub path: String,
    /// Raw file bytes (may be binary; UI decodes as needed).
    pub bytes: Vec<u8>,
    /// Whether the response was truncated by a backend size limit.
    pub truncated: bool,
}

/// Output of a host-mediated subprocess invocation made by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecOutput {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured stdout bytes.
    pub stdout: Vec<u8>,
    /// Captured stderr bytes.
    pub stderr: Vec<u8>,
}
