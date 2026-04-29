//! Shared error-display helpers for UI surfaces.
//!
//! Provides:
//! - [`is_session_expired`] — classify a `ClientError` as an expired/invalid session.
//! - [`SessionExpiredCard`] — component that renders a "Session expired" card with a
//!   "Re-authenticate" button routing to `Route::ReauthAccount`.

use crate::i18n::t_args;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::ClientError;
use poly_ui_macros::{context_menu, ui_action};

use crate::ui::actions::{ActionCx, UiAction};

/// Actions for the session-expired error card.
#[derive(Debug, Clone)]
pub enum SessionExpiredCardAction {
    /// User clicked the "Re-authenticate" button.
    Reauth,
}

impl UiAction for SessionExpiredCardAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // Navigation is handled inline via crate::nav!; this enum exists only
        // to satisfy the action-coverage lint.
    }
}

/// Returns `true` when `err` indicates that the user's session token has expired
/// or been invalidated by the backend.
///
/// Matches:
/// - `ClientError::AuthFailed` (any message)
/// - `ClientError::PermissionDenied` whose message contains `"401"`,
///   `"unauthorized"`, `"token"`, or `"expired"` (case-insensitive)
pub fn is_session_expired(err: &ClientError) -> bool {
    match err {
        ClientError::AuthFailed(_) => true,
        ClientError::PermissionDenied(msg) => {
            let lower = msg.to_lowercase();
            lower.contains("401")
                || lower.contains("unauthorized")
                || lower.contains("token")
                || lower.contains("expired")
        }
        _ => false,
    }
}

/// Card shown when a backend operation fails due to an expired or invalidated
/// session token.
///
/// Renders:
/// - Title: "🔐 Session expired" (FTL key `error-session-expired-title`)
/// - Body: "Your {backend} session has expired. Sign in again to continue."
///   (FTL key `error-session-expired-body`)
/// - Button: "Re-authenticate" (FTL key `error-session-expired-action`)
///   → routes to `Route::ReauthAccount { backend, instance_id, account_id }`
#[ui_action(SessionExpiredCardAction)]
#[context_menu(None)]
#[component]
pub fn SessionExpiredCard(
    backend: String,
    instance_id: String,
    account_id: String,
    /// Display name for the backend, shown in the body text.
    /// Pass the backend slug or a human-readable name.
    #[props(default = String::new())]
    backend_display_name: String,
) -> Element {
    let display = if backend_display_name.is_empty() {
        backend.clone()
    } else {
        backend_display_name.clone()
    };

    let title = t_args("error-session-expired-title", &[]);
    let body = t_args("error-session-expired-body", &[("backend", &display)]);
    let action_label = t_args("error-session-expired-action", &[]);

    let backend_for_nav = backend.clone();
    let instance_id_for_nav = instance_id.clone();
    let account_id_for_nav = account_id.clone();

    rsx! {
        div { class: "session-expired-card",
            div { class: "session-expired-card-icon", "🔐" }
            h3 { class: "session-expired-card-title", "{title}" }
            p { class: "session-expired-card-body", "{body}" }
            button {
                class: "session-expired-card-action btn-primary",
                r#type: "button",
                onclick: move |_| {
                    crate::nav!(Route::ReauthAccount {
                        backend: backend_for_nav.clone(),
                        instance_id: instance_id_for_nav.clone(),
                        account_id: account_id_for_nav.clone(),
                    });
                },
                "{action_label}"
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn auth_failed_is_session_expired() {
        assert!(is_session_expired(&ClientError::AuthFailed("401 Unauthorized".into())));
        assert!(is_session_expired(&ClientError::AuthFailed("token revoked".into())));
        assert!(is_session_expired(&ClientError::AuthFailed(String::new())));
    }

    #[test]
    fn permission_denied_with_token_keyword_is_expired() {
        assert!(is_session_expired(&ClientError::PermissionDenied(
            "invalid token".into()
        )));
        assert!(is_session_expired(&ClientError::PermissionDenied(
            "session expired".into()
        )));
        assert!(is_session_expired(&ClientError::PermissionDenied(
            "401 Forbidden".into()
        )));
        assert!(is_session_expired(&ClientError::PermissionDenied(
            "Unauthorized access".into()
        )));
    }

    #[test]
    fn permission_denied_without_auth_keyword_is_not_expired() {
        assert!(!is_session_expired(&ClientError::PermissionDenied(
            "you cannot delete this resource".into()
        )));
        assert!(!is_session_expired(&ClientError::PermissionDenied(
            "moderator only".into()
        )));
    }

    #[test]
    fn other_errors_are_not_session_expired() {
        assert!(!is_session_expired(&ClientError::NotFound("channel".into())));
        assert!(!is_session_expired(&ClientError::Network("timeout".into())));
        assert!(!is_session_expired(&ClientError::Internal("oops".into())));
        assert!(!is_session_expired(&ClientError::NotSupported("x".into())));
        assert!(!is_session_expired(&ClientError::RateLimited {
            retry_after_ms: 5000
        }));
    }
}
