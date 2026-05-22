//! Drag-and-drop overlay for the chat view.
//!
//! Single responsibility: render the full-screen drop-zone overlay that appears
//! when the user drags files over the chat. All drag state is owned by the
//! `render_chat_view_markup` caller — this module only renders the visual.

use dioxus::prelude::*;
use crate::i18n::t;

/// Renders the drag-over overlay (full-screen drop target visual).
///
/// Returns an empty element when `is_drag_over` is false so Dioxus can
/// short-circuit diffing the subtree on every non-drag render.
pub(super) fn render_drag_overlay(is_drag_over: bool) -> Element {
    if !is_drag_over {
        return rsx! {};
    }

    rsx! {
        div { class: "drag-overlay",
            div { class: "drag-overlay-content",
                span { class: "drag-icon", "📎" }
                p { "{t(\"chat-drop-files\")}" }
            }
        }
    }
}
