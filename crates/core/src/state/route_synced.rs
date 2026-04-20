//! Route-synced state container.
//!
//! Wraps a value that mirrors the URL. Mutation is gated to
//! `crate::ui::routes::sync_route_to_app_state` via the `RouteSyncedWrite`
//! trait defined in `crate::ui::routes::internal` — a `use` of that trait
//! outside `crate::ui::routes::*` does not compile, so the privileged
//! `.set(...)` path is compiler-enforced. This prevents the
//! pre-mutation-then-`nav.push` cascade that wedged the WASM render loop
//! on 2026-04-19 (friend-card click hang): pre-mutation plus `on_update`'s
//! later write to the same field doubled every render storm.
//!
//! Why this works when `pub(in crate::ui::routes) fn set` did not:
//! `pub(in path)` requires `path` to be an ancestor of the defining
//! module. The trait's defining module IS `crate::ui::routes::internal`,
//! which IS a descendant of `crate::ui::routes`, so the ancestor rule is
//! satisfied — the visibility gate lives on the trait, not on the struct.
//!
//! Reads pass through `Deref`, so `state.nav.selected_channel`,
//! `state.nav.selected_channel.as_deref()`, `state.nav.selected_channel.cloned()`
//! and every other existing read site keeps working without renames.
//!
//! ## Escape hatch — `unsafe_presync_override`
//!
//! Four verified call sites (`voice_view.rs:257`,
//! `favorites_sidebar.rs:1146/1164/1302`) need to synchronously pre-set
//! `selected_channel` to prevent `ChatView` rendering against a dead
//! channel id during the URL/state race window. They use
//! [`RouteSynced::unsafe_presync_override`], which is intentionally
//! loud-named and requires a `&'static str` reason so a reviewer can
//! `rg unsafe_presync_override` and audit every holdout in one sitting.

use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RouteSynced<T>(
    /// Crate-internal so the privileged `RouteSyncedWrite` trait impl
    /// (defined in `crate::ui::routes::internal`) can assign. External
    /// crates only see `Deref`/`AsRef`.
    pub(crate) T,
);

impl<T: Default> Default for RouteSynced<T> {
    fn default() -> Self {
        Self(T::default())
    }
}

impl<T> RouteSynced<T> {
    /// Wrap an initial value. Construction is inert; mutation is gated.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Escape hatch for the four verified holdouts that must synchronously
    /// pre-set the value before `ChatView` renders (otherwise a URL/state
    /// race flashes the dead channel). **Do not add new call sites
    /// without discussing with a reviewer.** The `_reason` literal
    /// exists so `rg unsafe_presync_override` shows the justification
    /// for every hit; use `rg -A 1 unsafe_presync_override` to audit.
    ///
    /// If your call is followed by `nav.push` / `nav.replace` that
    /// targets the same field, delete this call — let the route be the
    /// single source of truth (that's the bug class this whole module
    /// exists to prevent).
    pub(crate) fn unsafe_presync_override(&mut self, value: T, _reason: &'static str) {
        self.0 = value;
    }
}

impl<T: Clone> RouteSynced<T> {
    /// Clone the inner value (not the wrapper). `nav.selected_channel.clone()`
    /// resolves to `RouteSynced::clone` (returning a `RouteSynced<T>`); when
    /// you actually want `T` — e.g. for `?` on an `Option<String>` — call
    /// `.cloned()` instead.
    pub fn cloned(&self) -> T {
        self.0.clone()
    }
}

impl<T> Deref for RouteSynced<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> AsRef<T> for RouteSynced<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> From<T> for RouteSynced<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}
