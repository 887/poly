//! `DiscoverBackend` capability sub-trait (Phase H.4.c).
//!
//! Carved out of [`ClientBackend`] in Phase H.4.c.  Exposes community/server
//! discovery (search) operations that are only available on backends with
//! a searchable community index.
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(db) = backend.as_discover() {
//!     let page = db.search_communities(&query, scope, None).await?;
//! }
//! ```
//!
//! WIT note: `search-communities` is in `poly:messenger/messenger-client`.
//! Currently implemented by `poly-lemmy` and `poly-reddit`.
//!
//! [`ClientBackend`]: crate::ClientBackend

use async_trait::async_trait;

use crate::{ClientResult, CommunityPage, CommunityScope};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait DiscoverBackend: Send + Sync {
    /// Search for communities / subreddits matching `query`.
    ///
    /// `scope` is only meaningful for backends with
    /// [`CommunitySearchSupport::SubscribedLocalAll`] (Lemmy). Reddit ignores
    /// the scope and always searches across all of Reddit. `cursor` is the
    /// opaque pagination token returned by the previous call's
    /// `CommunityPage::next_cursor`; pass `None` for the first page.
    async fn search_communities(
        &self,
        query: &str,
        scope: CommunityScope,
        cursor: Option<String>,
    ) -> ClientResult<CommunityPage>;
}
