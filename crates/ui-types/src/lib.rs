//! Shared UI primitive types for the Poly app.
//!
//! This crate is intentionally thin — no heavy dependencies.
//! It exists because `crates/ui-macros` is a proc-macro crate and cannot
//! export regular types or `macro_rules!` macros.

/// Classifies why an event handler on a UI element is intentionally passive.
///
/// # Rules
/// - Every variant describes a *structural* reason — something about the
///   element's role in the layout, not about its implementation status.
/// - "Not implemented yet" is NOT a valid reason. Remove the element instead.
/// - Adding a variant requires a doc comment explaining the structural contract
///   and goes through normal code review.
/// - Marked `#[non_exhaustive]` — match arms must include `_` or use
///   `if let`; downstream crates cannot construct variants directly.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiNoopReason {
    /// A drag-resize splitter or reorder handle. The actual interaction is
    /// delivered via `pointermove`/`pointerup` on the document root, not via
    /// `onclick` on this element.
    DragHandle,

    /// A read-only visual indicator (status dot, badge, presence ring) that
    /// reflects state but has no defined click action on this surface.
    ReadOnlyIndicator,

    /// A decorative icon or avatar rendered inside a parent row that owns the
    /// click target. Clicking the icon routes through the parent row's handler;
    /// this element does not independently handle clicks.
    DecorativeIcon,

    /// A layout spacer, separator, or divider with no interactive purpose.
    LayoutSpacer,

    /// An event barrier that exists solely to call `event.stop_propagation()`
    /// or `event.prevent_default()`. The element itself has no user-facing
    /// action; the barrier logic is encoded in the handler body elsewhere.
    EventBarrier,

    /// A progress spinner or loading indicator rendered while an async
    /// operation is in flight. Non-interactive during this state by design;
    /// replaced once the operation completes.
    ProgressIndicator,
}

/// Marks an event handler as intentionally passive.
///
/// The argument **must** be a [`UiNoopReason`] variant. Bare strings,
/// integers, and missing arguments are compile errors — use the enum.
///
/// Do **not** add a `UiNoopReason` variant for an unimplemented feature.
/// Remove the UI element instead until the feature is ready.
///
/// # Example
/// ```rust
/// use poly_ui_types::{ui_noop, UiNoopReason};
///
/// // Resize splitter — drag is via pointermove on the document, click is unreachable
/// // onclick: move |_| ui_noop!(UiNoopReason::DragHandle),
///
/// // Status dot — read-only indicator, no click action defined
/// // onclick: move |_| ui_noop!(UiNoopReason::ReadOnlyIndicator),
/// ```
#[macro_export]
macro_rules! ui_noop {
    ($reason:expr) => {{
        // Type-checks that $reason is a UiNoopReason at compile time.
        // Zero runtime cost — fully eliminated by the optimizer.
        fn _assert_ui_noop_reason(_: $crate::UiNoopReason) {}
        _assert_ui_noop_reason($reason);
    }};
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn all_variants_are_debug() {
        let variants = [
            UiNoopReason::DragHandle,
            UiNoopReason::ReadOnlyIndicator,
            UiNoopReason::DecorativeIcon,
            UiNoopReason::LayoutSpacer,
            UiNoopReason::EventBarrier,
            UiNoopReason::ProgressIndicator,
        ];
        for v in variants {
            let s = format!("{v:?}");
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn macro_accepts_valid_reason() {
        // Should compile and be a no-op at runtime
        ui_noop!(UiNoopReason::DragHandle);
        ui_noop!(UiNoopReason::ReadOnlyIndicator);
        ui_noop!(UiNoopReason::DecorativeIcon);
        ui_noop!(UiNoopReason::LayoutSpacer);
        ui_noop!(UiNoopReason::EventBarrier);
        ui_noop!(UiNoopReason::ProgressIndicator);
    }
}
