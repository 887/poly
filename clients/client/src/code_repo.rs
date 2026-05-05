//! `CodeRepoBackend` capability sub-trait (Phase H.2.a).
//!
//! Carved out of [`ClientBackend`] in Phase H.2.a.  Implemented by backends
//! that expose code-repository channels (`ChannelType::Code`): currently
//! `poly-github` and `poly-forgejo`.
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(cr) = backend.as_code_repo() {
//!     let entries = cr.list_files(&channel_id, "").await?;
//!     // …
//! }
//! ```
//!
//! WIT interface: `poly:messenger/messenger-client` — `list-files` and
//! `read-file` functions.
//!
//! [`ClientBackend`]: crate::ClientBackend

use async_trait::async_trait;

use crate::{ClientResult, FileContent, FileEntry};

/// Capability sub-trait for code-repository channel operations.
///
/// Mirrors the `list-files` / `read-file` functions from the
/// `poly:messenger/messenger-client` WIT interface.
///
/// No default impls: presence of `impl CodeRepoBackend` is the opt-in signal.
/// Backends that do not support code channels leave
/// [`IsBackend::as_code_repo`] returning `None` (the default).
///
/// [`IsBackend::as_code_repo`]: crate::IsBackend::as_code_repo
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait CodeRepoBackend: Send + Sync {
    /// List entries at the given path within a code-type channel.
    ///
    /// `path` is repo-relative; an empty string means the repo root.
    async fn list_files(&self, channel_id: &str, path: &str) -> ClientResult<Vec<FileEntry>>;

    /// Read the raw bytes of a file in a code-type channel.
    async fn read_file(&self, channel_id: &str, path: &str) -> ClientResult<FileContent>;
}
