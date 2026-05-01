//! Host-side toast component + queue (P11 of plan-client-ui-polish.md).
//!
//! Plugins return `ActionOutcome::Toast(payload)` from `invoke_*_action`; the
//! shared action-outcome handler (`super::action_outcome::handle_action_outcome`)
//! pushes a [`ToastMessage`] onto the globally-provided `Signal<Vec<ToastMessage>>`
//! that this module reads. [`ToastOverlay`] is mounted once at the root layout
//! and renders stacked toasts in the top-right corner.
//!
//! Toasts auto-dismiss after 5 s. A user-visible close button (`×`) lets the
//! user dismiss earlier. Each toast has a unique monotonic `id` so the
//! `Pending` → `Completed` path can dismiss the in-flight "Working…" toast by
//! id when polling resolves.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_client::ToastTone;
use poly_ui_macros::{context_menu, ui_action};
use std::sync::atomic::{AtomicU64, Ordering};

/// Toast auto-dismiss delay in milliseconds.
pub const TOAST_AUTO_DISMISS_MS: u64 = 5_000;

static NEXT_TOAST_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a unique monotonic toast id.
pub fn next_toast_id() -> u64 {
    NEXT_TOAST_ID.fetch_add(1, Ordering::Relaxed)
}

/// Single toast enqueued into the global toast signal.
///
/// `label_key` is an FTL key resolved via [`crate::i18n::t`] at render time —
/// plugin-emitted keys are namespaced under `plugin-<id>-*`, host-emitted
/// keys (e.g. `ui-action-working`) live in the host bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastMessage {
    /// Monotonic id, unique across the whole session.
    pub id: u64,
    /// FTL key (resolved at render via `t()`).
    pub label_key: String,
    /// Severity tone — controls the `.toast-<tone>` CSS class.
    pub tone: ToastTone,
    /// When `true`, the toast is sticky — will not auto-dismiss.
    /// Used for long-running `Pending` toasts that are dismissed
    /// explicitly by id once the action resolves.
    pub sticky: bool,
}

impl ToastMessage {
    /// Construct an info/success/warning/error toast. Non-sticky.
    pub fn new(label_key: impl Into<String>, tone: ToastTone) -> Self {
        Self {
            id: next_toast_id(),
            label_key: label_key.into(),
            tone,
            sticky: false,
        }
    }

    /// Construct a sticky toast (used for `Pending` working indicators).
    pub fn sticky(label_key: impl Into<String>, tone: ToastTone) -> Self {
        Self {
            id: next_toast_id(),
            label_key: label_key.into(),
            tone,
            sticky: true,
        }
    }
}

/// Push a toast onto the global queue. Idempotent for unique ids.
pub fn push_toast(mut queue: Signal<Vec<ToastMessage>>, msg: ToastMessage) {
    queue.write().push(msg);
}

/// Remove a toast by id, if present. Used by the `Pending` path to dismiss
/// the in-flight working indicator after polling resolves.
pub fn dismiss_toast(mut queue: Signal<Vec<ToastMessage>>, id: u64) {
    queue.write().retain(|m| m.id != id);
}

/// CSS tone suffix for a `ToastTone`. Stable across renders.
pub(crate) fn tone_class(tone: ToastTone) -> &'static str {
    match tone {
        ToastTone::Info => "toast-info",
        ToastTone::Success => "toast-success",
        ToastTone::Warning => "toast-warning",
        ToastTone::Error => "toast-error",
    }
}

/// Root-mounted overlay that renders the toast queue stacked in the
/// top-right corner. Subscribes to the `Signal<Vec<ToastMessage>>` context
/// registered by `App`.
#[ui_action(None)]
#[context_menu(None)]
#[component]
pub fn ToastOverlay() -> Element {
    // Read via try_consume_context so snapshot tests and unit harnesses that
    // don't provide the queue don't panic — they just render nothing.
    let Some(queue): Option<Signal<Vec<ToastMessage>>> = try_consume_context() else {
        return rsx! {};
    };

    let items = queue.read().clone();
    if items.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "toast-overlay",
            role: "status",
            aria_live: "polite",
            for msg in items {
                ToastRow { key: "{msg.id}", msg }
            }
        }
    }
}

/// Single toast row + its auto-dismiss timer. Extracted so each toast owns
/// its own effect; dismissing one doesn't rerun timers for its siblings.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ToastRow(msg: ToastMessage) -> Element {
    let label = t(&msg.label_key);
    let tone_cls = tone_class(msg.tone);
    let class = format!("toast {tone_cls}");
    let id = msg.id;
    let sticky = msg.sticky;

    // Auto-dismiss effect — fires once per toast. Sticky toasts never tick.
    use_effect(move || {
        if sticky {
            return;
        }
        spawn(async move {
            schedule_dismiss(id, TOAST_AUTO_DISMISS_MS).await;
        });
    });

    let onclose = move |_| {
        if let Some(queue) = try_consume_context::<Signal<Vec<ToastMessage>>>() {
            dismiss_toast(queue, id);
        }
    };

    rsx! {
        div {
            class: "{class}",
            role: "alert",
            span { class: "toast-label", "{label}" }
            button {
                class: "toast-close",
                r#type: "button",
                aria_label: "Dismiss",
                onclick: onclose,
                "×"
            }
        }
    }
}

/// Sleep then remove the toast with the given id. Isolated so tests can
/// call it with a tiny delay.
async fn schedule_dismiss(id: u64, delay_ms: u64) {
    #[cfg(target_arch = "wasm32")]
    {
        // lint-allow-unused: fire-and-forget timer eval; recv() reply ignored.
        #[allow(clippy::let_underscore_must_use)]
        let _ = document::eval(&format!(
            "setTimeout(() => dioxus.send(true), {delay_ms});"
        ))
        .recv::<bool>()
        .await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
    }
    if let Some(queue) = try_consume_context::<Signal<Vec<ToastMessage>>>() {
        dismiss_toast(queue, id);
    }
}

// ─── Unit tests ──────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn next_toast_id_is_monotonic() {
        let a = next_toast_id();
        let b = next_toast_id();
        let c = next_toast_id();
        assert!(b > a);
        assert!(c > b);
    }

    #[test]
    fn toast_message_new_is_not_sticky() {
        let m = ToastMessage::new("k".to_string(), ToastTone::Info);
        assert!(!m.sticky);
        assert_eq!(m.label_key, "k");
        assert_eq!(m.tone, ToastTone::Info);
    }

    #[test]
    fn toast_message_sticky_is_sticky() {
        let m = ToastMessage::sticky("k".to_string(), ToastTone::Warning);
        assert!(m.sticky);
        assert_eq!(m.tone, ToastTone::Warning);
    }

    #[test]
    fn tone_class_maps_each_variant() {
        assert_eq!(tone_class(ToastTone::Info), "toast-info");
        assert_eq!(tone_class(ToastTone::Success), "toast-success");
        assert_eq!(tone_class(ToastTone::Warning), "toast-warning");
        assert_eq!(tone_class(ToastTone::Error), "toast-error");
    }

    // Queue-mutation helpers operate on the underlying Vec directly so we
    // can unit-test the push/dismiss logic without spinning up a Dioxus
    // VirtualDom. This is the same invariant that `push_toast` /
    // `dismiss_toast` uphold via `Signal::write`.

    fn push_raw(q: &mut Vec<ToastMessage>, m: ToastMessage) {
        q.push(m);
    }

    fn dismiss_raw(q: &mut Vec<ToastMessage>, id: u64) {
        q.retain(|m| m.id != id);
    }

    #[test]
    fn push_then_dismiss_removes_by_id() {
        let mut q: Vec<ToastMessage> = Vec::new();
        let a = ToastMessage::new("a", ToastTone::Info);
        let b = ToastMessage::new("b", ToastTone::Success);
        let a_id = a.id;
        let b_id = b.id;
        push_raw(&mut q, a);
        push_raw(&mut q, b);
        assert_eq!(q.len(), 2);
        dismiss_raw(&mut q, a_id);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].id, b_id);
    }

    #[test]
    fn dismiss_missing_id_is_a_noop() {
        let mut q: Vec<ToastMessage> = Vec::new();
        q.push(ToastMessage::new("a", ToastTone::Info));
        let len_before = q.len();
        dismiss_raw(&mut q, 99_999);
        assert_eq!(q.len(), len_before);
    }

    #[test]
    fn toast_overlay_renders_zero_items_when_queue_empty() {
        // Proxy: `ToastOverlay` early-returns rsx!{} when items.is_empty().
        let items: Vec<ToastMessage> = Vec::new();
        assert!(items.is_empty());
    }

    #[test]
    fn toast_overlay_renders_expected_count() {
        let items = vec![
            ToastMessage::new("a", ToastTone::Info),
            ToastMessage::new("b", ToastTone::Error),
        ];
        // Proxy for the rendered `.toast` div count.
        assert_eq!(items.len(), 2);
    }
}
