//! Route-synced state container.
//!
//! Wraps a value that mirrors the URL and should be set exclusively from
//! `crate::ui::routes::sync_route_to_app_state`. The mutator is named
//! `_set_from_route_sync_only` (verbose, `#[doc(hidden)]`, `pub(crate)`) to
//! make accidental misuse glaringly obvious in code review and grep. A CI
//! check (`scripts/check_route_synced_writes.sh`) enforces that no file
//! outside `crate::ui::routes` calls it.
//!
//! Why not compiler-enforced? Rust `pub(in path)` requires `path` to be a
//! strict ancestor of the defining module. `state` is not an ancestor of
//! `ui::routes`. Putting the mutator in `ui::routes` itself would create a
//! `state -> ui` dependency cycle (state uses RouteSynced internally for
//! NavigationState fields). The verbose-name + CI-grep combination is the
//! pragmatic alternative.
//!
//! This guard prevents the pre-mutation-then-`nav.push` cascade that wedged
//! the WASM render loop on 2026-04-19 (friend-card click hang) — pre-mutation
//! plus `on_update`'s later write to the same field doubled every render
//! storm.
//!
//! Reads pass through `Deref`, so `state.nav.selected_channel`,
//! `state.nav.selected_channel.as_deref()`, `if let Some(_) =
//! state.nav.selected_channel.cloned()` and the rest of the existing read
//! sites keep working without an `()`-getter renaming pass.

use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RouteSynced<T>(T);

impl<T: Default> Default for RouteSynced<T> {
    fn default() -> Self {
        Self(T::default())
    }
}

impl<T> RouteSynced<T> {
    /// Wrap an initial value. Construction is inert; only mutation is gated.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// **Do not call from click handlers, sidebars, or any code outside
    /// `crate::ui::routes::sync_route_to_app_state`.** Pre-mutating route
    /// state and then calling `nav.push(...)` causes `on_update` to write the
    /// same field again, doubling every render storm and wedging the WASM
    /// scheduler (see friend-card hang 2026-04-19).
    ///
    /// To change route-derived state, call `nav.push(Route::…)` and let
    /// `on_update` write it. CI verifies this method is called only from
    /// `routes.rs`.
    #[doc(hidden)]
    pub(crate) fn _set_from_route_sync_only(&mut self, value: T) {
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
