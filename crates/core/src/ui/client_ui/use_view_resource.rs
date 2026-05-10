//! `ViewQuery` trait + `use_view_resource` hook.
//!
//! These two items collapse the repeated boilerplate found in 15+ components:
//!
//! ```ignore
//! let res = {
//!     let account_id = account_id.clone();
//!     let channel_id = channel_id.clone();
//!     use_resource(move || {
//!         let account_id = account_id.clone();
//!         let channel_id = channel_id.clone();
//!         async move {
//!             client_manager.peek().with_backend(&account_id, async |b| {
//!                 b.some_method(&channel_id).await
//!             }).await
//!         }
//!     })
//! };
//! ```
//!
//! After migration:
//!
//! ```ignore
//! #[derive(Clone, PartialEq)]
//! struct ChannelViewQuery { account_id: String, channel_id: String }
//!
//! impl ViewQuery for ChannelViewQuery {
//!     type Output = ViewDescriptor;
//!     fn account_id(&self) -> &str { &self.account_id }
//!     async fn fetch(&self, b: &dyn IsBackend) -> ClientResult<Self::Output> {
//!         b.get_channel_view(&self.channel_id).await
//!     }
//! }
//!
//! let res = use_view_resource(ChannelViewQuery { account_id, channel_id });
//! ```
//!
//! The hook handles backend resolution, timeout, and reactive re-fire via
//! `use_resource` (Dioxus tracks dependencies through signals captured in the
//! closure; the query struct is cloned into the async block so future
//! re-renders pass updated values if the component key changes).
//!
//! ## Hang-class notes
//!
//! - The hook uses `client_manager.peek()` (not `.read()`) so it does NOT
//!   subscribe the component to every `client_manager` write (hang class #7).
//! - `with_backend` internally calls `read_with_timeout(5s)` (hang class #4).
//! - Only `Clone + PartialEq + 'static` queries are accepted; the `PartialEq`
//!   bound enables future `use_reactive_effect`-style re-fire dedup (hang
//!   class #6).

use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use dioxus::prelude::*;
use poly_client::{ClientResult, IsBackend};

use std::future::Future;

/// A typed backend query keyed by per-component dependencies.
///
/// Implementors describe what to fetch and how the fetch is keyed; the
/// [`use_view_resource`] hook handles backend resolution, timeout, and
/// reactive-effect re-fire.
///
/// # Implementation contract
///
/// - `account_id()` must return the account whose backend handles the query.
/// - `fetch()` must call exactly **one** `ClientBackend` method and return its
///   result unchanged (no `unwrap_or_default`, no filtering, no `.ok()`
///   conversions — keep those at the call site so the hook stays transparent).
/// - The impl must be `Clone` (the hook clones into the async block) and
///   `PartialEq` (enables future Dioxus-level re-run dedup).
pub trait ViewQuery: Clone + PartialEq + 'static {
    /// The type returned by the backend method (without `Result` wrapping).
    type Output: Clone + 'static;

    /// The account whose backend handles this query.
    fn account_id(&self) -> &str;

    /// Run the actual call against the resolved backend.
    ///
    /// Returns `ClientResult<Self::Output>`. The hook propagates this result
    /// directly — callers match on `Some(Ok(_))` / `Some(Err(_))` / `None`
    /// (loading) as before.
    ///
    /// The explicit `'a` lifetime ties the returned future's lifetime to both
    /// the query reference (`&'a self`) and the backend reference
    /// (`b: &'a dyn IsBackend`). This is required by the RPITIT lowering in
    /// Rust 2024 when the async fn body borrows from either argument.
    fn fetch<'a>(
        &'a self,
        backend: &'a dyn IsBackend,
    ) -> impl Future<Output = ClientResult<Self::Output>> + 'a;
}

/// Create a [`Resource`] that resolves `Q` against the matching backend.
///
/// The returned [`Resource<ClientResult<Q::Output>>`] has the same semantics
/// as a hand-rolled `use_resource(move || async move { … })`:
///
/// - `None`            → still loading
/// - `Some(Ok(v))`    → success, `v: Q::Output`
/// - `Some(Err(e))`   → backend call failed, `e: ClientError`
///
/// The hook does NOT subscribe the component to `client_manager` writes — it
/// uses `.peek()` — so a backend commit after first render does NOT
/// automatically re-fire the resource. Sites that need that re-fire (e.g.
/// `ClientSidebar`) must continue to use a raw `use_resource` with an
/// explicit `client_manager.read()` inside the closure.
pub fn use_view_resource<Q: ViewQuery>(query: Q) -> Resource<ClientResult<Q::Output>> {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    use_resource(move || {
        let q = query.clone();
        async move {
            client_manager
                .peek()
                .with_backend(q.account_id(), async |b| q.fetch(b).await)
                .await
        }
    })
}
