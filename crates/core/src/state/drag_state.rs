//! Drag state slice — transient HTML5 drag-and-drop state.
//!
//! Extracted from `ChatData` so that drag-event spam during `ondragover`
//! only re-renders drag-watching components rather than the entire chat
//! list. Provided alongside `BatchedSignal<ChatData>` at the `App` level
//! (see `crates/core/src/ui.rs`).
//!
//! # Hang-class notes
//! - Use `.batch(|d| …)` for writes (hang class #1: no raw `Signal::write()`).
//! - Use `.peek()` for one-shot snapshots that must not subscribe the parent.
//! - Reset all fields at once via `.batch(|d| { *d = DragState::default(); })`.

/// Source of the current HTML5 drag operation.
///
/// Distinguishes what kind of element started the drag so drop handlers
/// can apply the correct reorder or add-to-favorites logic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DragSource {
    /// No drag in progress.
    #[default]
    None,
    /// Dragging a favorite server icon in Bar 1 (reorder within favorites).
    FavoriteServer,
    /// Dragging an account icon in Bar 1 (reorder within accounts).
    AccountIcon,
    /// Dragging a server icon in Bar 2 AccountServerBar
    /// (reorder within Bar 2, or drop onto Bar 1 to favorite).
    AccountServer,
}

/// Reactive slice — drag state for the currently in-flight HTML5 drag.
///
/// Held as `BatchedSignal<DragState>` so drag-event spam during dragover
/// doesn't churn the chat list. Reset to defaults on drop / dragend.
#[derive(Debug, Clone, Default)]
pub struct DragState {
    /// Server ID currently being dragged (set on dragstart, cleared on drop/dragend).
    ///
    /// Used to pass drag state from Bar 2 (Account Server Bar) to Bar 1 (Favorites Bar)
    /// without needing browser DataTransfer API access.
    pub dragging_server_id: Option<String>,
    /// Source of the current drag operation.
    pub drag_source: DragSource,
    /// ID of the element currently being hovered over as a drop target.
    ///
    /// Set on `ondragover` of individual items so the parent can determine
    /// where to insert the dragged item on `ondrop`.
    pub drag_over_id: Option<String>,
}
