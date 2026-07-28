//! Region-scoped render contexts for the chat view.
//!
//! `ChatViewMarkupCtx` (`../markup_ctx.rs`) is a 74-field parameter object that
//! every render helper in this module takes **by value** and deep-clones before
//! delegating — 18 clone sites per render, each copying the message `Vec`, the
//! search-hit `Vec`, and every `Option<String>` in the bundle.  That is the
//! SOLID item-4 (ISP) / item-6 (no god-objects) violation recorded in
//! `docs/plans/plan-chat-view-ctx-split.md`.
//!
//! This module dissolves it region by region.  Each region gets a small context
//! struct carrying only the fields that region actually reads, and every
//! consumer takes it **by reference** — a by-value split would just reproduce
//! the bug with smaller structs.
//!
//! `ChatViewCore` is the one exception to the by-reference rule: it holds
//! nothing but `BatchedSignal` handles, so it is `Copy` and passing it around
//! costs a pointer-sized memcpy rather than a deep clone.
//!
//! | Region | Struct | Landed |
//! |--------|--------|--------|
//! | shared signal handles | `ChatViewCore` | Phase A |
//! | header | `HeaderCtx` | Phase B |

mod header;

pub(super) use header::{HeaderCtx, build_header_ctx};

use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use crate::state::{ChatLists, ChatViewState, NavState, UiLayout, UiOverlays, VoiceState};

use super::signals::ChatViewSignals;

/// The application-state signal handles every chat-view region needs.
///
/// Deliberately holds **only** `BatchedSignal` handles: no snapshots, no owned
/// `String`/`Vec` values.  `BatchedSignal<T>` is `Copy`, so this struct is too,
/// and a region function may take it by value without cloning any data.
/// Anything that owns data belongs in a region context, not here.
#[derive(Copy, Clone)]
pub(super) struct ChatViewCore {
    pub(super) nav: BatchedSignal<NavState>,
    pub(super) ui_layout: BatchedSignal<UiLayout>,
    pub(super) ui_overlays: BatchedSignal<UiOverlays>,
    pub(super) client_manager: BatchedSignal<ClientManager>,
    pub(super) chat_lists: BatchedSignal<ChatLists>,
    pub(super) chat_view_state: BatchedSignal<ChatViewState>,
    pub(super) voice_state: BatchedSignal<VoiceState>,
}

/// Copy the shared signal handles out of the signal bundle.
///
/// Performs no `.read()`: it moves handles, never values, so calling it does
/// not subscribe the calling component to anything.
pub(super) fn build_core(signals: &ChatViewSignals) -> ChatViewCore {
    ChatViewCore {
        nav: signals.nav,
        ui_layout: signals.ui_layout,
        ui_overlays: signals.ui_overlays,
        client_manager: signals.client_manager,
        chat_lists: signals.chat_lists,
        chat_view_state: signals.chat_view_state,
        voice_state: signals.voice_state,
    }
}

#[cfg(test)]
mod tests {
    use super::{BatchedSignal, ChatViewCore, NavState};

    /// `ChatViewCore` must stay handle-only. `Copy` is the type-level proof
    /// that passing it by value cannot deep-clone the message / search-hit
    /// `Vec`s the way `ChatViewMarkupCtx::clone` does. If someone adds an
    /// owned `String` or `Vec` field, `Copy` stops deriving and this fails to
    /// compile.
    const fn assert_core_is_copy<T: Copy>() {}
    const _: () = assert_core_is_copy::<ChatViewCore>();

    /// Seven signal handles and nothing else. A stray `String` or `Vec` field
    /// (24 bytes each) would break this, which is the point: the struct is the
    /// cheap-to-pass half of the split and must stay that way.
    #[test]
    fn core_stays_handle_sized() {
        let core_size = size_of::<ChatViewCore>();
        let handle_size = size_of::<BatchedSignal<NavState>>();
        assert_eq!(
            core_size,
            handle_size.saturating_mul(7),
            "ChatViewCore must hold exactly 7 signal handles and no owned data"
        );
    }
}
