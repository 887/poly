use dioxus::prelude::*;
use super::super::signals::ChatViewSignals;

pub(in super::super) fn use_unread_marker_visibility_effect(signals: &ChatViewSignals) {
    let mut unread_marker_on_screen = signals.unread_marker_on_screen;
    let history_state = signals.history_state;

    use_effect(move || { // poly-lint: allow stale-effect-capture — Signal-only; subscribes to history_state Signal
        let unread_marker_id = history_state.read().unread_marker_message_id.clone();
        let unread_count = history_state.read().unread_count;

        // If no unread marker or no unread count, marker is not visible
        if unread_marker_id.is_none() || unread_count == 0 {
            unread_marker_on_screen.set(false);
            return;
        }

        // Check if the unread marker message element is visible in the viewport
        let marker_id = unread_marker_id.unwrap_or_default();
        let dom_id = format!("message-{marker_id}");
        let js = format!(
            "(() => {{ \
                const el = document.getElementById('{dom_id}'); \
                if (!el) {{ dioxus.send(false); return; }} \
                const rect = el.getBoundingClientRect(); \
                const isVisible = rect.top >= 0 && rect.bottom <= window.innerHeight; \
                dioxus.send(isVisible); \
            }})()"
        );
        let mut eval = document::eval(&js);
        spawn(async move {
            if let Ok(visible) = eval.recv::<bool>().await {
                unread_marker_on_screen.set(visible);
            }
        });
    });
}
