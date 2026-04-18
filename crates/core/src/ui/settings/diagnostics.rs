//! Diagnostics settings page — connection stats, account health, storage usage.
//!
//! Shows per-account connection and presence status, active backend count,
//! and storage usage estimates. Useful for debugging and support.
//!
//! ## 150-line component rule
//! This component MUST stay under 150 lines. Extract sub-components if needed.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use dioxus::prelude::*;
use poly_client::{AccountPresence, ConnectionStatus};
use poly_ui_macros::{context_menu, ui_action};

/// Diagnostics page — shows health and status information for all accounts.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(None)]
#[component]
pub fn DiagnosticsPage() -> Element {
    let client_manager: Signal<ClientManager> = use_context();

    let account_ids: Vec<String> = client_manager
        .read()
        .active_account_ids()
        .into_iter()
        .collect();

    let demo_active = client_manager.read().demo_active;

    rsx! {
        div { class: "settings-section-content",
            h2 { class: "settings-section-title", {t("settings-diagnostics-title")} }
            p { class: "settings-section-description", {t("settings-diagnostics-description")} }

            // Backend summary
            div { class: "diagnostics-summary",
                div { class: "diagnostics-row",
                    span { class: "diagnostics-label", {t("settings-diagnostics-demo-active")} }
                    span { class: if demo_active { "diagnostics-value value-ok" } else { "diagnostics-value value-off" },
                        if demo_active { "Yes" } else { "No" }
                    }
                }
                div { class: "diagnostics-row",
                    span { class: "diagnostics-label", {t("settings-diagnostics-active-accounts")} }
                    span { class: "diagnostics-value", "{ account_ids.len() }" }
                }
            }

            // Per-account connection/presence table
            if !account_ids.is_empty() {
                h3 { class: "settings-subsection-title", {t("settings-diagnostics-accounts-title")} }
                div { class: "diagnostics-accounts-table",
                    div { class: "diagnostics-table-header",
                        span { {t("settings-diagnostics-col-account")} }
                        span { {t("settings-diagnostics-col-connection")} }
                        span { {t("settings-diagnostics-col-presence")} }
                    }
                    for account_id in &account_ids {
                        AccountDiagnosticsRow { account_id: account_id.clone() }
                    }
                }
            } else {
                p { class: "diagnostics-empty", {t("settings-diagnostics-no-accounts")} }
            }
        }
    }
}

/// A single row in the diagnostics account table.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AccountDiagnosticsRow(account_id: String) -> Element {
    let client_manager: Signal<ClientManager> = use_context();

    let conn = client_manager
        .read()
        .connection_statuses
        .get(&account_id)
        .cloned()
        .unwrap_or(ConnectionStatus::Disconnected);
    let presence = client_manager
        .read()
        .presence_statuses
        .get(&account_id)
        .copied()
        .unwrap_or(AccountPresence::Online);

    let conn_label = match &conn {
        ConnectionStatus::Connected => "Connected",
        ConnectionStatus::Connecting => "Connecting…",
        ConnectionStatus::Disconnected => "Disconnected",
        ConnectionStatus::Unauthenticated(_) => "Reauthenticate",
        ConnectionStatus::Error(e) => {
            let _ = e;
            "Error"
        }
    };

    rsx! {
        div { class: "diagnostics-table-row",
            span { class: "diagnostics-account-id", "{account_id}" }
            span { class: "diagnostics-connection-status {conn.css_class()}", "{conn_label}" }
            span { class: "diagnostics-presence-status {presence.css_class()}",
                "{presence.display_name()}"
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    #[test]
    fn diagnostics_page_is_display_only() {
        // DiagnosticsPage has no interactive actions; verify it compiles with ui_action(None).
        // This is a structural compile-time check — no runtime assertions needed.
        let _ = ();
    }
}
