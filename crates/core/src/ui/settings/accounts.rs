//! Accounts settings section.
//!
//! Lists all active messenger accounts with their display name, backend badge,
//! and a gear icon linking to /:backend/:instance_id/:account_id/settings.
//!
//! The "Add Account" button navigates to the plugin-driven routable signup flow
//! at `/signup`, where the user picks a backend and completes a full-page form.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::state::BatchedSignal;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the accounts settings section.
pub enum AccountsSettingsAction {
    /// Navigate to the signup/add-account flow.
    AddAccount,
    /// Navigate to the per-account settings page for a specific account.
    OpenAccountSettings(String),
}

impl UiAction for AccountsSettingsAction {
    fn apply(self, cx: ActionCx<'_>) {
        match self {
            Self::AddAccount => {
                if let Some(nav) = cx.navigator {
                    nav.push(Route::SignupPicker);
                }
            }
            Self::OpenAccountSettings(_account_id) => {
                // Requires backend slug + instance_id in addition to account_id —
                // those are only available in the component context. Kept for phase-E.
                todo!("phase-E: navigate to account settings")
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// `AddAccount` with no navigator should be a no-op (no panic).
    #[test]
    fn add_account_no_navigator_is_noop() {
        // navigator is None in test context — action must not panic
        AccountsSettingsAction::AddAccount.apply(crate::ui::actions::ActionCx::test_no_nav());
    }

    /// Structural test: all variants construct and the type implements UiAction.
    #[test]
    fn accounts_settings_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<AccountsSettingsAction>();
        let _ = AccountsSettingsAction::AddAccount;
        let _ = AccountsSettingsAction::OpenAccountSettings("acc123".into());
    }
}

/// Derive a stable hsl color from an account ID string (same as search.rs).
fn account_color(account_id: &str) -> String {
    let hash: u32 = account_id.bytes().fold(5381_u32, |h, b| {
        h.wrapping_mul(33).wrapping_add(u32::from(b))
    });
    let hue = hash % 360;
    format!("hsl({hue}, 65%, 55%)")
}

/// Emoji icon for a backend. TODO(polish-plan P54): replace with
/// plugin-declared `IconSource` once the settings row UI adopts the
/// new icon pipeline. Until then every account renders with a single
/// neutral fallback — the previous slug ladder violated WP 7's
/// plugin-declarative rule.
fn backend_emoji(_slug: &str) -> &'static str { "📡" }

/// A single row in the accounts list showing account icon, name, backend, and settings gear.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn AccountRow(
    account_id: String,
    display_name: String,
    backend_slug: String,
    backend_label: String,
    instance_id: String,
    icon_color: String,
) -> Element {
    let icon_char: String = display_name.chars().next().map_or_else(|| "?".to_string(), |c| c.to_uppercase().to_string());
    let emoji = backend_emoji(&backend_slug);
    rsx! {
        div { class: "accounts-settings-row",
            // Colored icon bubble
            div {
                class: "accounts-settings-icon",
                style: "background: {icon_color}",
                "{icon_char}"
            }
            // Name + backend label
            div { class: "accounts-settings-info",
                span { class: "accounts-settings-name", "{display_name}" }
                span { class: "accounts-settings-backend", "{emoji} {backend_label}" }
            }
            // Gear icon → account settings
            Link {
                to: Route::AccountSettingsRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                },
                class: "accounts-settings-gear",
                title: "{t(\"settings-account-settings-link\")}",
                "⚙"
            }
        }
    }
}

/// Accounts settings section.
///
/// Lists active messenger accounts grouped by backend and provides
/// an "Add Account" button that navigates to the plugin-driven signup
/// flow at `/signup`.
#[rustfmt::skip]
#[ui_action(AccountsSettingsAction)]
#[context_menu(none)]
#[component]
pub(super) fn AccountsSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let account_ids = client_manager.read().active_account_ids();

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-accounts\")}" }
            p { class: "settings-description", "{t(\"settings-accounts-description\")}" }

            if account_ids.is_empty() {
                p { class: "settings-empty-hint", "{t(\"settings-no-accounts\")}" }
            } else {
                div { class: "accounts-settings-list",
                    for account_id in &account_ids {
                        {
                            let aid = account_id.clone();
                            let cm = client_manager.read();
                            let session = cm.sessions.get(&aid);
                            let display_name = session.map_or_else(|| aid.clone(), |s| s.user.display_name.clone());
                            let backend_slug = session.map_or_else(|| "demo".to_string(), |s| s.backend.slug().to_string());
                            let backend_label = session.map_or_else(|| "Demo".to_string(), |s| s.backend.display_name().to_string());
                            let instance_id = session.map_or_else(|| "demo".to_string(), |s| s.instance_id.clone());
                            let icon_color = account_color(&aid);
                            rsx! {
                                AccountRow {
                                    key: "{aid}",
                                    account_id: aid,
                                    display_name,
                                    backend_slug,
                                    backend_label,
                                    instance_id,
                                    icon_color,
                                }
                            }
                        }
                    }
                }
            }

            // Navigate to the plugin-driven, routable signup picker at /signup.
            button {
                class: "btn btn-primary",
                onclick: move |_| { let _ = crate::nav!(Route::SignupPicker); },
                "{t(\"settings-add-account\")}"
            }
        }
    }
}
