use dioxus::prelude::*;
use super::super::signals::ChatViewSignals;

/// Auto-focus the message composer input whenever the selected channel or DM changes.
///
/// This gives the user immediate keyboard focus so they can start typing
/// right after clicking a channel or DM, matching Discord UX.
pub(in super::super) fn use_composer_focus_effect(signals: &ChatViewSignals) {
    let app_state = signals.app_state;
    use_effect(move || { // poly-lint: allow stale-effect-capture — Signal-only; subscribes to app_state Signal for channel/account changes
        // Depend on channel + active account so switching DMs also refocuses.
        let _channel = app_state.read().nav.selected_channel.clone();
        let _account = app_state.read().nav.active_account_id.clone();

        // Small delay so the composer DOM element is ready after route transition.
        let _ = document::eval(
            "setTimeout(() => { \
                const el = document.getElementById('poly-message-composer'); \
                if (el) el.focus(); \
            }, 80)",
        );
    });
}
